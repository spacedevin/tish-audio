# Modules

This directory contains individual audio effect and rack modules following the GarageBand-style modularity pattern. 

Each module lives in its own directory (e.g., `overdrive/`, `master_eq/`) and exports:
- `View`: the UI component built from `PedalChassis` or `RackChassis`.
- `buildAudio(ctx, node)`: constructs the local Web Audio API graph and binds parameter updates.
- `metadata`, `type`, `category`, and `layout`.

## Assets & Tailwind

- **Images & SVG**: Put static assets local to the module (e.g. `src/modules/overdrive/assets/face.svg`) and reference them.
- **Custom Fonts**: Define `@font-face` blocks locally or in `tish-tailwind/src/pedal-extras.css`.
- **Tailwind**: Module UI files (`index.tish`, etc.) are fully covered by the JIT content globs, so `bg-red-500` works as usual.
