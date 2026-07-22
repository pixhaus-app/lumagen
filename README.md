# Lumagen

**AI-assisted PBR material studio.** Describe a material, generate a seamless
albedo, and Lumagen derives a full, pixel-aligned PBR map set — then exports
engine-ready textures.

Lumagen is a native desktop app built on `eframe`/`egui` with a hand-rolled
`wgpu` material preview. The heavy lifting — the data model, procedural and
AI-driven map generation, document persistence, and export — lives in the
`lumagen-core` library crate, kept entirely free of UI code.

## What it does

- **One albedo → a complete map set.** Roughness, metallic, normal,
  displacement, AO, emission, and transparency, all pixel-aligned to the
  albedo.
- **Geometry is computed, not guessed.** Normal, AO, height, and opacity are
  derived deterministically from the albedo/height, so they stay perfectly
  registered — no VAE drift, no network round-trip, instant.
- **Pluggable providers.** [fal.ai](https://fal.ai) (default; async job queue
  with FLUX Kontext / Qwen-Image-Edit), [OpenRouter](https://openrouter.ai)
  (image over chat-completions), and a fully offline procedural **mock**
  provider that runs the whole pipeline with no key and no network.
- **Real-time preview.** The derived maps light a sphere / plane / cube in a
  Cook-Torrance GGX preview rendered straight to a `wgpu` target.
- **Engine-aware export.** Per-map PNGs, a 16-bit or 32-bit-float EXR
  displacement, a contact sheet, and a `manifest.json` — honoring per-engine
  normal conventions and ORM / mask-map channel packing.

## Workspace layout

| Crate          | Role                                                                    |
| -------------- | ----------------------------------------------------------------------- |
| `lumagen`      | The desktop binary — egui UI, `wgpu` preview, screens, and settings.    |
| `lumagen-core` | UI-free core — data model, generation, derivation, providers, export.   |

## Building

Requires the pinned toolchain in `rust-toolchain.toml` (Rust 1.97,
edition 2024).

```sh
# Run the app
cargo run --release

# Full check (matches CI)
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
```

### Packaging (Windows)

```sh
cargo bundle --release --format msi
```

The build script embeds the brand `.ico` and version info into the executable.

## Configuration

Provider API keys are stored in the operating system's credential vault, never
on disk in app settings or config files. Set them from **Settings → Providers**;
with no key configured, Lumagen falls back to the offline mock provider.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
