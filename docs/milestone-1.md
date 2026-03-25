# Roadmap — ralk

## Filosofía

Objetivos cortos, demostrables, que prueban que la base es sólida. Cada milestone produce algo visible. Si no podés mostrarlo en pantalla, no está terminado.

---

## Milestone 1: "La escena viva" ✅ COMPLETO (2026-03-25)

**Objetivo:** Abrir el motor y ver una escena 3D con un modelo glTF iluminado, texturas PBR, sombras, y poder caminar libremente con WASD + mouse. Una demo técnica que demuestre que el motor funciona de punta a punta.

**Resultado:** Pipeline Vulkan completo en macOS (MoltenVK) y Linux. Carga DamagedHelmet.glb con texturas PBR Cook-Torrance, sombras direccionales PCF 3×3, cámara FPS libre, depth buffer correcto. 2 render passes por frame (shadow + main). Validation layers limpias.

**Decisiones tomadas:**
- `orthographic_rh` de glam 0.29 ya produce [0,1] depth (no hay `_zo` en esta versión)
- `perspective_rh_zo` tampoco existe — se construye manualmente la matriz en `camera.rs`
- Tangentes defaultean a `[1,0,0,1]` cuando el glTF no las incluye (suficiente para la mayoría de meshes)
- Descriptor pool de lighting incluye `COMBINED_IMAGE_SAMPLER` para el shadow map (binding 1)

### Fase 1 — Vulkan init + ventana (semana 1)

Criterio de éxito: ventana que muestra un color que cambia.

- [x] winit abre ventana 1280x720 en Wayland (fallback X11)
- [x] Vulkan instance con validation layers en debug
- [x] Selección de GPU física (preferir discreta)
- [x] Logical device + graphics queue
- [x] Swapchain triple buffering FIFO
- [x] Clear pass: pantalla azul oscuro
- [x] Resize de ventana recrea swapchain sin crash

### Fase 2 — Triángulo (semana 1-2)

Criterio de éxito: triángulo de colores RGB en pantalla.

- [x] Vertex + fragment shader en GLSL, compilados a SPIR-V en build.rs
- [x] Graphics pipeline con dynamic rendering
- [x] Vértices hardcodeados en el shader (vertex buffer real en Fase 3)
- [x] Command buffer: begin → begin rendering → bind pipeline → draw → end
- [x] Sincronización: fence por frame, semáforos acquire/present (hecho en Fase 1)
- [x] Frames in flight (2 frames simultáneos, hecho en Fase 1)

### Fase 3 — Recursos GPU (semana 2)

Criterio de éxito: el triángulo usa el resource manager, validation layers sin warnings.

- [x] `GpuResourceManager` con gpu-allocator
- [x] Crear vertex buffer vía API simple (index/uniform en fases posteriores)
- [x] Staging buffer → device local transfer para uploads
- [x] Handles opacos (`BufferHandle`, `ImageHandle`)
- [x] Cleanup en Drop sin leaks

### Fase 4 — Cámara 3D (semana 2-3)

Criterio de éxito: moverte alrededor del triángulo viéndolo desde todos los ángulos.

- [x] `Camera3D` con glam: posición, pitch, yaw
- [x] View matrix (lookAt) y Projection matrix (perspectiva)
- [x] WASD relativo a dirección de cámara + sprint con Shift
- [x] Mouse delta → rotación (pitch clamp ±89°)
- [x] Delta time para movimiento frame-independent
- [x] MVP pasada al shader via push constants (64 bytes)

### Fase 5 — Carga de modelos glTF (semana 3)

Criterio de éxito: modelo 3D de Khronos samples visible en pantalla, caminable.

- [x] Parsear .glb con crate gltf
- [x] Extraer posiciones, normales, UVs, índices
- [x] Crear vertex + index buffers por mesh
- [x] Model matrix por instancia (posición/rotación/escala en el mundo)
- [x] `vkCmdDrawIndexed` por mesh
- [x] Fallback a cubo builtin si no hay glTF en assets/; DamagedHelmet/Sponza como targets

### Fase 6 — Iluminación Blinn-Phong (semana 3-4)

Criterio de éxito: objetos con volumen visual, sombra suave, brillo especular.

- [x] Fragment shader Blinn-Phong: ambient + diffuse + specular
- [x] Uniform buffer con datos de luz (posición, color, intensidad)
- [x] Una luz direccional + una point light
- [x] Posición de cámara en el UBO para specular
- [x] Normal matrix calculada en shader (transpose(inverse(mat3(model))))
- [x] Descriptor set para el UBO de luces (set 0, binding 0, fragment stage)

