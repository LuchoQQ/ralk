# Milestone 3: "El mundo vive" ✅ COMPLETO (2026-03-26)

**Objetivo:** Escena persistente, physics, audio, render graph, bloom, gizmos de editor.

**Resultado:** Multi-modelo via scene.json (serde), rapier3d physics con rigid bodies/colliders y debug wireframes, audio espacial con rodio (impactos por contact events, ambiente en loop), render graph declarativo con barriers automáticos, bloom HDR (downsample/upsample chain + composite pass), object picking via ray-AABB + gizmos translate/rotate/scale con drag en screen-space. Validation layers limpias.

**Decisiones tomadas:**
- Render graph: structs con callbacks (no trait objects)
- Gizmo rendering: line list con wireframe pipeline reutilizado, vertex buffer por frame (8KB)
- Bloom: bilinear downsample, 5 mip levels
- Object picking: ray-AABB (slab method), no rapier raycast
- Tone mapping movido a composite pass, `triangle.frag` trabaja en HDR linear
- Scene format: JSON con serde, modelos referenciados por path
- MSAA depth registrado con `depth_aspect(format)`, no DEPTH|STENCIL hardcodeado
- Editor sync: aplicar transform_changed antes de leer ECS (evita revert de sliders)
- Click en vacío recaptura mouse (fix WASD)

| Fase | Descripción | Estado |
|------|-------------|--------|
| 15 | Multi-modelo + scene.json (serde) | ✅ |
| 16 | Physics rapier3d (rigid bodies, colliders, debug wireframes) | ✅ |
| 17 | Audio rodio (spatial, contact events, ambiente) | ✅ |
| 18 | Render graph (passes declarativos, barriers automáticos) | ✅ |
| 19 | Bloom HDR (downsample/upsample chain, composite pass) | ✅ |
| 20 | Gizmos + object picking (ray-AABB, translate/rotate/scale) | ✅ |

**Siguiente:** → `docs/milestone-4.md`