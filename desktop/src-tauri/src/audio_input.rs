//! Native microphone capture via cpal + Tauri events (WKWebView often has no `navigator.mediaDevices`).
//!
//! `cpal::Stream` is not `Send` on macOS, so it cannot live in `tauri::State`. The stream is owned
//! on a dedicated thread; we only keep stop-channel + join handle in state.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamError, StreamConfig, SupportedStreamConfig};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use tauri::{AppHandle, Emitter};

const CHUNK_SAMPLES: usize = 384;

#[derive(Clone, Serialize)]
pub struct ListedInput {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeAudioChunk {
    pub device_id: String,
    pub sample_rate: u32,
    pub samples: Vec<f32>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartNativeInputResult {
    pub sample_rate: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartNativeInputArgs {
    pub device_id: String,
    #[allow(dead_code)]
    pub target_sample_rate: f64,
}

/// Stop signal + join handle for the thread that owns the `cpal::Stream`.
pub struct AudioInputController {
    inner: Arc<Mutex<Option<(mpsc::Sender<()>, JoinHandle<()>)>>>,
}

impl Default for AudioInputController {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
        }
    }
}

impl Clone for AudioInputController {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl AudioInputController {
    fn stop_locked(&self) {
        let mut g = self.inner.lock().expect("audio input mutex poisoned");
        if let Some((tx, join)) = g.take() {
            let _ = tx.send(());
            let _ = join.join();
        }
    }
}

fn parse_native_index(device_id: &str) -> Option<usize> {
    device_id.strip_prefix("native:")?.parse().ok()
}

fn log_cpal_stream_error(e: StreamError) {
    eprintln!(
        "[ToneFrame:native-audio] cpal stream error (mic permission denied, device unplugged, or driver): {:?}",
        e
    );
}

/// Append mono frames, drain full chunks, emit Tauri events; log first emit and emit failures.
fn push_mono_emit_chunks(
    app: &AppHandle,
    device_id: &str,
    sample_rate: u32,
    pending: &mut Vec<f32>,
    mono: &[f32],
    logged_first_emit: &Arc<AtomicBool>,
) {
    pending.extend_from_slice(mono);
    while pending.len() >= CHUNK_SAMPLES {
        let chunk: Vec<f32> = pending.drain(..CHUNK_SAMPLES).collect();
        let payload = NativeAudioChunk {
            device_id: device_id.to_string(),
            sample_rate,
            samples: chunk,
        };
        if !logged_first_emit.swap(true, Ordering::Relaxed) {
            eprintln!(
                "[ToneFrame:native-audio] first PCM chunk emitted ({} samples @ {} Hz, device_id={})",
                CHUNK_SAMPLES, sample_rate, device_id
            );
        }
        if let Err(e) = app.emit("native-audio-chunk", payload) {
            eprintln!(
                "[ToneFrame:native-audio] emit(native-audio-chunk) failed — is the webview listening? IPC/event: {}",
                e
            );
        }
    }
}

fn nth_input_device(index: usize) -> Result<cpal::Device, String> {
    let host = cpal::default_host();
    host.input_devices()
        .map_err(|e| e.to_string())?
        .nth(index)
        .ok_or_else(|| format!("audio input device index {} not found", index))
}

#[tauri::command]
pub fn list_audio_inputs() -> Result<Vec<ListedInput>, String> {
    let host = cpal::default_host();
    let mut out = Vec::new();
    let input_iter = match host.input_devices() {
        Ok(iter) => iter,
        Err(e) => {
            let msg = e.to_string();
            eprintln!(
                "[ToneFrame:native-audio] list_audio_inputs: input_devices() failed — {} (on macOS: microphone entitlement + Info.plist NSMicrophoneUsageDescription; allow app in System Settings → Privacy)",
                msg
            );
            return Err(msg);
        }
    };
    for (i, device) in input_iter.enumerate() {
        let name = device.name().map_err(|e| {
            let msg = e.to_string();
            eprintln!(
                "[ToneFrame:native-audio] list_audio_inputs: device.name() failed for index {}: {}",
                i, msg
            );
            msg
        })?;
        out.push(ListedInput {
            id: format!("native:{}", i),
            name,
        });
    }
    if out.is_empty() {
        eprintln!(
            "[ToneFrame:native-audio] list_audio_inputs: host returned zero input devices (empty enumeration — often permission or no hardware)"
        );
        return Err("no audio input devices found".into());
    }
    eprintln!(
        "[ToneFrame:native-audio] list_audio_inputs: ok ({} devices)",
        out.len()
    );
    Ok(out)
}

fn interleaved_to_mono_f32_f32(data: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return data.to_vec();
    }
    let mut v = Vec::with_capacity(data.len() / channels);
    for frame in data.chunks(channels) {
        let mut sum = 0f32;
        for &s in frame {
            sum += s;
        }
        v.push(sum / channels as f32);
    }
    v
}

fn interleaved_to_mono_f32_i16(data: &[i16], channels: usize) -> Vec<f32> {
    let scale = 1.0 / 32768.0f32;
    if channels <= 1 {
        return data.iter().map(|&s| s as f32 * scale).collect();
    }
    let mut v = Vec::with_capacity(data.len() / channels);
    for frame in data.chunks(channels) {
        let mut sum = 0f32;
        for &s in frame {
            sum += s as f32 * scale;
        }
        v.push(sum / channels as f32);
    }
    v
}

fn interleaved_to_mono_f32_i32(data: &[i32], channels: usize) -> Vec<f32> {
    let scale = 1.0 / 2147483648.0f32;
    if channels <= 1 {
        return data.iter().map(|&s| s as f32 * scale).collect();
    }
    let mut v = Vec::with_capacity(data.len() / channels);
    for frame in data.chunks(channels) {
        let mut sum = 0f32;
        for &s in frame {
            sum += s as f32 * scale;
        }
        v.push(sum / channels as f32);
    }
    v
}

fn interleaved_to_mono_f32_i64(data: &[i64], channels: usize) -> Vec<f32> {
    let scale = 1.0 / 9223372036854775808.0f64;
    if channels <= 1 {
        return data
            .iter()
            .map(|&s| (s as f64 * scale) as f32)
            .collect();
    }
    let mut v = Vec::with_capacity(data.len() / channels);
    for frame in data.chunks(channels) {
        let mut sum = 0f64;
        for &s in frame {
            sum += s as f64 * scale;
        }
        v.push((sum / channels as f64) as f32);
    }
    v
}

fn interleaved_to_mono_f32_i8(data: &[i8], channels: usize) -> Vec<f32> {
    let scale = 1.0 / 128.0f32;
    if channels <= 1 {
        return data.iter().map(|&s| s as f32 * scale).collect();
    }
    let mut v = Vec::with_capacity(data.len() / channels);
    for frame in data.chunks(channels) {
        let mut sum = 0f32;
        for &s in frame {
            sum += s as f32 * scale;
        }
        v.push(sum / channels as f32);
    }
    v
}

fn interleaved_to_mono_f32_u16(data: &[u16], channels: usize) -> Vec<f32> {
    let scale = 1.0 / 65535.0f32;
    if channels <= 1 {
        return data.iter().map(|&s| s as f32 * scale).collect();
    }
    let mut v = Vec::with_capacity(data.len() / channels);
    for frame in data.chunks(channels) {
        let mut sum = 0f32;
        for &s in frame {
            sum += s as f32 * scale;
        }
        v.push(sum / channels as f32);
    }
    v
}

fn interleaved_to_mono_f32_u32(data: &[u32], channels: usize) -> Vec<f32> {
    let scale = 1.0 / 4294967295.0f64;
    if channels <= 1 {
        return data
            .iter()
            .map(|&s| (s as f64 * scale) as f32)
            .collect();
    }
    let mut v = Vec::with_capacity(data.len() / channels);
    for frame in data.chunks(channels) {
        let mut sum = 0f64;
        for &s in frame {
            sum += s as f64 * scale;
        }
        v.push((sum / channels as f64) as f32);
    }
    v
}

fn interleaved_to_mono_f32_u64(data: &[u64], channels: usize) -> Vec<f32> {
    let scale = 1.0 / 18446744073709551615.0f64;
    if channels <= 1 {
        return data
            .iter()
            .map(|&s| (s as f64 * scale) as f32)
            .collect();
    }
    let mut v = Vec::with_capacity(data.len() / channels);
    for frame in data.chunks(channels) {
        let mut sum = 0f64;
        for &s in frame {
            sum += s as f64 * scale;
        }
        v.push((sum / channels as f64) as f32);
    }
    v
}

fn interleaved_to_mono_f32_u8(data: &[u8], channels: usize) -> Vec<f32> {
    let scale = 1.0 / 255.0f32;
    if channels <= 1 {
        return data.iter().map(|&s| s as f32 * scale).collect();
    }
    let mut v = Vec::with_capacity(data.len() / channels);
    for frame in data.chunks(channels) {
        let mut sum = 0f32;
        for &s in frame {
            sum += s as f32 * scale;
        }
        v.push(sum / channels as f32);
    }
    v
}

fn interleaved_to_mono_f32_f64(data: &[f64], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return data.iter().map(|&s| s as f32).collect();
    }
    let mut v = Vec::with_capacity(data.len() / channels);
    for frame in data.chunks(channels) {
        let mut sum = 0f64;
        for &s in frame {
            sum += s;
        }
        v.push((sum / channels as f64) as f32);
    }
    v
}

