# ralk

A Vulkan-first 3D engine written in Rust.

No wgpu. No abstraction layers. Direct `ash` bindings to Vulkan 1.2+.

---

## Run

```bash
cargo run           # debug (Vulkan validation layers on)
cargo run --release # release
WINIT_UNIX_BACKEND=x11 cargo run  # force X11 (Linux/RenderDoc)
```

**Controls:** `W/A/S/D` move · mouse look · `Shift` sprint · `Esc` pause / sidebar

---

## Engine features

| System | Description |
|--------|-------------|
| **Renderer** | Vulkan 1.2+ dynamic rendering, no render-pass objects |
| **PBR** | Cook-Torrance BRDF, metallic-roughness, PCF shadow maps |
| **GPU-driven** | Compute-shader frustum cull + `vkCmdDrawIndexedIndirect` grouped by material |
| **LOD** | 4 levels via meshopt, selected per-instance in compute shader |
| **Post-process** | SSAO (depth-reconstruct normals), bloom (dual-Kawase chain), ACES/Reinhard tone-map |
| **MSAA** | 1×/2×/4× selectable at runtime |
| **ECS** | hecs — Transform, MeshRenderer, BoundingBox, physics & audio components |
| **Physics** | rapier3d — rigid bodies, character capsule, collision events |
| **Audio** | rodio — spatial attenuation, looping sinks, procedural WAV generation |
| **Scripting** | mlua (Lua 5.4) — hot-reload, command-queue pattern |
| **Day/Night** | Piecewise-linear sun colour/intensity, sky tint push-constant |
| **Editor** | egui sidebar + gizmo-based entity picking/transform |
| **Async assets** | GLB parsed on background thread via `mpsc` channel |
| **Shader hot-reload** | `notify` file watcher → live GLSL recompile via shaderc |
| **Gamepad** | gilrs — analogue axes with dead-zone, any XInput/DS4 controller |

---

## Platform requirements

| Platform | Requirements |
|----------|-------------|
| **macOS** | MoltenVK (bundled with Xcode or via `brew install molten-vk`) |
| **Linux** | Vulkan 1.2+ driver: Mesa 22+ (RADV/ANV) or NVIDIA proprietary ≥ 515 |
| **Windows** | Not tested |

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
├── main.rs       App entry point, game loop, all systems
├── asset/        glTF loader, async scene loading
├── audio/        AudioEngine (rodio), spatial sinks, WAV generation
├── engine/       VulkanContext, render graph, GPU profiler, pipelines
├── input/        InputState (keyboard + mouse + gamepad)
├── physics/      PhysicsWorld (rapier3d wrapper)
├── scene/        ECS components, camera, lighting, gizmos, culling
├── scripting/    ScriptEngine (mlua), hot-reload, command queue
└── ui/           egui sidebar and state structs

shaders/          GLSL source files (compiled by build.rs → *.spv)
assets/           Models (glTF), sounds (generated at runtime)
scripts/          Lua scripts loaded at runtime
docs/             Architecture notes, milestone docs
```

---

## Docs

- [`docs/architecture.md`](docs/architecture.md) — engine internals
- [`assets/LICENSES.md`](assets/LICENSES.md) — asset licence declarations

---

## Licence

MIT
