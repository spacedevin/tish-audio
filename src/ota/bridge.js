import { unzipSync } from "fflate";
import { BaseDirectory, exists, mkdir, writeTextFile, readTextFile, writeFile, remove } from "@tauri-apps/plugin-fs";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { appDataDir, join } from "@tauri-apps/api/path";

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
};