fn build_stream_for_format(
    device: &cpal::Device,
    supported: SupportedStreamConfig,
    app: AppHandle,
    device_id: String,
) -> Result<(Stream, u32), String> {
    let channels = supported.channels() as usize;
    let sample_rate = supported.sample_rate().0;
    let format = supported.sample_format();
    let config: StreamConfig = supported.config();
    let mut pending: Vec<f32> = Vec::new();
    let logged_first_emit = Arc::new(AtomicBool::new(false));

    let stream = match format {
        SampleFormat::F32 => {
            let fe = Arc::clone(&logged_first_emit);
            device
                .build_input_stream(
                    &config,
                    move |data: &[f32], _| {
                        let mono = interleaved_to_mono_f32_f32(data, channels);
                        push_mono_emit_chunks(
                            &app,
                            &device_id,
                            sample_rate,
                            &mut pending,
                            &mono,
                            &fe,
                        );
                    },
                    log_cpal_stream_error,
                    None,
                )
                .map_err(|e| e.to_string())?
        }
        SampleFormat::I16 => {
            let fe = Arc::clone(&logged_first_emit);
            device
                .build_input_stream(
                    &config,
                    move |data: &[i16], _| {
                        let mono = interleaved_to_mono_f32_i16(data, channels);
                        push_mono_emit_chunks(
                            &app,
                            &device_id,
                            sample_rate,
                            &mut pending,
                            &mono,
                            &fe,
                        );
                    },
                    log_cpal_stream_error,
                    None,
                )
                .map_err(|e| e.to_string())?
        }
        SampleFormat::I32 => {
            let fe = Arc::clone(&logged_first_emit);
            device
                .build_input_stream(
                    &config,
                    move |data: &[i32], _| {
                        let mono = interleaved_to_mono_f32_i32(data, channels);
                        push_mono_emit_chunks(
                            &app,
                            &device_id,
                            sample_rate,
                            &mut pending,
                            &mono,
                            &fe,
                        );
                    },
                    log_cpal_stream_error,
                    None,
                )
                .map_err(|e| e.to_string())?
        }
        SampleFormat::I64 => {
            let fe = Arc::clone(&logged_first_emit);
            device
                .build_input_stream(
                    &config,
                    move |data: &[i64], _| {
                        let mono = interleaved_to_mono_f32_i64(data, channels);
                        push_mono_emit_chunks(
                            &app,
                            &device_id,
                            sample_rate,
                            &mut pending,
                            &mono,
                            &fe,
                        );
                    },
                    log_cpal_stream_error,
                    None,
                )
                .map_err(|e| e.to_string())?
        }
        SampleFormat::I8 => {
            let fe = Arc::clone(&logged_first_emit);
            device
                .build_input_stream(
                    &config,
                    move |data: &[i8], _| {
                        let mono = interleaved_to_mono_f32_i8(data, channels);
                        push_mono_emit_chunks(
                            &app,
                            &device_id,
                            sample_rate,
                            &mut pending,
                            &mono,
                            &fe,
                        );
                    },
                    log_cpal_stream_error,
                    None,
                )
                .map_err(|e| e.to_string())?
        }
        SampleFormat::U16 => {
            let fe = Arc::clone(&logged_first_emit);
            device
                .build_input_stream(
                    &config,
                    move |data: &[u16], _| {
                        let mono = interleaved_to_mono_f32_u16(data, channels);
                        push_mono_emit_chunks(
                            &app,
                            &device_id,
                            sample_rate,
                            &mut pending,
                            &mono,
                            &fe,
                        );
                    },
                    log_cpal_stream_error,
                    None,
                )
                .map_err(|e| e.to_string())?
        }
        SampleFormat::U32 => {
            let fe = Arc::clone(&logged_first_emit);
            device
                .build_input_stream(
                    &config,
                    move |data: &[u32], _| {
                        let mono = interleaved_to_mono_f32_u32(data, channels);
                        push_mono_emit_chunks(
                            &app,
                            &device_id,
                            sample_rate,
                            &mut pending,
                            &mono,
                            &fe,
                        );
                    },
                    log_cpal_stream_error,
                    None,
                )
                .map_err(|e| e.to_string())?
        }
        SampleFormat::U64 => {
            let fe = Arc::clone(&logged_first_emit);
            device
                .build_input_stream(
                    &config,
                    move |data: &[u64], _| {
                        let mono = interleaved_to_mono_f32_u64(data, channels);
                        push_mono_emit_chunks(
                            &app,
                            &device_id,
                            sample_rate,
                            &mut pending,
                            &mono,
                            &fe,
                        );
                    },
                    log_cpal_stream_error,
                    None,
                )
                .map_err(|e| e.to_string())?
        }
        SampleFormat::U8 => {
            let fe = Arc::clone(&logged_first_emit);
            device
                .build_input_stream(
                    &config,
                    move |data: &[u8], _| {
                        let mono = interleaved_to_mono_f32_u8(data, channels);
                        push_mono_emit_chunks(
                            &app,
                            &device_id,
                            sample_rate,
                            &mut pending,
                            &mono,
                            &fe,
                        );
                    },
                    log_cpal_stream_error,
                    None,
                )
                .map_err(|e| e.to_string())?
        }
        SampleFormat::F64 => {
            let fe = Arc::clone(&logged_first_emit);
            device
                .build_input_stream(
                    &config,
                    move |data: &[f64], _| {
                        let mono = interleaved_to_mono_f32_f64(data, channels);
                        push_mono_emit_chunks(
                            &app,
                            &device_id,
                            sample_rate,
                            &mut pending,
                            &mono,
                            &fe,
                        );
                    },
                    log_cpal_stream_error,
                    None,
                )
                .map_err(|e| e.to_string())?
        }
        _ => {
            return Err(format!(
                "unsupported sample format {:?} for native input",
                format
            ));
        }
    };

    stream.play().map_err(|e| {
        let msg = e.to_string();
        eprintln!(
            "[ToneFrame:native-audio] stream.play() failed — {} (device may be in use or access denied)",
            msg
        );
        msg
    })?;
    eprintln!(
        "[ToneFrame:native-audio] capture stream running (sample_rate={}, format={:?})",
        sample_rate, format
    );
    Ok((stream, sample_rate))
}

