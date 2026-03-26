# Milestone 1: "La escena viva" ✅ COMPLETO (2026-03-25)

**Objetivo:** Pipeline Vulkan completo con modelo glTF iluminado, texturas PBR, sombras, cámara FPS libre.

**Resultado:** DamagedHelmet.glb con PBR Cook-Torrance, sombras PCF 3×3, cámara libre. macOS (MoltenVK) + Linux. Validation layers limpias. 60+ FPS.

**Decisiones tomadas:**
- `orthographic_rh` de glam 0.29 ya produce [0,1] depth
- Perspectiva construida manualmente en `camera.rs` (no existe `perspective_rh_zo` en 0.29)
- Tangentes defaultean a `[1,0,0,1]` cuando el glTF no las incluye
- Descriptor pool de lighting incluye `COMBINED_IMAGE_SAMPLER` para shadow map (binding 1)

| Fase | Descripción | Estado |
|------|-------------|--------|
| 1 | Vulkan init + ventana + clear pass | ✅ |
| 2 | Triángulo (pipeline + dynamic rendering) | ✅ |
| 3 | GpuResourceManager (buffers, staging, handles) | ✅ |
| 4 | Cámara 3D (WASD + mouse, MVP push constants) | ✅ |
| 5 | Carga glTF (meshes, index buffers, transforms) | ✅ |
| 6 | Iluminación PBR Cook-Torrance (dir + point light) | ✅ |
| 7 | Texturas PBR (albedo, normal, MR, descriptor por material) | ✅ |
| 8 | Depth buffer + shadow map PCF 3×3 | ✅ (frustum culling → M2) |

**Siguiente:** → `docs/milestone-2.md`