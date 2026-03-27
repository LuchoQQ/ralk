# vibe-engine

Motor 3D Rust + Vulkan, nativo Linux + macOS (MoltenVK).

## Build

```bash
cargo run                              # debug con validation layers
cargo run --release                    # release
WINIT_UNIX_BACKEND=x11 cargo run       # forzar X11 (RenderDoc en Linux)
```

## Docs (leer cuando sea relevante, no antes)

- `docs/architecture.md` → estado actual del código, tipos, render loop
- `docs/milestone-1.md` → M1 completado (Vulkan → PBR → sombras)
- `docs/milestone-2.md` → M2 completado (ECS → skybox → egui → MSAA)
- `docs/milestone-3.md` → M3 completado (physics → audio → render graph → bloom → gizmos)
- `docs/milestone-4.md` → M4 activo (SSAO, profiling, GPU-driven, LOD, async assets, scripting)
- `docs/prompts-m4.md` → prompts por fase del M4
- `docs/gotchas.md` → errores que se repiten

## Reglas del stack (no cambiar sin justificación)

- **ash 0.38** directo — NO wgpu, NO vulkano
- **Vulkan 1.2+**, dynamic rendering — NO VkRenderPass objects
- **glam 0.29** — NO nalgebra en hot path
- **GLSL → SPIR-V** vía shaderc — NO WGSL
- **hecs** para ECS, **rapier3d** para physics, **rodio** para audio
- **Render graph** para todos los passes — NO barriers manuales en record_command_buffer

## Reglas de código

- `#[repr(C)]` + `Pod` + `Zeroable` en todo struct que va a GPU
- `unsafe` blocks mínimos con `// SAFETY:`
- Handles opacos para recursos GPU fuera de `src/engine/`
- Nunca `.unwrap()` en render loop
- Passes nuevos se agregan al render graph, no al command buffer directo

## Milestone actual: M4 "Production ready"

Fases 21-26. Ver `docs/milestone-4.md` para checkboxes y `docs/prompts-m4.md` para prompts.

## Decisiones pendientes

- [ ] ¿SSAO normals: MRT o reconstrucción desde depth? → decidir en fase 21
- [ ] ¿GPU-driven: bindless textures o agrupar por material? → decidir en fase 23
- [ ] ¿LOD transition: dithered crossfade o hard switch? → decidir en fase 24
- [ ] ¿Scripting: mlua (Lua 5.4) o rhai (Rust-native)? → decidir en fase 26