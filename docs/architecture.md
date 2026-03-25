# Architecture — ralk (Milestone 1)

Estado al 2026-03-25. Describe el sistema tal como quedó al cierre del Milestone 1.
Usá este documento como contexto para planificar el Milestone 2.

---

## Stack técnico

| Capa | Crate/tecnología | Notas |
|------|-----------------|-------|
| Gráficos | `ash 0.38` (Vulkan 1.2+) | Sin wgpu, sin vulkano. MoltenVK en macOS. |
| Ventana | `winit 0.30` | `ApplicationHandler` trait, event loop sin `run()` legacy |
| Memoria GPU | `gpu-allocator 0.27` | Allocator wrapeado en `GpuResourceManager` |
| Math | `glam 0.29` | `bytemuck` feature habilitada. No nalgebra. |
| Assets | `gltf 1.x` | `.glb` binario + conversión manual a RGBA8 |
| Shaders | GLSL → SPIR-V via `shaderc` en `build.rs` | Compilados en build time, incluidos con `include_bytes!` |
| Plataforma | Linux nativo + macOS (MoltenVK) | Vulkan 1.3 en Linux, 1.2 + KHR extensions en macOS |

---

## Estructura de módulos

```
src/
├── main.rs              App (ApplicationHandler), game loop, asset loading
├── asset/
│   ├── mod.rs           Re-exports públicos
│   └── loader.rs        SceneData, load_glb(), builtin_cube()
├── engine/
│   ├── mod.rs           Re-exports públicos (VulkanContext)
│   ├── vulkan_init.rs   VulkanContext — toda la lógica Vulkan
│   ├── gpu_resources.rs GpuResourceManager — buffers, imágenes, meshes
│   ├── pipeline.rs      create_graphics_pipeline(), create_shadow_pipeline(), layouts
│   └── vertex.rs        Vertex struct (#[repr(C)], Pod, Zeroable)
├── scene/
│   ├── mod.rs           Re-exports
│   ├── camera.rs        Camera3D — view/proj, WASD update
│   └── lights.rs        LightingState, LightingUbo, compute_light_mvp()
└── input/
    └── mod.rs           InputState — teclado + mouse delta

shaders/
├── triangle.vert        Main pass vertex: TBN en world space, fragWorldPos
├── triangle.frag        Main pass fragment: PBR Cook-Torrance + shadow PCF 3×3
├── shadow.vert          Shadow pass vertex: lightMVP push constant
└── shadow.frag          Shadow pass fragment: vacío (depth-only)
```

---

## Tipos de datos clave

### Asset side (CPU)

```rust
// src/asset/loader.rs
struct TextureData { pixels: Vec<u8>, width, height, is_srgb: bool }
struct MaterialData { albedo_tex, normal_tex, metallic_roughness_tex: Option<usize>,
                      base_color_factor: [f32;4], metallic_factor, roughness_factor: f32 }
struct MeshData     { vertices: Vec<Vertex>, indices: Vec<u32>,
                      transform: Mat4, material_index: Option<usize> }
struct SceneData    { meshes: Vec<MeshData>, textures: Vec<TextureData>,
                      materials: Vec<MaterialData> }
```

### Vertex (GPU layout)

```rust
// src/engine/vertex.rs — binding 0, stride 48 bytes
struct Vertex {
    position:  [f32; 3],  // location 0, offset  0
    normal:    [f32; 3],  // location 1, offset 12
    tex_coord: [f32; 2],  // location 2, offset 24
    tangent:   [f32; 4],  // location 3, offset 32  (xyz + handedness w, glTF spec)
}
```

### GPU resources

```rust
// src/engine/gpu_resources.rs
struct BufferHandle(u64);   // opaque, generational
struct ImageHandle(u64);    // opaque, generational
struct GpuMesh { vertex_buffer, index_buffer: BufferHandle, index_count: u32 }
```

### Lighting UBO (std140, 144 bytes)

```rust
// src/scene/lights.rs — set 0, binding 0
struct LightingUbo {
    dir_light_dir:     [f32; 4],   // offset   0: xyz = direction toward scene
    dir_light_color:   [f32; 4],   // offset  16: xyz = color, w = intensity
    point_light_pos:   [f32; 4],   // offset  32: xyz = world pos
    point_light_color: [f32; 4],   // offset  48: xyz = color, w = intensity
    camera_pos:        [f32; 4],   // offset  64: xyz = world pos
    light_mvp:         [f32; 16],  // offset  80: orthographic light view-projection
}                                  // total: 144 bytes
```

### Push constants

| Pipeline | Stage | Size | Contenido |
|----------|-------|------|-----------|
| Main (PBR) | VERTEX | 128 bytes | `[mvp: Mat4, model: Mat4]` |
| Shadow | VERTEX | 64 bytes | `light_view_proj * model: Mat4` |

---

## Descriptor sets