/// Opens the cpal stream and only signals `ready_tx` after `stream.play()` succeeds so the command
/// handler can return an error instead of reporting success while capture never starts.
fn run_capture_thread(
    ready_tx: mpsc::Sender<Result<u32, String>>,
    stop_rx: mpsc::Receiver<()>,
    idx: usize,
    device_id: String,
    app: AppHandle,
) {
    let device = match nth_input_device(idx) {
        Ok(d) => d,
        Err(e) => {
            eprintln!(
                "[ToneFrame:native-audio] run_capture_thread: nth_input_device failed: {}",
                e
            );
            let _ = ready_tx.send(Err(e));
            return;
        }
    };
    let supported = match device.default_input_config() {
        Ok(c) => c,
        Err(e) => {
            let msg = format!("default_input_config: {}", e);
            eprintln!(
                "[ToneFrame:native-audio] run_capture_thread: {} (often permission or invalid device)",
                msg
            );
            let _ = ready_tx.send(Err(msg));
            return;
        }
    };
    let (stream, sample_rate) = match build_stream_for_format(&device, supported, app, device_id) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "[ToneFrame:native-audio] run_capture_thread: build_stream_for_format failed: {}",
                e
            );
            let _ = ready_tx.send(Err(e));
            return;
        }
    };
    if ready_tx.send(Ok(sample_rate)).is_err() {
        drop(stream);
        return;
    }
    let _ = stop_rx.recv();
    drop(stream);
}

