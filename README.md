# flower-fractal-toy

A GUI toy written in Rust (Bevy) to display and play with phyllotaxis "flower fractals":
two breathing, slowly spinning layers of 5,000 colour-cycled seeds, each one a Tibetan
mantra syllable.

![sunflower-ezgif com-cut](https://github.com/user-attachments/assets/2f97ef40-e109-410f-9684-3577c12705cb)

## Run as a toy

```
cargo run --release
```

Or download a build from the releases. An egui **Controls** window exposes every
parameter per layer (seed count, divergence angle, dish shape, breathe and spin modes).

## Run as your macOS wallpaper

```
scripts/flower-paper install
```

That builds the release binary, copies it to `bin/flower-paper` (so the process shows
up under that name in Activity Monitor), writes a launchd agent and starts it. The window:

- sits just below the desktop icons and above the system desktop picture,
- covers the main screen, with no title bar or shadow,
- appears on every Space, ignores the mouse, and stays out of the Dock and Cmd-Tab,
- is capped at 30 fps (`FLOWER_FPS=60 scripts/flower-paper install` to change),
- moves at 3% of toy speed: a breath takes one to five minutes, the rim turns about a degree
  per second at most, and parameter drift plays out over many minutes
  (`FLOWER_TEMPO=0.2 scripts/flower-paper install` to change; `--tempo` on the binary, slider in the toy).

Other commands: `start`, `stop`, `restart`, `status`, `uninstall`, and `run` (foreground,
Ctrl-C to quit). A solid black desktop picture keeps the edges clean; `install` prints the
one-liner that sets it.

The binary itself accepts `--wallpaper` (`-w`) and `--fps N`; on Linux and Windows
`--wallpaper` only gives a borderless always-on-bottom window.

## Mantra glyphs (v0.3)

Every seed is a Tibetan (Uchen) syllable, cycling through, in seed order:
om mani padme hum, om vajrasattva hum, om ah hum, namo amitabha, namo buddhaya,
namo dharmaya, namo sanghaya. Each character's top points at the centre of the flower,
and its size follows the seed spacing so neighbours never overlap.

The glyphs live in `assets/tibetan_atlas.png`, compiled into the binary. Regenerate it
(after editing the syllable list, size or weight) with the macOS system font Kailasa:

```
swift scripts/gen-atlas.swift                      # writes assets/tibetan_atlas.png
swift scripts/gen-atlas.swift /tmp/preview.png --opaque   # black background, for eyeballing
```

`--screenshot FILE` saves the frame rendered three seconds after launch and exits, which
is how to check the look without a visible window.

## Performance notes (v0.2)

- Seeds share a 180-entry hue palette instead of one material each, so Bevy instances
  them: at most 180 draw calls per frame instead of 17,000.
- No light and no shadow maps (the materials are unlit; the old point light with shadows
  rendered the whole flower six extra times per frame for nothing).
- Tonemapping is explicitly off; colours are exact and the full-screen pass is skipped.
- Seed placement runs in parallel across all cores and only for layers that changed.
- Release profile uses thin LTO and a single codegen unit.
