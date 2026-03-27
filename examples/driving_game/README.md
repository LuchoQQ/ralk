# Driving Game — ralk example

A minimal lap-racing game built on top of **vibe-engine** (ralk).
Demonstrates every major engine feature in one playable experience.

## Run

```bash
# From the repository root:
cargo run --example driving_game         # debug (validation layers on)
cargo run --example driving_game --release   # release (~3-5× faster)
```

> **macOS / MoltenVK**: works out of the box.
> **Linux**: requires a Vulkan 1.2+ driver (Mesa 22+, NVIDIA proprietary ≥ 515).
> No assets to download — placeholder sounds are generated on first run.

---

## Controls

| Input | Action |
|-------|--------|
| **W** | Throttle |
| **S** | Brake |
| **A / D** | Steer left / right *(camera yaw when mouse captured)* |
| **Space** | Handbrake — guaranteed tyre squeal at speed |
| **Shift** | Sprint (free-camera fast-move) |
| **Esc** | Pause / release mouse |
| **Left-click** | Click entity in editor mode (mouse released) |
| **W / E / R** | Gizmo: Translate / Rotate / Scale *(mouse released)* |

### Gamepad (XInput / DS4)

| Input | Action |
|-------|--------|
| Left stick Y | Forward / brake |
| Left stick X | Steer |
| Right stick | Camera look |

---

## Gameplay

1. Launch → **main menu** appears centred on screen.
2. Click **PLAY** → 4-second countdown (3 – 2 – 1 – GO!).
3. Drive through the 4 **checkpoints** in order (0 → 1 → 2 → 3/finish).
4. Cross the finish line after all checkpoints → **LAP COMPLETE** screen with your time.
5. Best time is saved to `best_lap.json` in the working directory and loaded on the next run.
6. Press **Esc** during a race to **pause** → RESUME / RESTART / MAIN MENU.

---

## Engine features demonstrated

| Feature | Where |
|---------|-------|
| Vulkan PBR + shadow maps | Every frame |
| GPU-driven rendering (compute cull + indirect draw) | Main render pass |
| 4-level LOD system | Far objects auto-switch LOD |
| SSAO (disabled with MSAA > 1) | Ambient occlusion pass |
| Bloom | Post-process pass |
| MSAA 4× | Anti-aliasing |
| Day/night cycle | Sun rotates, sky tints, street lights toggle |
| ECS (hecs) | Transform, MeshRenderer, Vehicle, Checkpoint, StreetLight … |
| Rapier 3D physics | Rigid bodies, collision impacts |
| Spatial audio (rodio) | Engine pitch (RPM), tyre squeal, wind, impacts |
| Lua scripting (mlua) | `scripts/game.lua` — hot-reloaded |
| Async asset loading | GLB parsed on background thread |
| Shader hot-reload | Edit GLSL → reloads in ≤ 1 s |
| egui debug panels | Stats, GPU profiler, lights, LOD, day/night, vehicle audio … |
| Gamepad support (gilrs) | Axis steering + camera look |

---

## Checkpoint layout (top view)

```
          CP 0 (0, 6)
             ●
            / \
           /   \
CP 3 ●---+     +--- CP 1
(finish)   \   /   (-7, 0)     (7, 0)
            \ /
             ●
          CP 2 (0, -6)
```

Trigger radius: **2.5 m** sphere.
Vehicle position proxy: camera position (full raycast-suspension vehicle pending Fase 27).

---

## Built with vibe-engine

[vibe-engine (ralk)](../../README.md) — a Vulkan-first 3D engine written in Rust.
`ash 0.38` · `glam 0.29` · `hecs` · `rapier3d` · `rodio` · `egui` · `mlua`
