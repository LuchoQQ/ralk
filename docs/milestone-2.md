# Milestone 2: "El motor respira" ✅ COMPLETO (2026-03-26)

**Objetivo:** Motor con ECS, skybox HDR + IBL, debug UI interactivo, hot-reload de shaders, frustum culling, gamepad, MSAA 4x, mipmaps.

**Resultado:** Escena con 3+ modelos glTF via hecs, skybox HDR con IBL split-sum completo (irradiance + prefiltered env + BRDF LUT), egui con sliders de luces en vivo, hot-reload de shaders en <1s, frustum culling Gribb/Hartmann con stats, fixed timestep 60Hz, gilrs gamepad, MSAA 4x con toggle, mipmaps, tone mapping ACES switcheable. Validation layers limpias.

**Decisiones tomadas:**
- `hecs 0.10` para ECS (liviano, sin macros)
- IBL precomputado CPU-only en `skybox.rs` (parser RGBE, importance sampling GGX)
- `egui-ash-renderer` con dynamic rendering (sin VkRenderPass)
- `notify` para file watcher + `shaderc` runtime para hot-reload
- Culling Gribb/Hartmann: 6 planos de view-proj, near = row2 (convención Vulkan)
- MSAA images arrancan UNDEFINED cada frame, barrier al inicio del command buffer
- Tone mapping switcheable via `cameraPos.w` (sin cambio de UBO layout)
- Sampler `max_lod = 1000.0` para mipmaps

| Fase | Descripción | Estado |
|------|-------------|--------|
| 9 | ECS con hecs (reemplazó SceneInstance) | ✅ |
| 10 | Skybox HDR + IBL split-sum | ✅ |
| 11 | Debug UI egui (stats, sliders, input routing) | ✅ |
| 12 | Hot-reload shaders (notify + shaderc runtime) | ✅ |
| 13 | Frustum culling + fixed timestep + gamepad | ✅ |
| 14 | MSAA 4x + mipmaps + tone mapping ACES | ✅ |

**Siguiente:** → `docs/milestone-3.md`