```
Set 0 — Lighting (1 set per frame-in-flight, 2 total)
  binding 0: UNIFORM_BUFFER      → LightingUbo (144 bytes, updated every frame)
  binding 1: COMBINED_IMAGE_SAMPLER → shadow map (comparison sampler, LESS_OR_EQUAL)

Set 1 — Material (1 set per glTF material + 1 default)
  binding 0: COMBINED_IMAGE_SAMPLER → albedo    (R8G8B8A8_SRGB)
  binding 1: COMBINED_IMAGE_SAMPLER → normal    (R8G8B8A8_UNORM)
  binding 2: COMBINED_IMAGE_SAMPLER → metallic-roughness (R8G8B8A8_UNORM, G=rough, B=metal)
```

Pools separados: `descriptor_pool` (lighting) y `material_descriptor_pool` (materials).
El pool de materiales tiene `(N_materials + 1) * 3` descriptores de tipo COMBINED_IMAGE_SAMPLER.

---

## Render loop por frame

```
draw_frame()
  ├── wait_for_fences(current_frame)
  ├── acquire_next_image()
  ├── reset_fences()
  ├── write LightingUbo → ubo_buffers[current_frame]
  ├── compute light_view_proj = compute_light_mvp(&lights.directional)
  └── record_command_buffer(cmd, image_index, view_proj, light_view_proj)
        │
        ├── barrier: swapchain UNDEFINED → COLOR_ATTACHMENT_OPTIMAL
        │
        ├── [SHADOW PASS]
        │   ├── barrier: shadow_map SHADER_READ_ONLY → DEPTH_STENCIL_ATTACHMENT
        │   ├── begin_rendering(depth=shadow_map, CLEAR=1.0, 2048×2048)
        │   ├── bind shadow_pipeline
        │   ├── for each instance:
        │   │     push_constants(light_view_proj * instance.model)
        │   │     draw_indexed()
        │   ├── end_rendering()
        │   └── barrier: shadow_map DEPTH_STENCIL_ATTACHMENT → SHADER_READ_ONLY
        │
        ├── [MAIN PASS]
        │   ├── begin_rendering(color=swapchain, depth=depth_image, CLEAR)
        │   ├── bind graphics_pipeline
        │   ├── bind descriptor_sets[current_frame]  (set 0: lighting + shadow)
        │   ├── for each instance:
        │   │     bind instance.material_set          (set 1: textures)
        │   │     push_constants(mvp, model)
        │   │     draw_indexed()
        │   └── end_rendering()
        │
        └── barrier: swapchain COLOR_ATTACHMENT_OPTIMAL → PRESENT_SRC_KHR
```

---

## Shader: vertex (main pass)

**Input:** `Vertex` (position, normal, tex_coord, tangent)
**Push constants:** MVP (mat4) + model (mat4)

**Output varyings:**
- `fragWorldPos` (vec3): `(model * position).xyz`
- `fragTexCoord` (vec2): passthrough
- `fragT`, `fragB`, `fragN` (vec3): TBN vectors in world space

**TBN construction:**
```glsl
mat3 normalMat = transpose(inverse(mat3(model)));
vec3 N = normalize(normalMat * inNormal);
vec3 T = normalize(normalMat * vec3(inTangent.xyz));
T = normalize(T - dot(T, N) * N);   // Gram-Schmidt re-ortogonalización
vec3 B = cross(N, T) * inTangent.w; // handedness desde glTF vec4 tangent
```

## Shader: fragment (main pass)

**BRDF:** Cook-Torrance metallic-roughness PBR
- `D_GGX`: distribución GGX/Trowbridge-Reitz
- `G_Smith`: geometría Smith con Schlick-GGX
- `F_Schlick`: Fresnel Schlick (F0 = mix(0.04, albedo, metallic))

**Luces:**
1. Directional light: attenuada por shadow factor
2. Point light: atenuación cuadrática `1 / (1 + 0.35d + 0.44d²)`
3. Ambient: `kD * albedo * 0.03` (sin IBL)

**Shadow:**
```glsl
vec4 shadowClip = lights.lightMvp * vec4(fragWorldPos, 1.0);
vec2 shadowUV   = shadowClip.xy / shadowClip.w * 0.5 + 0.5;
float ref       = shadowClip.z / shadowClip.w - 0.002;  // bias
// PCF 3×3 con sampler2DShadow (LESS_OR_EQUAL → 1.0=lit, 0.0=shadow)
```

**Tone mapping:** Reinhard `color / (color + 1.0)`

---

## Inicialización de VulkanContext

