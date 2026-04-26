//! Native synth engines: stream PCM to the webview via `native-synth-chunk` events (monitor path).
//! Full IR interpreter lands later; this wedge proves OTA transport + worklet playback.

macro_rules! log_safe {
    ($($arg:tt)*) => {
        let _ = std::io::Write::write_fmt(
            &mut std::io::stderr(),
            format_args!("{}\n", format_args!($($arg)*)),
        );
    };
}

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

const CHUNK_SAMPLES: usize = 384;
const OUT_SR: f32 = 48000.0;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeSynthChunk {
    pub handle_id: u64,
    pub sample_rate: u32,
    pub samples: Vec<f32>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSynthEngineResult {
    pub handle_id: u64,
}

struct Voice {
    gate: bool,
    hz: f32,
    phase: f32,
    env: f32,
    vel: f32,
}

struct EngineThread {
    voice: Arc<Mutex<Voice>>,
    stop_rx: mpsc::Receiver<()>,
}

fn hz_from_midi(note: i32) -> f32 {
    440.0 * 2f32.powf((note as f32 - 69.0) / 12.0)
}

fn run_engine_loop(app: AppHandle, handle_id: u64, slot: EngineThread) {
    let voice = slot.voice;
    let stop_rx = slot.stop_rx;
    let mut samples = vec![0.0f32; CHUNK_SAMPLES];
    let chunk_dur = Duration::from_secs_f64(CHUNK_SAMPLES as f64 / 48000.0_f64);
    let two_pi = std::f32::consts::TAU;
    loop {
        if stop_rx.try_recv().is_ok() {
            log_safe!("[ToneFrame:native-synth] engine {} stopped", handle_id);
            break;
        }
        {
            let mut v = voice.lock().expect("synth voice poisoned");
            let hz = v.hz.max(20.0).min(20000.0);
            let mut i = 0usize;
            while i < CHUNK_SAMPLES {
                if v.gate {
                    v.env = (v.env + 0.003).min(1.0);
                } else {
                    v.env = (v.env - 0.0012).max(0.0);
                }
                v.phase += two_pi * hz / OUT_SR;
                if v.phase > two_pi {
                    v.phase -= two_pi;
                }
                samples[i] = v.phase.sin() * v.env * 0.18 * v.vel;
                i += 1;
            }
        }
        let payload = NativeSynthChunk {
            handle_id,
            sample_rate: OUT_SR as u32,
            samples: samples.clone(),
        };
        if let Err(e) = app.emit("native-synth-chunk", payload) {
            log_safe!(
                "[ToneFrame:native-synth] emit failed for handle {}: {}",
                handle_id,
                e
            );
        }
        thread::sleep(chunk_dur);
    }
}

pub struct SynthEngineController {
    next_id: AtomicU64,
    engines: Mutex<HashMap<u64, mpsc::Sender<()>>>,
    voices: Mutex<HashMap<u64, Arc<Mutex<Voice>>>>,
}

impl Default for SynthEngineController {
    fn default() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            engines: Mutex::new(HashMap::new()),
            voices: Mutex::new(HashMap::new()),
        }
    }
}

#[tauri::command]
pub fn create_synth_engine(
    app: AppHandle,
    state: tauri::State<'_, SynthEngineController>,
    spec: Value,
    sample_rate: u32,
) -> Result<CreateSynthEngineResult, String> {
    let _ = spec;
    let _ = sample_rate;
    let id = state.next_id.fetch_add(1, Ordering::Relaxed);
    let voice = Arc::new(Mutex::new(Voice {
        gate: false,
        hz: 220.0,
        phase: 0.0,
        env: 0.0,
        vel: 0.7,
    }));
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    {
        let mut g = state.engines.lock().expect("synth engines poisoned");
        g.insert(id, stop_tx);
    }
    {
        let mut gv = state.voices.lock().expect("synth voices poisoned");
        gv.insert(id, Arc::clone(&voice));
    }
    let slot = EngineThread { voice, stop_rx };
    let app2 = app.clone();
    thread::spawn(move || {
        run_engine_loop(app2, id, slot);
    });
    log_safe!("[ToneFrame:native-synth] created engine handle_id={}", id);
    Ok(CreateSynthEngineResult { handle_id: id })
}

#[tauri::command]
pub fn release_synth_engine(
    state: tauri::State<'_, SynthEngineController>,
    handle_id: u64,
) -> Result<(), String> {
    let tx = {
        let mut g = state.engines.lock().expect("synth engines poisoned");
        g.remove(&handle_id)
    };
    if let Some(stop) = tx {
        let _ = stop.send(());
    }
    {
        let mut gv = state.voices.lock().expect("synth voices poisoned");
        gv.remove(&handle_id);
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SynthNoteEvent {
    pub kind: String,
    pub note: Option<i32>,
    pub velocity: Option<i32>,
    pub controller: Option<i32>,
    pub value: Option<i32>,
}

#[tauri::command]
pub fn send_synth_note_event(
    state: tauri::State<'_, SynthEngineController>,
    handle_id: u64,
    event: SynthNoteEvent,
) -> Result<(), String> {
    let voice_arc = {
        let gv = state.voices.lock().expect("synth voices poisoned");
        gv.get(&handle_id).cloned()
    };
    let Some(voice_arc) = voice_arc else {
        log_safe!(
            "[ToneFrame:native-synth] note_event: NO VOICE for handle_id={} kind={}",
            handle_id,
            event.kind
        );
        return Ok(());
    };
    let mut v = voice_arc.lock().expect("synth voice poisoned");
    let k = event.kind.as_str();
    if k == "noteOn" {
        let n = event.note.unwrap_or(60);
        let vel = event.velocity.unwrap_or(100).clamp(1, 127) as f32 / 127.0;
        v.hz = hz_from_midi(n);
        v.vel = vel.max(0.05);
        v.gate = true;
        log_safe!(
            "[ToneFrame:native-synth] note_event noteOn handle_id={} note={} vel={} -> hz={} gate=true",
            handle_id,
            n,
            event.velocity.unwrap_or(100),
            v.hz
        );
    } else if k == "noteOff" {
        v.gate = false;
        log_safe!(
            "[ToneFrame:native-synth] note_event noteOff handle_id={}",
            handle_id
        );
    } else {
        log_safe!(
            "[ToneFrame:native-synth] note_event: ignored kind={} handle_id={}",
            k,
            handle_id
        );
    }
    Ok(())
}

#[tauri::command]
pub fn set_synth_param(
    _state: tauri::State<'_, SynthEngineController>,
    _handle_id: u64,
    _param_id: String,
    _value: f64,
) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub fn apply_synth_preset(
    _state: tauri::State<'_, SynthEngineController>,
    _handle_id: u64,
    _preset: Value,
) -> Result<(), String> {
    Ok(())
}
