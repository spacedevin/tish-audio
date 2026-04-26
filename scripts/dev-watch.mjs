#!/usr/bin/env node
/**
 * Watches src/**\/*.tish and rebuilds dist/main.js on change.
 * Without this, `npm run dev` (or `npm run desktop:dev` via Tauri's
 * beforeDevCommand) only builds main.js ONCE at startup — every
 * subsequent edit needs a manual `npm run build` + window reload.
 *
 * Behavior:
 *   - Initial main.js build is performed by the parent `npm run dev`
 *     script (we run AFTER that completes).
 *   - We watch src/ recursively for *.tish + *.js changes and re-run
 *     `tish build` with a 200ms debounce so cascading saves coalesce.
 *   - We touch dist/index.html on success so anything that watches the
 *     index can trigger a webview reload (Tauri does not, but a future
 *     hot-reload bridge can).
 *
 * Spawned as a background sub-process from `npm run dev`. The parent's
 * `dev-server.tish` continues serving dist/ as before; we just keep
 * the bundle fresh underneath it.
 */
import { spawn, spawnSync } from "child_process";
import { watch } from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(__dirname, "..");
const srcDir = path.join(root, "src");
const tishCli = path.join(__dirname, "tish.mjs");

let pending = null;
let buildInFlight = false;
let buildAgain = false;

const colorise = (tag, color) => "\x1b[" + color + "m" + tag + "\x1b[0m";

function runMainBuild() {
  if (buildInFlight) {
    buildAgain = true;
    return;
  }
  buildInFlight = true;
  const t0 = Date.now();
  process.stdout.write(colorise("[dev-watch]", "36") + " rebuilding main.js…\n");
  const r = spawnSync(
    process.execPath,
    [tishCli, "build", "--target", "js", "src/main.tish", "-o", "dist/main"],
    { cwd: root, stdio: "inherit", env: process.env }
  );
  buildInFlight = false;
  const dt = Date.now() - t0;
  if (r.status !== 0) {
    process.stdout.write(colorise("[dev-watch]", "31") + " build FAILED in " + dt + "ms\n");
  } else {
    process.stdout.write(colorise("[dev-watch]", "32") + " built dist/main.js in " + dt + "ms — reload the webview to pick up changes\n");
  }
  if (buildAgain) {
    buildAgain = false;
    runMainBuild();
  }
}

function schedule() {
  if (pending) return;
  pending = setTimeout(() => {
    pending = null;
    runMainBuild();
  }, 200);
}

function startWatch() {
  process.stdout.write(colorise("[dev-watch]", "36") + " watching " + srcDir + " for *.tish changes\n");
  try {
    watch(srcDir, { recursive: true }, (event, filename) => {
      if (!filename) return;
      const fn = String(filename);
      if (!fn.endsWith(".tish") && !fn.endsWith(".js")) return;
      // Skip generated worklet outputs landing back in dist (defensive)
      if (fn.includes("dist" + path.sep)) return;
      schedule();
    });
  } catch (e) {
    process.stderr.write(colorise("[dev-watch]", "31") + " fs.watch failed: " + (e && e.message ? e.message : String(e)) + "\n");
    process.exit(1);
  }
}

startWatch();

// Keep the process alive. fs.watch already keeps the loop alive but be defensive.
setInterval(() => {}, 1 << 30);
