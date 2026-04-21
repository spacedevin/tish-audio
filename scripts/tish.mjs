#!/usr/bin/env node
/**
 * Resolve a `tish` binary that supports dev-server (VM + http/fs/process).
 * The @tishlang/tish npm prebuild must be compiled with --features full; if yours is too old,
 * set TISH to a local binary or build ../tish/tish with: cargo build -p tishlang --release --features full
 */
import fs from "fs";
import path from "path";
import { spawnSync } from "child_process";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(__dirname, "..");

function canExec(p) {
  try {
    const st = fs.statSync(p);
    if (!st.isFile()) return false;
    fs.accessSync(p, fs.constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

function resolveTishExe() {
  const env = process.env.TISH?.trim();
  if (env && canExec(env)) return env;

  const siblings = [
    path.join(root, "..", "tish", "tish", "target", "release", "tish"),
    path.join(root, "..", "tish", "tish", "target", "debug", "tish"),
  ];
  for (const p of siblings) {
    if (canExec(p)) return p;
  }

  const npmT = path.join(root, "node_modules", "@tishlang", "tish", "bin", "tish");
  if (fs.existsSync(npmT)) return npmT;

  const home = process.env.HOME || process.env.USERPROFILE;
  if (home) {
    const cargoT = path.join(home, ".cargo", "bin", "tish");
    if (canExec(cargoT)) return cargoT;
  }

  console.error(
    "tish-audio: no `tish` executable found. Install deps (npm install), set TISH to a capable binary, " +
      "or build the compiler repo: (cd ../tish/tish && cargo build -p tishlang --release --features full)."
  );
  process.exit(127);
}

const exe = resolveTishExe();
const r = spawnSync(exe, process.argv.slice(2), { stdio: "inherit", cwd: root, env: process.env });
process.exit(r.status ?? 1);
