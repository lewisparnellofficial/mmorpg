# mmorpg-editor-core

`mmorpg-editor-core` is a dependency-light spike for terrain authoring. It
contains the device-neutral and deterministic parts of a future Linux editor;
it deliberately has no GUI, windowing, tablet-driver, rendering, or file-I/O
dependencies.

## Scope

The crate currently provides:

- `NormalizedTabletSample`, which normalizes pressure and rotation to
  `0.0..=1.0`, tilt axes to `-1.0..=1.0`, and carries eraser and proximity
  state.
- `TabletPoint`, pairing a document-space coordinate with a normalized sample.
- `HeightMap`, a row-major grid of finite `f32` values with a configured
  inclusive minimum and maximum. Every constructor and mutation clamps sample
  values to those bounds.
- `TerrainDocument`, a small source document that owns a heightmap.
- `TerrainEditor`, which applies pressure-aware raise, lower, and smooth
  strokes and records each stroke as one undo/redo operation.
- Deterministic text save/load through `TerrainDocument::to_source` and
  `TerrainDocument::from_source`.

A future Qt or SDL shell can convert native tablet events into
`NormalizedTabletSample` and feed `TabletPoint` values into
`TerrainEditor::apply_stroke`. GUI and device integration remain outside this
crate by design.

## Brush behavior

Raise and lower strokes use pressure, radial falloff, and brush strength. A
pen marked as an eraser reverses raise/lower direction. A smooth stroke moves
samples toward the weighted neighborhood average. Samples outside the
document are ignored, and samples are always clamped by `HeightMap`.

The current brush is intentionally small and predictable. It does not yet
provide stamps, layers, masks, terrain streaming, collaboration, sculpt
falloff presets, or GPU acceleration. History stores before/after sample
buffers, which is suitable for this small spike but will need a more compact
delta representation for large production worlds.

## Source format

The source format is UTF-8 text with LF line endings and a fixed field order:

```text
MMORPG_EDITOR_TERRAIN 1
width 3
height 2
bounds 0.0 1.0
samples
0.0
0.5
1.0
0.25
0.75
0.0
end
```

There must be exactly `width * height` sample lines between `samples` and
`end`. Values are written with Rust's shortest round-tripping `f32` display,
so saving the same document produces the same bytes and load/save preserves
the sample bit patterns. The parser rejects unknown fields, malformed numbers,
extra data, non-finite values, invalid bounds, and dimensions that exceed the
crate's sample-count limit.

This is a source/interchange format for the spike, not a production asset
format. It is intentionally easy to inspect and version-control. A future
asset pipeline can add compression or a binary runtime format while retaining
the same validated document model.

## Commands

Run from this directory:

```bash
cargo fmt -- --check
cargo test
```

The crate has no third-party dependencies and is not part of the repository's
main workspace yet; that keeps this isolated spike independently buildable
until the editor application boundary is selected.
