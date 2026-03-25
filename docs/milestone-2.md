# Milestone 2: "El motor respira"

## Objetivo

Convertir la demo del M1 en un motor donde podés iterar en tiempo real: ECS para organizar la escena, skybox HDR para contexto visual, debug UI para modificar todo sin recompilar, y hot-reload de shaders para iterar gráficamente al instante.

## Criterio de éxito del milestone

Abrir el motor y ver: skybox HDR de fondo con IBL, 3+ modelos glTF posicionados vía ECS, sombras PCF, PBR con ambient del entorno, panel egui con FPS + sliders de luces + transforms editables, y poder editar `triangle.frag`, guardar, y ver el cambio sin reiniciar. MSAA 4x. Validation layers limpias. 60+ FPS.

---

## Fase 9 — ECS con hecs

**Qué cambia:** Reemplazar `Vec<SceneInstance>` de `vulkan_init.rs` y la lógica de escena ad-hoc de `main.rs` por un `hecs::World` como fuente de verdad.

**Archivos que se tocan:**
- `Cargo.toml` → agregar `hecs = "0.10"`
- `src/scene/` → nuevo `ecs.rs` con componentes
- `src/main.rs` → `App` pasa de tener `SceneInstance[]` a `hecs::World`
- `src/engine/vulkan_init.rs` → `draw_frame()` recibe iterador de `(Mat4, usize, usize)` en vez de `&[SceneInstance]`

**Componentes a crear en `src/scene/ecs.rs`:**
```rust
struct Transform { position: Vec3, rotation: Quat, scale: Vec3 }
  // → fn to_mat4(&self) -> Mat4

struct MeshRenderer { mesh_index: usize, material_set_index: usize }
  // mesh_index indexa en VulkanContext.meshes[]
  // material_set_index indexa en VulkanContext.material_descriptor_sets[]

struct DirectionalLight { direction: Vec3, color: Vec3, intensity: f32 }
struct PointLight { color: Vec3, intensity: f32, radius: f32 }
struct ActiveCamera;  // marker, se combina con Camera3D existente
```

**Flujo nuevo por frame:**
```
world.query::<(&Transform, &MeshRenderer)>()
  → para cada uno: push (transform.to_mat4(), mesh_index, material_set_index)
world.query::<(&Transform, &DirectionalLight)>()
  → llenar LightingUbo.dir_light_dir/color
world.query::<(&Transform, &PointLight)>()
  → llenar LightingUbo.point_light_pos/color
```

**Lo que desaparece:**
- `SceneInstance { mesh: GpuMesh, model: Mat4, material_set: vk::DescriptorSet }` → reemplazado por queries
- Construcción manual de instancias en `main.rs` → `world.spawn()`

**Criterio de éxito:** Misma visual que M1, 3 helmets en posiciones distintas, todo via `world.spawn()`.

**Prompt:**
```
Leé docs/architecture.md para entender la estructura actual.

Implementá ECS con hecs para reemplazar la escena hardcodeada.

1. Agregar hecs = "0.10" a Cargo.toml

2. Crear src/scene/ecs.rs con componentes: Transform (position: Vec3, rotation: Quat, scale: Vec3, método to_mat4()), MeshRenderer (mesh_index: usize, material_set_index: usize), DirectionalLight (direction: Vec3, color: Vec3, intensity: f32), PointLight (color: Vec3, intensity: f32, radius: f32), ActiveCamera (marker).

3. En src/main.rs: crear hecs::World en App. Después del VulkanContext::new(), spawnear:
   - 3 DamagedHelmet como (Transform, MeshRenderer) en [-3,0,0], [0,0,0], [3,0,0]
   - Luz direccional como (Transform, DirectionalLight) con valores de LightingState actual
   - Cámara como (Transform, ActiveCamera)

4. Modificar draw_frame() en vulkan_init.rs: en vez de &[SceneInstance], recibir &[(Mat4, usize, usize)]. main.rs construye ese Vec haciendo query de (Transform, MeshRenderer).

5. Update de LightingUbo: query de DirectionalLight y PointLight en vez de LightingState.

6. Eliminar SceneInstance y Vec<SceneInstance> de vulkan_init.rs.

No cambies shaders, pipelines, ni estructura de VulkanContext. Solo la fuente de datos.

Criterio de éxito: misma visual, 3 helmets, validation layers limpias.
```

