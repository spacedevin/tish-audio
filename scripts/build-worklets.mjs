#!/usr/bin/env node
/**
 * Compile AudioWorklet entry points from Tish → self-contained JS.
 * Outputs use *.worklet.js suffix for clarity in dist/assets.
 */
import fs from "fs";
import path from "path";
import { spawnSync } from "child_process";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(__dirname, "..");
const tishCli = path.join(__dirname, "tish.mjs");

function listProcessorEntries() {
  const dir = path.join(root, "src", "audio", "backends", "worklet", "processors");
  if (!fs.existsSync(dir)) return [];
  return fs
    .readdirSync(dir)
    .filter((f) => f.endsWith(".processor.tish"))
    .map((f) => path.join(dir, f))
    .sort();
}

function runTishBuild(entryTish, outBase) {
  const r = spawnSync(process.execPath, [tishCli, "build", "--target", "js", entryTish, "-o", outBase], {
    stdio: "inherit",
    cwd: root,
    env: process.env,
  });
  if (r.status !== 0) {
    console.error(`build-worklets: failed for ${entryTish}`);
    process.exit(r.status ?? 1);
  }
}

function main() {
  const synthDir = path.join(root, "dist", "assets", "synth");
  fs.mkdirSync(synthDir, { recursive: true });

  for (const entry of listProcessorEntries()) {
    const base = path.basename(entry, ".tish");
    const outFile = path.join(synthDir, base + ".worklet.js");
    runTishBuild(entry, outFile);
    if (!fs.existsSync(outFile)) {
      console.error(`build-worklets: missing output ${outFile}`);
      process.exit(1);
    }
  }

  const nativeOutTish = path.join(root, "src", "audio", "nativeSynthOut.worklet.tish");
  if (fs.existsSync(nativeOutTish)) {
    const outFile = path.join(root, "dist", "assets", "native-synth-out.worklet.js");
    runTishBuild(nativeOutTish, outFile);
    if (!fs.existsSync(outFile)) {
      console.error(`build-worklets: missing output ${outFile}`);
      process.exit(1);
    }
  }
}

main();
