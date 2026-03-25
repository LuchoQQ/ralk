# ralk

Motor 3D Rust + Vulkan, nativo Linux + macOS (MoltenVK).

## Build

```bash
cargo run                              # debug con validation layers
cargo run --release                    # release
WINIT_UNIX_BACKEND=x11 cargo run       # forzar X11 (necesario para RenderDoc en Linux)
```

Shaders se compilan en `build.rs` con shaderc. Primera build tarda ~3 min, después se cachea.

## Docs (leer cuando sea relevante, no antes)

- `docs/architecture.md` → estado actual del código, tipos, flujo de datos, render loop
- `docs/milestone-1.md` → fases completadas del M1 (referencia histórica)
- `docs/milestone-2.md` → fases del milestone actual, criterios de éxito, prompts
- `docs/gotchas.md` → bugs reales de Vulkan/ash/Linux/macOS que se repiten
- `docs/stack-decisions.md` → qué herramientas usamos y por qué
- `docs/strategy.md` → plan original de 12 fases con prompts de vibecoding

## Reglas del stack (no cambiar sin justificación documentada)

- **ash 0.38** directo — NO wgpu, NO vulkano
- **Vulkan 1.2+** mínimo, dynamic rendering — NO VkRenderPass objects
- **glam 0.29** para math — NO nalgebra en hot path
- **GLSL → SPIR-V** vía shaderc — NO WGSL

## Reglas de código

- `#[repr(C)]` + `Pod` + `Zeroable` en todo struct que va a GPU
- `unsafe` blocks mínimos con `// SAFETY:` explicando por qué
- Handles opacos para recursos GPU, nunca exponer Vulkan raw handles fuera de `src/engine/`
- `anyhow::Result` en init/assets, `Result` tipado en render loop, nunca `.unwrap()` en render loop

## Estructura

```
src/engine/   → Vulkan init, recursos GPU, pipelines, sync
src/scene/    → cámara, lights, ECS (M2), culling (M2)
src/render/   → (futuro) render passes separados
src/asset/    → glTF loader, shader compiler (M2)
src/input/    → teclado/mouse (winit), gamepad (gilrs, M2)
src/ui/       → debug UI (egui, M2)
shaders/      → GLSL, compilados a SPIR-V en build time + hot-reload (M2)
```

## Milestone actual: M2 "El motor respira"

Fases 9-14. Ver `docs/milestone-2.md` para detalle completo.

## Decisiones pendientes

- [ ] ¿rust-gpu para shaders? → evaluar post-M2
- [ ] ¿egui-ash o integración manual egui-winit? → decidir en fase 11
- [ ] ¿Shadow map fit-to-frustum? → bonus M2 o M3