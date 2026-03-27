# Asset Licenses

All assets distributed with this repository are either self-generated at
runtime or sourced under permissive open-source licences.

---

## 3D Models

| File | Source | Licence |
|------|--------|---------|
| `assets/DamagedHelmet.glb` | [glTF-Sample-Models](https://github.com/KhronosGroup/glTF-Sample-Models) by Khronos Group | [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/) |

---

## Audio

All audio files in `assets/sounds/` are **procedurally generated at runtime**
by `src/audio/mod.rs` on the first launch.  No pre-recorded samples are
shipped with the repository.  The generated files are:

| File | Description | Generator |
|------|-------------|-----------|
| `engine_loop.wav` | Sawtooth wave at 120 Hz with harmonics 1–6 | `write_engine_loop()` |
| `skid.wav` | 850 Hz AM-modulated buzz (45 Hz carrier, 1020 Hz partial) | `write_skid_loop()` |
| `wind_loop.wav` | Beating-frequency pattern (350 + 420 + 510 + 580 Hz) | `write_wind_loop()` |
| `impact.wav` | White-noise burst, 80 ms, exponential decay | `write_flat_sample()` |
| `ambient.wav` | Low-frequency tone blend (60 + 80 Hz), 4 s loop | `write_flat_sample()` |

These generated files are placed into `assets/sounds/` on first run and are
**not** version-controlled (they appear in `.gitignore` if added).  You may
replace them with any CC0 / royalty-free samples; the engine loads whatever
WAV files are present at that path.

Suggested free sources:
- [Freesound.org](https://freesound.org) (CC0 or CC BY)
- [OpenGameArt.org](https://opengameart.org) — "Audio" category
- [sonniss.com/gameaudiogpl](https://sonniss.com/gameaudiogpl) (GDC bundles, free for games)

---

## Skybox / IBL

The default skybox cubemap is loaded from the first `.hdr` or `.ktx2` file
found in `assets/`.  If none is present the engine falls back to a solid
colour background.

If you add a skybox HDR, verify its licence before distributing.
Recommended free sources:
- [Poly Haven](https://polyhaven.com/hdris) — CC0 HDRIs

---

## Engine source code

All Rust source code in `src/`, `build.rs`, and `shaders/` is authored
in-house and released under the **MIT Licence** (see `LICENSE` in the
repository root).