---

## Fase 10 — Skybox + IBL

**Qué cambia:** Cubemap HDR como fondo + Image-Based Lighting para ambient, reemplazando el `ambient = kD * albedo * 0.03` hardcodeado en `triangle.frag`.

**Archivos nuevos:**
- `shaders/skybox.vert` — fullscreen triangle, dirección de vista desde inv(projection) * NDC
- `shaders/skybox.frag` — `texture(samplerCube, direction)`
- `src/engine/skybox.rs` — carga cubemap, precompute irradiance + prefiltered env + BRDF LUT

**Archivos que se modifican:**
- `src/engine/vulkan_init.rs` → campos: skybox_pipeline, ibl_images, skybox descriptor set. Render skybox después de geometry en `record_command_buffer`
- `src/engine/pipeline.rs` → `create_skybox_pipeline()`: depth test LESS_OR_EQUAL, depth write OFF, cull FRONT (renderizamos dentro del cubo)
- `shaders/triangle.frag` → ambient = irradiance sample × albedo × kD + prefiltered × F × BRDF_LUT. Reemplaza el `* 0.03`
- Descriptor set 0 layout → agregar bindings 2 (irradiance), 3 (prefiltered), 4 (BRDF LUT)

**Recursos GPU nuevos:**

| Recurso | Formato | Tamaño | Uso |
|---------|---------|--------|-----|
| Cubemap HDR | R16G16B16A16_SFLOAT | 6 × 512×512 | Skybox visual |
| Irradiance | R16G16B16A16_SFLOAT | 6 × 64×64 | IBL diffuse |
| Prefiltered env | R16G16B16A16_SFLOAT | 6 × 128×128, 5 mips | IBL specular |
| BRDF LUT | R16G16_SFLOAT | 512×512 | Schlick-GGX lookup |

**Render order en `record_command_buffer`:**
```
[SHADOW PASS] → sin cambios
[MAIN PASS]
  ├── bind graphics_pipeline (PBR + IBL)
  ├── for each (Transform, MeshRenderer) → draw geometry
  ├── bind skybox_pipeline
  └── draw(3) → fullscreen triangle, skybox detrás de todo
```

**Criterio de éxito:** Fondo HDR visible. Objetos metálicos reflejan el entorno. Comparar con/sin IBL — la diferencia es dramática en el ambient.

---

## Fase 11 — Debug UI con egui

**Qué cambia:** Overlay interactivo que renderiza como último pass sobre la escena.

**Dependencias nuevas:** `egui`, `egui-winit`, `egui-ash` (o integración manual)

**Archivos nuevos:**
- `src/ui/mod.rs` — `DebugUi` struct, integración egui-winit + render a Vulkan
- `src/ui/panels.rs` — scene inspector, light editor, stats

**Archivos que se modifican:**
- `src/main.rs` → procesar input de egui antes que cámara; si egui.wants_pointer_input(), no pasar a `InputState`
- `src/engine/vulkan_init.rs` → render egui como último paso en `record_command_buffer`
- `src/input/mod.rs` → `set_captured(bool)` para bloquear input de cámara

**Paneles:**

| Panel | Contenido |
|-------|-----------|
| Stats | FPS, frame time ms, draw calls, triángulos, culled (0 hasta fase 13) |
| Scene | Lista de entidades hecs, click → inspector de componentes |
| Transform | Sliders position x/y/z, rotation euler, scale |
| Lights | DirectionalLight: dirección, color, intensidad. PointLight: pos, color, radius |
| Shadow | Slider de bias (hardcodeado 0.002 en `triangle.frag` actualmente) |

**Input routing:**
```
winit event → egui.on_event(event)
  si egui.wants_pointer_input() → no pasar a InputState
  sino → InputState.process_event() → cámara normal
```

