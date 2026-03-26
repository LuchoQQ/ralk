# vibe-engine

Motor 3D Rust + Vulkan, nativo Linux + macOS (MoltenVK).

## Build

```bash
cargo run                              # debug con validation layers
cargo run --release                    # release
WINIT_UNIX_BACKEND=x11 cargo run       # forzar X11 (necesario para RenderDoc en Linux)
```

## Docs (leer cuando sea relevante, no antes)

- `docs/architecture.md` → estado actual del código, tipos, flujo de datos, render loop
- `docs/milestone-1.md` → M1 completado (Vulkan init → PBR → sombras)
- `docs/milestone-2.md` → M2 completado (ECS → skybox → egui → hot-reload → MSAA)
- `docs/milestone-3.md` → M3 activo (multi-modelo, physics, audio, render graph, bloom, gizmos)
- `docs/prompts-m3.md` → prompts por fase del M3
- `docs/gotchas.md` → errores que se repiten
- `docs/stack-decisions.md` → por qué cada herramienta

## Reglas del stack (no cambiar sin justificación documentada)

- **ash 0.38** directo — NO wgpu, NO vulkano
- **Vulkan 1.2+** mínimo, dynamic rendering — NO VkRenderPass objects
- **glam 0.29** para math — NO nalgebra en hot path
- **GLSL → SPIR-V** vía shaderc — NO WGSL
- **hecs** para ECS — ya integrado en M2

## Reglas de código

- `#[repr(C)]` + `Pod` + `Zeroable` en todo struct que va a GPU
- `unsafe` blocks mínimos con `// SAFETY:`
- Handles opacos para recursos GPU, nunca exponer Vulkan raw handles fuera de `src/engine/`
- `anyhow::Result` en init/assets, nunca `.unwrap()` en render loop

## Milestone actual: M3 "El mundo vive"

Fases 15-20. Ver `docs/milestone-3.md` para checkboxes y `docs/prompts-m3.md` para prompts.

## Decisiones pendientes

- [ ] ¿Render graph: trait RenderPass o structs con callbacks? → decidir en fase 18
- [ ] ¿Gizmo rendering: mesh propio o line list? → decidir en fase 20
- [ ] ¿Bloom: 13-tap o bilinear downsample? → decidir en fase 19