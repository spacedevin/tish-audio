mod audio_input;
mod midi_input;

use audio_input::{AudioInputController, list_audio_inputs, start_native_input, stop_native_input};
use midi_input::{
    list_midi_inputs, start_native_midi_input, stop_native_midi_input, MidiInputController,
};

/// Write a log line to stderr, discarding any I/O error so we never panic in a
/// packaged macOS app where stderr is closed (recent Rust eprintln! panics on
/// write failure).
macro_rules! log_safe {
    ($($arg:tt)*) => {
        let _ = std::io::Write::write_fmt(
            &mut std::io::stderr(),
            format_args!("{}\n", format_args!($($arg)*)),
        );
    };
}

/// Request microphone access via AVFoundation and WAIT for the user's answer before returning.
/// Returns Ok(true) when granted, Err("denied") when denied/restricted, so callers can refuse
/// to start capture instead of silently streaming zeros.
#[tauri::command]
fn ensure_mic_permission() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        use objc2::runtime::Bool;
        use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};
        use std::sync::mpsc;
        use std::time::Duration;

        unsafe {
            let Some(media_type) = AVMediaTypeAudio else {
                return Err("AVMediaTypeAudio constant unavailable".into());
            };

            let status = AVCaptureDevice::authorizationStatusForMediaType(media_type);

            if status == AVAuthorizationStatus::Authorized {
                log_safe!("[ToneFrame:mic-permission] already authorized");
                return Ok(true);
            }

            if status == AVAuthorizationStatus::Denied
                || status == AVAuthorizationStatus::Restricted
            {
                log_safe!("[ToneFrame:mic-permission] denied — opening System Settings");
                // Open the macOS Privacy → Microphone pane so the user can re-enable without
                // hunting through menus.
                let _ = std::process::Command::new("open")
                    .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
                    .spawn();
                return Err("denied".into());
            }

            // NotDetermined: show the system dialog and block until the user responds.
            log_safe!("[ToneFrame:mic-permission] requesting microphone access…");
            let (tx, rx) = mpsc::channel::<bool>();
            let handler = block2::RcBlock::new(move |granted: Bool| {
                log_safe!(
                    "[ToneFrame:mic-permission] user response: granted={}",
                    granted.as_bool()
                );
                let _ = tx.send(granted.as_bool());
            });
            AVCaptureDevice::requestAccessForMediaType_completionHandler(media_type, &handler);

            match rx.recv_timeout(Duration::from_secs(60)) {
                Ok(true) => Ok(true),
                Ok(false) => Err("denied".into()),
                Err(_) => Err("permission request timed out".into()),
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(true)
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AudioInputController::default())
        .manage(MidiInputController::default())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            ensure_mic_permission,
            list_audio_inputs,
            start_native_input,
            stop_native_input,
            list_midi_inputs,
            start_native_midi_input,
            stop_native_midi_input,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