**Criterio de éxito:** Panel visible, FPS counter, mover luz con slider y ver cambio en la escena. Click en slider no mueve cámara.

---

## Fase 12 — Hot-reload de shaders

**Qué cambia:** Shaders compilables en runtime además de en build time. File watcher sobre `shaders/`.

**Dependencias nuevas:** `notify = "7"` (o polling cada 500ms)

**Archivos nuevos:**
- `src/asset/shader_compiler.rs` — compilar GLSL→SPIR-V con shaderc en runtime, watcher

**Archivos que se modifican:**
- `src/engine/vulkan_init.rs` → `recreate_pipeline(which, vert_spv, frag_spv)` que hace `vkDeviceWaitIdle` → destroy viejo → create nuevo
- `src/engine/pipeline.rs` → `create_graphics_pipeline` y `create_shadow_pipeline` reciben `&[u8]` en vez de `include_bytes!`
- `src/main.rs` → cada frame: `shader_compiler.check_changes()`, si hay, llamar `vulkan.recreate_pipeline()`

**Mapeo archivo → pipeline:**

| Archivo modificado | Pipeline a recrear |
|----|-----|
| `triangle.vert` o `triangle.frag` | `graphics_pipeline` |
| `shadow.vert` o `shadow.frag` | `shadow_pipeline` |
| `skybox.vert` o `skybox.frag` | `skybox_pipeline` |

**Error handling:** Si la compilación falla, loguear el error en el panel stats de egui, mantener pipeline anterior. El motor nunca crashea por un shader con error de sintaxis.

**Criterio de éxito:** Editar `triangle.frag` (cambiar tone mapping), guardar, ver el cambio en <1s. Error de sintaxis muestra mensaje en egui, motor sigue corriendo.

---

## Fase 13 — Frustum culling + game loop robusto

**Qué cambia:** No dibujar lo que la cámara no ve. Separar update de render.

**Archivos nuevos:**
- `src/scene/culling.rs` — `extract_frustum_planes(view_proj) -> [Vec4; 6]`, `is_aabb_visible(aabb, planes) -> bool`

**Archivos que se modifican:**
- `src/asset/loader.rs` → `MeshData` gana `aabb_min: Vec3, aabb_max: Vec3` calculados de los vértices al cargar
- `src/scene/ecs.rs` → nuevo componente `BoundingBox { min: Vec3, max: Vec3 }` (local space)
- `src/main.rs` → filtrar draw list con frustum test, game loop fixed timestep 60Hz, integrar gilrs
- `src/input/mod.rs` → gamepad vía gilrs: stick izq = move, stick der = camera

**Culling en el loop:**
```rust
let planes = extract_frustum_planes(camera.view_proj());
for (_, (transform, mesh_renderer, bbox)) in world.query::<(&Transform, &MeshRenderer, &BoundingBox)>() {
    let world_aabb = transform_aabb(bbox, transform.to_mat4());
    if is_aabb_visible(&world_aabb, &planes) {
        draw_list.push(...);
    } else { culled += 1; }
}
// egui stats: "Rendered: {}/{}  ({} culled)"
```

**Fixed timestep:**
```rust
const TICK_RATE: f64 = 1.0 / 60.0;
accumulator += dt;
while accumulator >= TICK_RATE {
    update_systems(&mut world, &input, TICK_RATE as f32);
    accumulator -= TICK_RATE;
}
draw_frame(...);
```

**Criterio de éxito:** Stats en egui muestran objetos culleados. Movimiento de cámara idéntico a cualquier framerate. Gamepad funciona.

---

## Fase 14 — MSAA + mipmaps + polish

**Qué cambia:** Anti-aliasing, mipmaps, tone mapping mejorado.

**Archivos que se modifican:**
- `src/engine/vulkan_init.rs` →
  - Crear MSAA color image (queryar `limits.framebufferColorSampleCounts`)
  - MSAA depth image (mismo sample count)
  - Main pass: `RenderingAttachmentInfo` con `.resolve_image_view(swapchain)` y `.resolve_mode(AVERAGE)`
  - Shadow pass sin MSAA
