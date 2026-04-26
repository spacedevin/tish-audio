# Hardware Auto-Rig

When a user picks a MIDI device row from **Connect hardware**, the patch
engine can auto-spawn a starter graph: a `MIDI_IN` node with the correct port
fan-out, an optional layout panel for the device's controls, and (when the
patch board is empty) a default rack chain wired to `MASTER OUT`.

This file describes the system and how to register a new device or instrument
with it.

## Files

| File | What lives there |
|------|------------------|
| [`hardwareAutoRig.tish`](./hardwareAutoRig.tish) | The recipe registry. JSDoc-style schema and the canonical SMK-25 example. |
| [`useMidiPatch.tish`](./useMidiPatch.tish) | The hook that calls the registry on connect. Generic — knows nothing device-specific. |
| [`../inputDevices/midiHardwareVisuals.tish`](../inputDevices/midiHardwareVisuals.tish) | Maps OS-visible MIDI port names → a stable `hardwareVisualId` constant. |
| [`midiPortGroups.tish`](./midiPortGroups.tish) | Merges multi-USB devices (e.g. SMK-25 family) into a single combo row. |
| [`deviceAutoLayout.tish`](./deviceAutoLayout.tish) | Legacy SMK-25 helpers (kept as a fallback layout-edge builder). |

## Lifecycle

```
Connect hardware menu
       │
       ▼
matchMidiHardwareVisualId(portLabel)            ─►  hardwareVisualId
       │
       ▼
mergeMidiHardwareRows(rows)                     ─►  combo row?
       │
       ▼
useMidiPatch.connectMidiChoice(row)
       │
       ▼
getHardwareAutoRig(row.hardwareVisualId)        ─►  recipe | null
       │
       ▼   (if recipe)
recipe.isCombo(row)                             ─►  bool: spawn layout?
recipe.layoutNode({ ... })                      ─►  layout panel node
recipe.layoutEdges({ ... })                     ─►  MIDI_IN ► layout edges
recipe.starterRig({ ... })  (board empty only)  ─►  { nodes, edges, ... }
recipe.recommendedQuality(caps)                 ─►  Studio / Live + backend
       │
       ▼
runBatched(setNodes / setConnections / saveSynthQualitySettings)
```

Each callback is optional. Skip a callback to opt out of that step.

## Recipe schema

```ts
type AutoRig = {
  hardwareVisualId: string;          // matches midiHardwareVisuals.tish
  displayName: string;               // human-readable label
  comboKey?: string | null;          // optional grouping key for combo merge

  isCombo?(row): boolean;
  // Should this device row be treated as a multi-port combo (e.g. an SMK-25
  // family device that exposes 3 USB ports as one logical instrument)?

  layoutNode?({ id, x, y, portCount, hardwareVisualId, mergedPortIds }): Node | null;
  // Optional patch-board panel/face module that visualises the device.
  // Return null to skip and only add MIDI_IN.

  layoutEdges?({ midiInNodeId, layoutId, portCount, mergedPortIds, newId }): Edge[];
  // Edges connecting MIDI_IN.midi[N] → layout.midi[N] (or whichever ports).
  // newId() returns a fresh patch-graph id.

  starterRig?({ midiInNodeId, layoutId, layoutX, layoutY,
                PEDAL_LIBRARY, newId, hardwareVisualId, mergedPortIds })
    : { nodes: Node[], edges: Edge[], midiInExtraParams?: Record<string, any> } | null;
  // Only invoked when the patch board is empty (no non-system nodes).
  // Build a tasteful starter chain (synths, splitters, FX) and route it to
  // sys_out. Returned `midiInExtraParams` are merged into the MIDI_IN node's
  // params (used today for `midiComboLayout` + `autoLayoutDeviceId`).

  recommendedQuality?(caps): { qualityTier, backend } | null;
  // Picks a default Synth Quality tier + backend selector for the new rig.
  // caps = { hasWorklet, isTauri, isMobile }.
};
```

## Adding a new hardware recipe

### 1. Identify the device

Edit [`midiHardwareVisuals.tish`](../inputDevices/midiHardwareVisuals.tish):

```js
export const HARDWARE_AKAI_MPK_MINI = "akai_mpk_mini";

// Inside matchMidiHardwareVisualId(portLabel):
if (lower(s).indexOf("mpk mini") >= 0) {
  return HARDWARE_AKAI_MPK_MINI;
}

// Optionally inside hardwareDisplayNameForId(id):
if (id === HARDWARE_AKAI_MPK_MINI) return "Akai MPK Mini";
```

### 2. (Optional) build a layout panel module

If the device has knobs/pads/keys that should be patchable on the board, add
a module under `src/modules/<your_face>/index.tish` with a `View`,
`buildAudio`, and `defaultParams` (see [`smk25_face`](../modules/smk25_face/)
for the canonical example). Register it in `src/modules/registry.tish`.

