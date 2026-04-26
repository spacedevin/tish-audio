# Rack Bundle Spec (Stage 1)

A rack bundle is data-first and references built-in functions by id.

## manifest.json

Required keys:

- `id`
- `name`
- `version`
- `era`
- `requirements`
- `paintSceneRef`
- `audioBuildRef`
- `presets` (array)
- `license`

Optional:

- `sampleBanks` (array)

## Sources

Allowed source descriptors for loaderRuntime:

- `fs` path (local JSON)
- `url` (remote JSON)
- `bundle` (pre-resolved object)

No arbitrary code execution in stage 1.

## Theme token requirement

Racks should use token names from `src/render/themeTokens.tish`, not raw color hex values.

## IP guidance

- Homage to eras and workflows is okay.
- Do not use trademarked product names or logos.
- Do not ship factory ROM/sample dumps from third-party hardware.
