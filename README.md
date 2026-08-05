<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/wordmark-dark.png">
  <source media="(prefers-color-scheme: light)" srcset="assets/wordmark-light.png">
  <img alt="Lumagen" src="assets/wordmark-dark.png" width="440">
</picture>

### AI material studio for seamless, engine-ready PBR textures

Describe a material — Lumagen generates a seamless albedo and derives the
**full, pixel-aligned PBR map set**, ready to drop straight into your engine.

![License](https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-6f57c9)
![Platform](https://img.shields.io/badge/platform-Windows%20%C2%B7%20macOS%20%C2%B7%20Linux-2b2d34)
![Built with Rust](https://img.shields.io/badge/built%20with-Rust%20%C2%B7%20egui%20%C2%B7%20wgpu-00c9e8)

<img src="assets/tour.gif" alt="Lumagen — from library to a finished, exported material" width="900">

</div>

## What is Lumagen?

Lumagen is a desktop app for 3D and game artists who need **complete, tileable
PBR materials** without hand-assembling eight separate texture maps.

Type a description (or bring your own albedo) and Lumagen gives you a full
material — albedo, roughness, metallic, normal, displacement, ambient occlusion,
emission, and transparency — with **every map pixel-aligned to the albedo** and
exported in the layout your engine expects. It runs **fully offline out of the
box**; plug in a provider key when you want AI-generated albedos.

## Why artists use it

- **One description → a whole material.** No more sourcing or authoring eight
  maps by hand for every surface.
- **Everything lines up.** The geometry maps (normal, displacement, AO,
  opacity) are *computed* from the albedo, so they are pixel-perfect aligned and
  seam-safe — never the misregistered mess you get from generating each map
  independently.
- **Seamless by default.** Output is tileable, with a built-in 2×2 / 3×3 seam
  check so you can verify before you ship.
- **Works with no account.** The offline mock engine runs the entire pipeline
  with zero API key and zero network — perfect for trying the whole flow.
- **Exports the way your engine wants.** Per-map PNG + 16-bit/EXR displacement,
  ORM / mask-map channel packing, OpenGL ↔ DirectX normal flip, your own
  filename pattern, plus a contact sheet and `manifest.json`.
- **Non-destructive refine.** Tune detail and fidelity live, re-roll variations,
  and upscale — without rebuilding the set.

## How it works

### 1 · Organize your materials

![Lumagen library — material grid with per-map coverage pips](assets/screenshot-library.png)

Your library groups materials by project, with at-a-glance **coverage pips** —
how many of the eight maps each material has — and target resolution. Point
Lumagen at a folder of loose maps and it groups them into a material for you.

### 2 · Describe the material

![Lumagen new-material wizard — prompt, material type, seed](assets/screenshot-create.png)

Start from a text prompt and a **material type** (metal, painted, glass, fabric,
stone, wood, or custom). Add a negative prompt, pick a seed, and choose the
export size. Whatever you generate is always seamless and tileable.

### 3 · Generate the full map set

![Lumagen workspace — all eight PBR channels generated from one albedo](assets/screenshot-maps.png)

From a single source albedo, Lumagen produces **all eight channels at once**:
the semantic maps (roughness, metallic, emission) come from your provider, while
the geometry maps are computed locally so they stay perfectly registered to the
albedo.

### 4 · Refine and check seams

![Lumagen tiled seam-check with the live adjust panel](assets/screenshot-refine.png)

Tile the preview **2×2 or 3×3** to confirm the material is seamless. Dial in
fidelity-to-albedo and detail per map, re-generate, or branch off variations —
all non-destructive and live.

### 5 · Export engine-ready

![Lumagen export dialog — engine preset, normal convention, filename pattern](assets/screenshot-export.png)

Choose the engine preset and export: per-map PNGs, a 16-bit or 32-bit-float EXR
displacement, a contact sheet, and a `manifest.json` — all named by your own
pattern (`$material_$map_$res`).

## The map set

Lumagen treats one map as the anchor and keeps the rest aligned to it:

| Map | Role | How it's produced |
| --- | --- | --- |
| **Albedo** | Base color — the anchor | AI-generated or imported |
| **Roughness** | Surface response | AI-generated |
| **Metallic** | Surface response | AI-generated |
| **Normal** | Geometry | Computed from the albedo |
| **Displacement** | Geometry (height) | Computed |
| **Ambient Occlusion** | Geometry | Computed |
| **Emission** | Optical | AI-generated |
| **Transparency** | Optical (opacity) | Computed |

Because the geometry maps are derived rather than generated, they line up with
the albedo by construction — no VAE drift, no network round-trip, and instant.

## Providers

Lumagen ships with three backends:

- **Offline mock** *(default)* — runs the full pipeline procedurally with no key
  and no network.
- **[fal.ai](https://fal.ai)** — the default AI backend, with first-class access
  to control/edit models (FLUX Kontext, Qwen-Image-Edit) that keep maps aligned.
- **[OpenRouter](https://openrouter.ai)** — image generation over chat
  completions.

API keys are stored in your **operating system's credential vault** (Windows
Credential Manager, macOS Keychain, or libsecret) — never written to disk in
app settings or config files. Add a key any time under **Settings → Providers**;
until you do, Lumagen simply uses the offline engine.

## Install and run

Lumagen is a native Rust app. With a [Rust toolchain](https://rustup.rs)
installed (the pinned version in `rust-toolchain.toml`, edition 2024):

```sh
cargo run --release
```

### Build a Windows installer

```sh
cargo bundle --release --format msi
```

## License

Dual-licensed under either [MIT](LICENSE-MIT) or
[Apache 2.0](LICENSE-APACHE), at your option.