This step is optional — many devices are happy with just a `MIDI_IN` row.

### 3. Register the recipe

Edit [`hardwareAutoRig.tish`](./hardwareAutoRig.tish):

```js
import { HARDWARE_AKAI_MPK_MINI } from "../inputDevices/midiHardwareVisuals.tish";
import { mpc_pads_metadata, mpc_pads_type } from "../modules/instruments/mpc_pads/index.tish";

const MPK_MINI_RECIPE = {
  hardwareVisualId: HARDWARE_AKAI_MPK_MINI,
  displayName: "Akai MPK Mini",

  // Treat any non-merged MPK Mini row as our entry point.
  isCombo: (row) => row && row.hardwareVisualId === HARDWARE_AKAI_MPK_MINI,

  // No dedicated layout face — skip layoutNode / layoutEdges.

  // Drop in an MPC pad rack + a MultiBank XV when the board is empty.
  starterRig: (opts) => {
    const lib = opts.PEDAL_LIBRARY;
    if (!lib) return null;
    const newId = opts.newId;
    const mpcTpl = lib.INSTRUMENTS.find((p) => p["type"] === "mpc_pads");
    const xvTpl = lib.INSTRUMENTS.find((p) => p["type"] === "multibank_xv");
    const mpcId = newId();
    const xvId = newId();
    const nodes = [];
    if (mpcTpl) {
      nodes.push({ id: mpcId, ...mpcTpl, params: { ...(mpcTpl.defaultParams || {}) },
                   bypassed: false, x: opts.layoutX + 360, y: opts.layoutY - 40 });
    }
    if (xvTpl) {
      nodes.push({ id: xvId, ...xvTpl, params: { ...(xvTpl.defaultParams || {}) },
                   bypassed: false, x: opts.layoutX + 360, y: opts.layoutY + 200 });
    }
    return {
      nodes: nodes,
      edges: [
        { id: newId(), from: opts.midiInNodeId, fromPort: "midi", to: mpcId, toPort: "midi" },
        { id: newId(), from: opts.midiInNodeId, fromPort: "midi", to: xvId,  toPort: "midi" },
        { id: newId(), from: mpcId, fromPort: "main", to: "sys_out", toPort: "main" },
        { id: newId(), from: xvId,  fromPort: "main", to: "sys_out", toPort: "main" }
      ]
    };
  },

  recommendedQuality: (caps) => ({
    qualityTier: caps.hasWorklet ? "studio" : "live",
    backend: "auto"
  })
};

HARDWARE_AUTO_RIGS[HARDWARE_AKAI_MPK_MINI] = MPK_MINI_RECIPE;
```

### 4. Verify

Plug the device in, click **Connect hardware**, pick the row, and confirm that
the starter rig appears with cables already wired into `MASTER OUT`.

## Per-instrument auto-rig

A recipe doesn't have to belong to a *physical* device. To make any new
instrument participate in the same flow when it ships its own recommended
chain, register a recipe with a synthetic `hardwareVisualId` (say,
`"instrument_default_xv1000"`) and have your module's metadata reference it.

`useMidiPatch` will look it up the same way; the only difference is that the
hardware visual ID is computed from the instrument rather than from the OS
port name. This is how we route "I added a MultiBank XV-1000" → "wire it to
MASTER OUT and pick Studio quality" without needing a real keyboard plugged
in.

## Design rules

- **Recipes are pure data.** No React hooks, no DOM access, no audio context
  side effects. They produce graph nodes/edges and (optionally) read audio
  capabilities to pick a quality tier.
- **Use `newId()`.** Always allocate ids via the helper passed in. Hardcoded
  ids will collide on the second `Connect hardware` click.
- **Don't override factory presets.** Each instrument ships its own
  `metadata.defaultParams` and `metadata.presets`. Recipes spawn the rack
  *with* those defaults — they don't mutate them.
- **Empty-board only for `starterRig`.** Adding a device to a populated patch
  board only spawns the layout (if any) — never the full rig — so the user's
  existing chain is preserved.
- **Quality is opt-in.** `recommendedQuality` is only consulted when the
  starter rig actually fires (empty board). User-set Studio/Live/Lite is
  preserved otherwise.
- **Keep `displayName` synced** with `hardwareDisplayNameForId` in
  `midiHardwareVisuals.tish` so logs and UI badges agree.

## Where SMK-25 lives now

The SMK-25 family — historically the only recipe — moved out of inline code
in `useMidiPatch.tish` and into the registry as `SMK25_FAMILY_RECIPE` in
`hardwareAutoRig.tish`. Behaviour is unchanged: combo merge → layout panel +
3 port edges → on empty board, semantic splitter + Keys/Pads analog poly +
sys_out cables, with Studio (or Live without worklet) auto-selected. The
recipe is the canonical reference for everything new.
