# ralk — vibe-engine

A Vulkan-first 3D engine written in Rust, with a playable driving game as its reference implementation.

No wgpu. No abstraction layers. Direct `ash` bindings to Vulkan 1.2+.

---

## Example game — Ralk Racing

A lap-racing game that exercises every engine feature: PBR lighting,
GPU-driven rendering, physics, spatial audio, day/night cycle, Lua scripting,
and a full game-state machine.

```bash
cargo run --example driving_game           # debug
cargo run --example driving_game --release # release (recommended)
```

**Controls:** `W/S` throttle/brake · `A/D` steer · `Space` handbrake · `Esc` pause

See [`examples/driving_game/README.md`](examples/driving_game/README.md) for full controls and gameplay guide.

---

## Engine features

| System | Description |
|--------|-------------|
| **Renderer** | Vulkan 1.2+ dynamic rendering, no render-pass objects |
| **PBR** | Cook-Torrance BRDF, metallic-roughness, PCF shadow maps |
| **GPU-driven** | Compute-shader frustum cull + `vkCmdDrawIndexedIndirect` grouped by material |
| **LOD** | 4 levels via meshopt, selected per-instance in compute shader |
| **Post-process** | SSAO (depth-reconstruct normals), bloom (dual-KawaseSe chain), ACES/Reinhard tone-map |
| **MSAA** | 1×/2×/4× selectable at runtime |
| **ECS** | hecs — Transform, MeshRenderer, BoundingBox, physics & audio components |
| **Physics** | rapier3d — rigid bodies, collision contact events |
| **Audio** | rodio — spatial attenuation, pitch-shift (engine RPM), looping sinks |
| **Scripting** | mlua (Lua 5.4) — hot-reload, command-queue pattern |
| **Day/Night** | Piecewise-linear sun colour/intensity, sky tint push-constant |
| **Editor** | egui panels for every subsystem + gizmo-based entity picking/transform |
| **Async assets** | GLB parsed on background thread via `mpsc` channel |
| **Shader hot-reload** | `notify` file watcher → live GLSL recompile via shaderc |
| **Gamepad** | gilrs — analogue axes with dead-zone, any XInput/DS4 controller |

---

## Build

```bash
cargo build                    # compile (shaders compiled in build.rs)
cargo run                      # debug with Vulkan validation layers
cargo run --release            # release
WINIT_UNIX_BACKEND=x11 cargo run   # force X11 (useful for RenderDoc on Linux)
```

### Platform requirements

| Platform | Requirements |
|----------|-------------|
| **macOS** | MoltenVK (bundled with Xcode or via `brew install molten-vk`) |
| **Linux** | Vulkan 1.2+ driver: Mesa 22+ (RADV/ANV) or NVIDIA proprietary ≥ 515 |
| **Windows** | Not tested (contributions welcome) |

---

## Stack

| Layer | Crate | Version |
|-------|-------|---------|
| Vulkan bindings | `ash` | 0.38 |
| Window/input | `winit` | 0.30 |
| GPU allocator | `gpu-allocator` | 0.27 |
| Math | `glam` | 0.29 |
| ECS | `hecs` | 0.10 |
| Physics | `rapier3d` | 0.22 |
| Audio | `rodio` | 0.19 |
| Scripting | `mlua` (Lua 5.4) | 0.10 |
| UI | `egui` | 0.33 |
| Mesh processing | `meshopt` | 0.3 |
| Gamepad | `gilrs` | 0.11 |
| Shaders | GLSL → SPIR-V via `shaderc` | — |

---

## Project layout

```
src/
├── main.rs          App entry point, game loop, all game systems
├── asset/           glTF loader, async scene loading
├── audio/           AudioEngine (rodio), spatial sinks, WAV generation
├── engine/          VulkanContext, render graph, GPU profiler, pipelines
├── input/           InputState (keyboard + mouse + gamepad)
├── physics/         PhysicsWorld (rapier3d wrapper)
├── scene/           ECS components, camera, lighting, gizmos, culling
├── scripting/       ScriptEngine (mlua), hot-reload, command queue
└── ui/              egui panels and state structs

shaders/             GLSL source files (compiled by build.rs → *.spv)
assets/              Models (glTF), sounds (generated at runtime)
scripts/             Lua scripts loaded at runtime
docs/                Milestone docs, architecture notes
examples/
└── driving_game/    Game documentation and README
```

---

## Docs

- [`docs/architecture.md`](docs/architecture.md) — engine internals
- [`docs/milestone-4.md`](docs/milestone-4.md) — M4: SSAO, profiling, GPU-driven, LOD, async, scripting
- [`docs/milestone-5.md`](docs/milestone-5.md) — M5: driving game (active)
- [`assets/LICENSES.md`](assets/LICENSES.md) — asset licence declarations

---

## Licence

MIT