### Fase 7 — Texturas y PBR básico (semana 4-5)

Criterio de éxito: DamagedHelmet de Khronos con texturas PBR correctas.

- [x] Carga de imágenes desde glTF (sin crate image extra — conversión manual R8G8B8→RGBA8)
- [x] VkImage + VkImageView + VkSampler por textura (sampler compartido, linear/repeat)
- [x] Upload vía staging buffer + layout transitions (UNDEFINED→TRANSFER_DST→SHADER_READ_ONLY)
- [x] Fragment shader PBR metallic-roughness Cook-Torrance GGX (D_GGX, G_Smith, F_Schlick)
- [x] Normal mapping vía TBN matrix (tangent en vertex shader, handedness glTF)
- [x] Un descriptor set por material (set 1: albedo + normal + MR); set 0 = lighting UBO
- [x] Materiales extraídos de glTF automáticamente; texturas default 1×1 para slots ausentes
- [x] `SceneData { meshes, textures, materials }` reemplaza `Vec<MeshData>`

### Fase 8 — Depth, multi-objeto, sombras (semana 5-6)

Criterio de éxito: escena con 5+ objetos, sombras direccionales, sin z-fighting.

- [x] Depth buffer (D32_SFLOAT o D24_S8 según soporte) — create_attachment_image, transición inicial one-shot
- [x] Depth test en pipeline — CompareOp::LESS, depth write habilitado, depth format en PipelineRenderingCreateInfo
- [x] Renderizar múltiples objetos con transforms distintos — SceneInstance por mesh, model matrix individual
- [x] Shadow map pass: depth desde perspectiva de la luz direccional (2048×2048 ortográfica)
- [x] Sampling de shadow map en el fragment shader principal (PCF 3×3, comparison sampler)
- [ ] Frustum culling básico (AABB vs view frustum) → movido a Milestone 2

### Entregable final del Milestone 1 ✅

Escena con modelo glTF PBR (DamagedHelmet), sombras suaves, cámara FPS libre, depth correcto.
Vulkan nativo en macOS (MoltenVK) y Linux, validation layers limpias, 60+ FPS.

**Esto prueba:** el pipeline completo funciona. Cada fase posterior es un feature encima de esta base, no una reescritura.

---

## Milestone 2: "El mundo interactivo" (pendiente de planificación)

**Objetivo:** Convertir la demo técnica en algo que se parece a un motor real. Agregar estructura (ECS liviano), usabilidad (debug UI, hot-reload), y calidad visual (IBL, skybox, MSAA).

### Candidatos para Milestone 2

Prioridad alta — se necesitan para escalar el contenido:
- **ECS mínimo:** `hecs` o shipyard para gestionar entidades, transforms, y materiales sin `Vec<SceneInstance>` hardcodeado
- **Frustum culling:** AABB vs view frustum (quedó afuera del M1)
- **Render graph básico:** abstracción de passes con dependency tracking y barriers automáticas
- **Skybox:** cubemap HDR + IBL (ambient specular/diffuse desde environment map)

Prioridad media — mejora la experiencia de desarrollo:
- **Debug UI:** egui overlay con FPS counter, tweakers de luces, posición de cámara
- **Hot-reload shaders:** watcher sobre `shaders/`, recompila y recrea pipelines sin reiniciar
- **Fixed timestep:** separar update physics de render (game loop robusto)
- **Múltiples modelos en escena:** colocar N instancias de M meshes con transforms distintos

Prioridad baja — nice to have:
- **MSAA 4x:** calidad visual sin cambios grandes en el pipeline
- **Gamepad:** integración con `gilrs`
- **Audio básico:** `rodio` para efectos de sonido simples
- **Escena serializada:** guardar/cargar desde JSON

### Decisiones pendientes que hay que tomar antes de M2

- ¿ECS propio o `hecs`/`shipyard`? → `hecs` es más simple para empezar
- ¿Render graph explícito o seguir con passes hardcodeadas? → render graph si hay más de 3 passes
- ¿egui-ash o imgui-rs? → egui-ash tiene mejor integración con ash directo

---

## Principios del roadmap

1. **Cada fase tiene un test visual.** Si no lo ves en pantalla, no está terminado.
2. **No adelantar fases.** La fase 7 asume que la 6 funciona. Saltear genera bugs fantasma.
3. **Commit por fase.** Si la fase 8 rompe algo, `git checkout` a fase 7 y reintentar.
4. **El Milestone 1 no incluye ECS, render graph, ni audio.** Esas son complejidades que se agregan sobre una base probada, no antes.
5. **"Funciona feo" es mejor que "no funciona bonito."** Primero que ande, después que sea elegante.