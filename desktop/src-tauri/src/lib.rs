mod audio_input;

use audio_input::{AudioInputController, list_audio_inputs, start_native_input, stop_native_input};

/// Request microphone access via AVFoundation. Call from the UI when the user starts the input
/// flow (e.g. Connect Input), not at cold boot: macOS shows the prompt more reliably after a
/// gesture, and this must run on every invoke — a previous `std::sync::Once` + `setup()` call
/// made later `invoke("ensure_mic_permission")` calls no-ops so dev never showed TCC.
#[tauri::command]
fn ensure_mic_permission() {
    #[cfg(target_os = "macos")]
    {
        use objc2::runtime::Bool;
        use objc2_av_foundation::{AVCaptureDevice, AVMediaTypeAudio};
        unsafe {
            let Some(media_type) = AVMediaTypeAudio else {
                eprintln!(
                    "[ToneFrame:mic-permission] AVMediaTypeAudio is unavailable; cannot request AVFoundation access"
                );
                return;
            };
            eprintln!(
                "[ToneFrame:mic-permission] requesting AVFoundation microphone access (async; check System Settings → Privacy if denied)"
            );
            let handler = block2::RcBlock::new(|granted: Bool| {
                eprintln!(
                    "[ToneFrame:mic-permission] AVFoundation audio access response: granted={}",
                    granted.as_bool()
                );
            });
            AVCaptureDevice::requestAccessForMediaType_completionHandler(
                media_type,
                &handler,
            );
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AudioInputController::default())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            ensure_mic_permission,
            list_audio_inputs,
            start_native_input,
            stop_native_input,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
