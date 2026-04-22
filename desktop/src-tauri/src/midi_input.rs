//! Native MIDI input via midir. Emits `native-midi-chunk` events to the webview (same pattern as PCM).
//! One OS thread per active logical input; dropping the stop channel ends the stream.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Serialize)]
pub struct ListedMidiInput {
    pub index: usize,
    pub name: String,
}

struct ActiveStream {
    stop_tx: Sender<()>,
    join: Option<std::thread::JoinHandle<()>>,
}

pub struct MidiInputController {
    inner: Mutex<HashMap<String, ActiveStream>>,
}

impl Default for MidiInputController {
    fn default() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }
}

macro_rules! log_safe {
    ($($arg:tt)*) => {
        let _ = std::io::Write::write_fmt(
            &mut std::io::stderr(),
            format_args!("{}\n", format_args!($($arg)*)),
        );
    };
}

#[tauri::command]
pub fn list_midi_inputs() -> Result<Vec<ListedMidiInput>, String> {
    let midi_in =
        midir::MidiInput::new("ToneFrame MIDI list").map_err(|e| format!("MidiInput::new: {}", e))?;
    let ports = midi_in.ports();
    let mut out = Vec::new();
    for (i, port) in ports.iter().enumerate() {
        let name = midi_in
            .port_name(port)
            .unwrap_or_else(|_| format!("MIDI input {}", i));
        out.push(ListedMidiInput { index: i, name });
    }
    Ok(out)
}

fn stop_locked(map: &mut HashMap<String, ActiveStream>, input_id: &str) {
    if let Some(mut s) = map.remove(input_id) {
        let _ = s.stop_tx.send(());
        if let Some(j) = s.join.take() {
            let _ = j.join();
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StopNativeMidiArgs {
    pub input_id: String,
}

#[tauri::command]
pub fn stop_native_midi_input(
    ctrl: tauri::State<'_, MidiInputController>,
    args: StopNativeMidiArgs,
) -> Result<(), String> {
    let mut g = ctrl.inner.lock().map_err(|_| "midi mutex poisoned")?;
    stop_locked(&mut g, &args.input_id);
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartNativeMidiArgs {
    pub device_index: usize,
    pub input_id: String,
}

#[tauri::command]
pub fn start_native_midi_input(
    app: AppHandle,
    ctrl: tauri::State<'_, MidiInputController>,
    args: StartNativeMidiArgs,
) -> Result<(), String> {
    let device_index = args.device_index;
    let input_id = args.input_id;
    let mut g = ctrl.inner.lock().map_err(|_| "midi mutex poisoned")?;
    stop_locked(&mut g, &input_id);

    let (stop_tx, stop_rx): (Sender<()>, Receiver<()>) = mpsc::channel();
    let app_handle = app.clone();
    let input_id_emit = input_id.clone();
    let input_id_outer_log = input_id.clone();

    let join = std::thread::spawn(move || {
        let midi_in = match midir::MidiInput::new("ToneFrame MIDI in") {
            Ok(m) => m,
            Err(e) => {
                log_safe!("[ToneFrame:native-midi] MidiInput::new failed: {}", e);
                return;
            }
        };
        let ports = midi_in.ports();
        let port = match ports.get(device_index) {
            Some(p) => p,
            None => {
                log_safe!(
                    "[ToneFrame:native-midi] device index {} not found ({} ports)",
                    device_index,
                    ports.len()
                );
                return;
            }
        };
        let conn = match midi_in.connect(
            port,
            "toneframe-in",
            move |_stamp: u64, message: &[u8], _: &mut ()| {
                let bytes: Vec<u8> = message.to_vec();
                let payload = serde_json::json!({
                    "inputId": &input_id_emit,
                    "bytes": bytes,
                });
                let _ = app_handle.emit("native-midi-chunk", payload);
            },
            (),
        ) {
            Ok(c) => c,
            Err(e) => {
                log_safe!("[ToneFrame:native-midi] connect failed: {}", e);
                return;
            }
        };

        log_safe!(
            "[ToneFrame:native-midi] stream started input_id={} index={}",
            input_id_outer_log,
            device_index
        );
        let _ = stop_rx.recv();
        drop(conn);
        log_safe!(
            "[ToneFrame:native-midi] stream stopped input_id={}",
            input_id_outer_log
        );
    });

    g.insert(
        input_id.clone(),
        ActiveStream {
            stop_tx,
            join: Some(join),
        },
    );
    Ok(())
}

