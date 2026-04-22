# parley_term extraction plan

`parley_term` is the right extraction boundary for CuTTY's renderer if the
longer-term goal is a reusable backend for projects like `soft_ratatui`.

## Why not move `cutty/src/display` as-is?

CuTTY's current display stack still mixes three concerns:

- `cutty/src/display/window.rs`: `winit` window ownership and platform details.
- `cutty/src/display/renderer.rs`: `vello` surface and present path.
- `cutty/src/display/mod.rs`: terminal adaptation, scene construction, IME/search UI, damage
  tracking, and CuTTY-specific overlays.

That is too app-specific for a library backend. A direct move would force
downstream users to depend on CuTTY's event model, config types, and windowing.

## Extraction target

`parley_term` keeps only the reusable pieces:

- A generic cell grid model (`TerminalGrid`, `TerminalCell`, `SceneCursor`).
- A font/text system built on `parley`.
- Scene emission for cell backgrounds, glyphs, cursor shapes, and text decorations.
- An optional `vello`/`wgpu` surface presenter for apps that want a ready-made window target.

## Intended adapters

The new crate is meant to sit underneath two adapter layers:

1. CuTTY adapter:
   Converts `cutty_terminal::Term` render state into `parley_term::TerminalGrid`
   plus CuTTY-only overlays like search bars, hyperlink previews, IME preedit,
   and damage/debug rectangles.

2. `soft_ratatui` adapter:
   Converts a `ratatui`/`soft_ratatui` buffer into `parley_term::TerminalGrid`.
   This is the layer that should handle style translation, symbols/graphemes,
   and any ratatui-specific background/selection conventions.

## Suggested next refactor in CuTTY

After this crate lands, the next step is to move CuTTY to an explicit adapter:

- Keep `Display` as the owner of window state, IME state, damage, and scheduling.
- Add a conversion step from CuTTY renderable cells to `parley_term::TerminalGrid`.
- Replace CuTTY's local scene building with `parley_term::SceneBuilder`.
- Leave message bar/search/IME/hint overlays in CuTTY for now and pass them in as overlay rects
  or a second scene pass.

That keeps CuTTY functional while shrinking `cutty/src/display/mod.rs` toward adapter code
instead of renderer ownership.
