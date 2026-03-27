# ralk

Motor 3D Rust + Vulkan, nativo Linux + macOS (MoltenVK).

## Build

```bash
cargo run                              # debug con validation layers
cargo run --release                    # release
WINIT_UNIX_BACKEND=x11 cargo run       # forzar X11 (RenderDoc en Linux)
```

## Docs (leer cuando sea relevante, no antes)

- `docs/architecture.md` → estado actual del código, tipos, render loop
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

## Estado actual

Motor funcional con sandbox de exploración en primera persona.

**Decisiones tomadas:**
- SSAO normals: reconstrucción desde depth (no MRT)
- GPU-driven: agrupado por material (no bindless)
- LOD transition: hard switch
- Scripting: mlua (Lua 5.4)

**Player:** capsule body dinámico en rapier (rotación bloqueada). Input → velocidad XZ, gravedad en Y la maneja rapier. Cámara sigue posición del capsule + eye offset.

**UI:** pantalla limpia durante exploración. Escape → sidebar izquierdo con todas las secciones de debug (colapsables).

**Sandbox:** `spawn_sandbox_scene()` — piso 40×40, paredes, pilares, obstáculos estáticos, cubos dinámicos, instancias de modelos GLB con física.

**Builtin cube:** siempre appendado al final de `load_multi_glb`. `cube_mesh_index = meshes.len() - 1` siempre válido.