Orden de creación (destrucción en orden inverso):
1. `ash::Entry` → `Instance` (Vulkan 1.2, validation en debug)
2. Debug messenger (solo debug builds)
3. Surface (via ash_window)
4. Physical device selection (preferir discreta, Vulkan 1.2+, swapchain, dynamic rendering)
5. Logical device + graphics queue
6. Swapchain (FIFO, triple buffering)
7. Image views del swapchain
8. `GpuResourceManager` (gpu-allocator, transfer command pool TRANSIENT)
9. Sampler compartido (material textures: LINEAR, REPEAT, max_lod=0)
10. Default textures 1×1 (albedo blanca sRGB, normal flat UNORM, MR default UNORM)
11. Scene textures (upload vía staging buffer)
12. **Depth buffer** (D32_SFLOAT o D24_S8, `DEPTH_STENCIL_ATTACHMENT`, initial layout one-shot)
13. **Shadow map** (2048×2048, D32_SFLOAT, `DEPTH_STENCIL_ATTACHMENT | SAMPLED`, initial `SHADER_READ_ONLY`)
14. Shadow comparison sampler (LESS_OR_EQUAL, CLAMP_TO_BORDER, FLOAT_OPAQUE_WHITE)
15. Descriptor set layouts (lighting + material)
16. Graphics pipeline (PBR) + shadow pipeline (depth-only)
17. UBO buffers (CpuToGpu, uno por frame-in-flight)
18. Lighting descriptor pool + sets (binding 0 = UBO, binding 1 = shadow map)
19. Material descriptor pool + sets (por material + 1 default)
20. Mesh upload → `SceneInstance { mesh, model, material_set }`
21. Command pool + command buffers (2 frames in flight, RESET_COMMAND_BUFFER)
22. Sync objects (semaphores por imagen de swapchain, fences por frame-in-flight)

---

## GpuResourceManager

Abstracción sobre gpu-allocator. Nunca expone `vk::Buffer` / `vk::Image` fuera de `src/engine/`.

**API pública:**
```rust
fn create_buffer(size, usage, location) -> BufferHandle
fn create_attachment_image(w, h, format, usage, aspect, initial_layout, ...) -> ImageHandle
fn upload_texture(pixels, w, h, format) -> ImageHandle  // staging → device local
fn upload_mesh(vertices, indices) -> GpuMesh
fn write_buffer(handle, data)
fn get_buffer(handle) -> vk::Buffer          // pub(super)
fn get_image_view(handle) -> vk::ImageView   // pub(super)
fn get_image_raw(handle) -> vk::Image        // pub(super), para barriers
fn destroy_buffer(handle)
fn destroy_image(handle)
fn destroy_mesh(mesh)
fn destroy_all()                             // drops allocator — llamar al final
```

`create_attachment_image` hace la transición de layout en un one-shot command buffer
al momento de creación (no cada frame). Así el primer frame no necesita transición desde UNDEFINED.

---

## Limitaciones conocidas al cierre del M1

Estas son las deudas técnicas que el M2 debería resolver:

| Limitación | Impacto | Solución sugerida |
|-----------|---------|-------------------|
| Sin ECS — `Vec<SceneInstance>` plano | No se puede gestionar entidades ni componentes | Agregar `hecs` |
| Sin frustum culling | Renderiza todo aunque esté fuera de cámara | AABB vs frustum planes |
| Shadow ortho box fijo ±5 unidades | Sombras incorrectas en escenas grandes (Sponza) | Fit to view frustum |
| Sin IBL/environment | Ambient muy flat (0.03) | Cubemap HDR + irradiance/specular maps |
| Sin MSAA | Aliasing visible en bordes | MSAA 4x en pipeline |
| Sampler sin mipmaps | Texturas alias-ean a distancia | Generar mipmaps en upload |
| Render loop bare-metal | Agregar passes es copy-paste de barriers | Render graph básico |
| Shaders compilados en build time | Iterar shaders requiere recompile completo | File watcher + hot-reload |
| Un solo punto de luz hardcodeado | No se puede configurar la escena en runtime | Debug UI (egui) |
| Sin fixed timestep | Physics acoplada a framerate | Separar update/render |

---

## Cómo leer el código

**Flujo de datos asset → GPU:**
```
load_glb("assets/Foo.glb")
  → SceneData { meshes, textures, materials }
     → VulkanContext::new(&window, &scene_data)
        → upload textures → ImageHandle[]
        → build descriptor sets por material → vk::DescriptorSet[]
        → upload meshes → GpuMesh[]
        → SceneInstance { mesh, model, material_set }[]
```

**Flujo por frame:**
```
camera.update(input, dt)
  → view_proj = camera.view_proj()
     → vulkan.draw_frame(window, view_proj, camera.position, &lights)
        → write LightingUbo (incluye light_mvp para shadow)
        → record_command_buffer:
             shadow pass → main pass
```

**Para agregar un nuevo render pass:**
1. Crear shaders en `shaders/` (`.vert` + `.frag`) — `build.rs` los compila automáticamente
2. Crear pipeline en `pipeline.rs` (descriptor layouts + push constants + `PipelineRenderingCreateInfo`)
3. Agregar campos en `VulkanContext` (pipeline, layout, recursos de imagen si hace falta)
4. Agregar barriers + begin_rendering/end_rendering en `record_command_buffer`
5. Limpiar en `destroy()` en orden inverso

**Para agregar un nuevo tipo de descriptor:**
1. Actualizar el `DescriptorSetLayout` correspondiente en `pipeline.rs`
2. Actualizar el pool (añadir `DescriptorPoolSize` del tipo nuevo)
3. Escribir el descriptor con `update_descriptor_sets`
4. Actualizar el GLSL para que declare el binding
