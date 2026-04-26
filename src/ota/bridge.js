import { unzipSync } from "fflate";
import { BaseDirectory, exists, mkdir, writeTextFile, readTextFile, writeFile, remove } from "@tauri-apps/plugin-fs";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { appDataDir, join } from "@tauri-apps/api/path";
import { getCurrentWindow } from "@tauri-apps/api/window";

window.__OTA_BRIDGE__ = {
  unzipSync,
  BaseDirectory, exists, mkdir, writeTextFile, readTextFile, writeFile, remove,
  convertFileSrc,
  appDataDir, join,
  invoke,
  ensureMicPermission: () => invoke("ensure_mic_permission"),
  listenNativeAudioChunk: (handler) =>
    listen("native-audio-chunk", (event) => {
      handler(event.payload);
    }),
  listenNativeMidiChunk: (handler) =>
    listen("native-midi-chunk", (event) => {
      handler(event.payload);
    }),
  createSynthEngine: (spec, sampleRate) =>
    invoke("create_synth_engine", { spec, sampleRate: sampleRate ?? 48000 }),
  releaseSynthEngine: (handleId) => invoke("release_synth_engine", { handleId }),
  applySynthPreset: (handleId, preset) =>
    invoke("apply_synth_preset", { handleId, preset }),
  setSynthParam: (handleId, paramId, value) =>
    invoke("set_synth_param", { handleId, paramId, value }),
  sendSynthNoteEvent: (handleId, event) =>
    invoke("send_synth_note_event", { handleId, event }),
  listenNativeSynthAudioChunk: (handler) =>
    listen("native-synth-chunk", (event) => {
      handler(event.payload);
    }),
  /** macOS overlay title bar: CSS drag regions are unreliable; use Tauri 2 `startDragging` from a pointer event. */
  startWindowDrag: () => {
    if (typeof window === "undefined" || !window.__TAURI_INTERNALS__) {
      return Promise.resolve();
    }
    return getCurrentWindow().startDragging();
  },
};
