#!/usr/bin/env node
// First-pass glyph atlas metadata generator.
// This intentionally writes JSON only; actual SDF baking can be swapped later.

import fs from "node:fs";
import path from "node:path";

const outPath = process.argv[2] || path.resolve(process.cwd(), "dist/assets/font-atlas.json");
const glyphs = [];
for (let cp = 32; cp <= 126; cp += 1) {
  glyphs.push({
    codepoint: cp,
    ch: String.fromCharCode(cp),
    advance: 0.56
  });
}

const payload = {
  family: "Inter",
  weights: [400, 600, 700, 900],
  size: 48,
  glyphs: glyphs
};

fs.mkdirSync(path.dirname(outPath), { recursive: true });
fs.writeFileSync(outPath, JSON.stringify(payload, null, 2) + "\n", "utf8");
console.log("Wrote atlas metadata:", outPath);