/// `target_sample_rate` is reserved for future Rust-side resampling; JS resamples for now.
#[tauri::command]
pub fn start_native_input(
    app: AppHandle,
    ctrl: tauri::State<'_, AudioInputController>,
    args: StartNativeInputArgs,
) -> Result<StartNativeInputResult, String> {
    let device_id = args.device_id;
    let idx = parse_native_index(&device_id).ok_or_else(|| "invalid device id; expected native:N".to_string())?;

    ctrl.stop_locked();

    let (ready_tx, ready_rx) = mpsc::channel::<Result<u32, String>>();
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let app_c = app.clone();
    let device_id_c = device_id.clone();
    let join = std::thread::spawn(move || {
        run_capture_thread(ready_tx, stop_rx, idx, device_id_c, app_c)
    });

    let sample_rate = match ready_rx.recv() {
        Ok(Ok(sr)) => sr,
        Ok(Err(e)) => {
            eprintln!(
                "[ToneFrame:native-audio] start_native_input: capture thread reported error: {}",
                e
            );
            let _ = join.join();
            return Err(e);
        }
        Err(_) => {
            eprintln!(
                "[ToneFrame:native-audio] start_native_input: ready channel closed before capture reported status"
            );
            let _ = join.join();
            return Err(
                "native capture thread failed before reporting status (channel closed)".to_string(),
            );
        }
    };

    let mut g = ctrl.inner.lock().expect("audio input mutex poisoned");
    *g = Some((stop_tx, join));

    eprintln!(
        "[ToneFrame:native-audio] start_native_input: ok device_id={} sample_rate={}",
        device_id, sample_rate
    );
    Ok(StartNativeInputResult { sample_rate })
}

#[tauri::command]
pub fn stop_native_input(ctrl: tauri::State<'_, AudioInputController>) -> Result<(), String> {
    ctrl.stop_locked();
    Ok(())
}