- `src/engine/gpu_resources.rs` → `upload_texture` genera mip chain con `vkCmdBlitImage` level por level
  - `mip_levels = floor(log2(max(w,h))) + 1`
  - Sampler: cambiar `max_lod` de 0 a `mip_levels`
- `shaders/triangle.frag` → agregar ACES filmic como alternativa a Reinhard
- `src/ui/panels.rs` → toggle MSAA off/2x/4x, toggle Reinhard/ACES

**MSAA con dynamic rendering:**
```rust
// color attachment apunta a MSAA image, resolve apunta a swapchain
color_attachment
    .image_view(msaa_color_view)
    .resolve_image_view(swapchain_view)
    .resolve_mode(vk::ResolveModeFlags::AVERAGE)
    .store_op(vk::AttachmentStoreOp::DONT_CARE)  // MSAA image descartable
```

**Criterio de éxito:** Bordes suaves. Texturas no pixelean a distancia. Toggle MSAA en egui. Tone mapping switcheable.

---

## Shadow map fit to camera (bonus)

El ortho box de `compute_light_mvp()` en `src/scene/lights.rs` es fijo ±5 unidades. Para escenas grandes (Sponza), calcular tight-fit:

```
corners_world = inverse(view_proj) * 8 NDC corners
corners_light = light_view * corners_world
ortho = min/max of corners_light.xyz
```

---

## Descriptor sets al cierre del M2

```
Set 0 — Lighting + environment (per frame-in-flight)
  binding 0: UNIFORM_BUFFER           → LightingUbo (144 bytes)
  binding 1: COMBINED_IMAGE_SAMPLER   → shadow map
  binding 2: COMBINED_IMAGE_SAMPLER   → irradiance cubemap
  binding 3: COMBINED_IMAGE_SAMPLER   → prefiltered env cubemap
  binding 4: COMBINED_IMAGE_SAMPLER   → BRDF LUT

Set 1 — Material (sin cambios)
  binding 0-2: albedo, normal, metallic-roughness
```

---

## Estructura final esperada

```
src/
├── main.rs              App, hecs::World, fixed timestep game loop
├── asset/
│   ├── mod.rs
│   ├── loader.rs        SceneData + AABB calculation
│   └── shader_compiler.rs   Runtime GLSL→SPIR-V + file watcher
├── engine/
│   ├── mod.rs
│   ├── vulkan_init.rs   VulkanContext + MSAA + skybox render
│   ├── gpu_resources.rs GpuResourceManager + mipmap generation
│   ├── pipeline.rs      PBR + shadow + skybox pipelines
│   ├── skybox.rs        Cubemap loading, IBL precomputation
│   └── vertex.rs        Sin cambios
├── scene/
│   ├── mod.rs
│   ├── camera.rs        Sin cambios
│   ├── lights.rs        compute_light_mvp() con frustum fit
│   ├── ecs.rs           Componentes + helpers
│   └── culling.rs       Frustum planes + AABB test
├── input/
│   └── mod.rs           InputState + gilrs gamepad
└── ui/
    ├── mod.rs            DebugUi, egui integration
    └── panels.rs         Scene inspector, light editor, stats

shaders/
├── triangle.vert/frag   PBR + IBL + shadow (hot-reloadable)
├── shadow.vert/frag     Sin cambios
├── skybox.vert/frag     Nuevo
```

---

## Tiempos estimados

| Fase | Descripción | Dificultad | Tiempo |
|------|-------------|------------|--------|
| 9 | ECS (hecs) | Media | 1-2 días |
| 10 | Skybox + IBL | Alta | 3-5 días |
| 11 | Debug UI (egui) | Media | 2-3 días |
| 12 | Hot-reload shaders | Media | 1-2 días |
| 13 | Culling + game loop + gamepad | Media | 2-3 días |
| 14 | MSAA + mipmaps + polish | Media | 2-3 días |
| **Total** | | | **~12-18 días** |