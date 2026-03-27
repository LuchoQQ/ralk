# Sandbox — ralk example

A configurable sandbox built on top of **ralk**, a Vulkan-first 3D engine in Rust.
Demonstrates main menu, scene management, player jump, prop placement, and day/night cycle.

## Run

```bash
# From the repository root:
cargo run                                  # debug (validation layers on)
cargo run --release                        # release (~3–5× faster)
cargo run --example driving_game           # same binary via example entry point
cargo run --example driving_game --release
```

> **macOS / MoltenVK**: works out of the box.
> **Linux**: requires a Vulkan 1.2+ driver (Mesa 22+, NVIDIA ≥ 515).
> No assets to download — placeholder sounds are generated on first run.

---

## First run flow

1. Launch → **main menu** appears.
2. Click **Nueva escena** → settings screen → enter a name → **Crear escena**.
3. Scene loads with a flat 40 × 40 m ground.  Mouse auto-captured → start exploring.
4. **Esc** → pause sidebar with debug panels.
5. Exit → session auto-saved to `scenes/.last_session.json`.
6. Re-launch → **Continuar** restores exact position, time of day, and all placed props.

Alternatively, load `scenes/sandbox.json` from **Cargar escena** for a pre-built layout.

---

## Controls

| Input | Action |
|-------|--------|
| **W / S** | Move forward / backward |
| **A / D** | Strafe left / right |
| **Space** | Jump |
| **Shift** | Sprint |
| **Esc** | Pause (sidebar) / cancel placement |
| **G** | Toggle grid snap (editor mode) |
| **W / E / R** | Gizmo: Translate / Rotate / Scale *(mouse released)* |
| **Left-click** | Pick entity *(mouse released)* / place prop *(placement mode)* |

### Gamepad (XInput / DS4)

| Input | Action |
|-------|--------|
| Left stick | Move |
| Right stick | Camera look |
| South button (A/Cross) | Jump |

---

## Props placement

1. **Esc** → pause sidebar.
2. Open **Props (Tab)** section.
3. Select a prop from the catalog (loaded from `assets/props/default_props.json`).
4. Close sidebar → click on the ground to place.
5. Select placed prop → move with **W/E/R** gizmos.
6. **Delete** (sidebar button) to remove selected entity.
7. **Ctrl+Z** (sidebar button) to undo last placement.
8. Save via **Scene → Save** in the sidebar.

---

## Adding custom props

1. Create or download a glTF model (`.glb`).
2. Place it in `assets/` or any reachable path.
3. Add an entry to `assets/props/default_props.json`:

```json
{
  "id": "my_crate",
  "name": "My Crate",
  "model": "assets/my_crate.glb",
  "thumbnail": "",
  "physics": "dynamic",
  "collider": "box",
  "category": "objetos"
}
```

4. Load or create a scene that references `assets/props/default_props.json` as the catalog.
5. The prop appears in the Props panel immediately.

---

## Adding custom skyboxes

Drop a `.hdr` equirectangular panorama into `assets/skyboxes/`.
It will appear in the **Nueva escena** settings dropdown on next launch.

---

## Engine features demonstrated

| Feature | Notes |
|---------|-------|
| Vulkan PBR + shadow maps | Every frame |
| GPU-driven rendering (compute cull + indirect draw) | Main render pass |
| 4-level LOD system | Distance-based hard switch |
| SSAO | Reconstructed from depth (disabled with MSAA > 1) |
| Bloom | Post-process pass |
| MSAA 4× | Anti-aliasing |
| Day/night cycle | Sun rotates, sky tints, street lights toggle |
| ECS (hecs) | Transform, MeshRenderer, PhysicsBody, StreetLight … |
| Rapier 3D physics | Rigid bodies, ray-cast grounded detection, jump impulse |
| Spatial audio (rodio) | Footsteps, jump, land, placement, delete, impacts |
| Lua scripting (mlua) | Hot-reloaded scripts in `scripts/` |
| Async asset loading | GLB parsed on background thread |
| Shader hot-reload | Edit GLSL → reloads in ≤ 1 s |
| egui debug panels | Stats, GPU profiler, lights, LOD, day/night, props catalog |
| Gamepad support (gilrs) | Analog stick + jump button |
| Scene persistence | Auto-save on exit, exact restore on continue |
| Props catalog | External JSON — add content without touching code |

---

## Built with ralk

[ralk](../../README.md) — a Vulkan-first 3D engine in Rust.
`ash 0.38` · `glam 0.29` · `hecs` · `rapier3d 0.22` · `rodio 0.19` · `egui 0.33` · `mlua`
