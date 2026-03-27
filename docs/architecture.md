# Architecture — ralk (Milestone 7)

Estado al 2026-03-27. Refleja el motor completo tras M7.

---

## Stack técnico

| Capa | Crate/tecnología | Notas |
|------|-----------------|-------|
| Gráficos | `ash 0.38` (Vulkan 1.2+) | Sin wgpu, sin vulkano. MoltenVK en macOS. |
| Ventana | `winit 0.30` | `ApplicationHandler` trait |
| Memoria GPU | `gpu-allocator 0.27` | Wrapeado en `GpuResourceManager` |
| Math | `glam 0.29` | `bytemuck` feature. No nalgebra en hot path. |
| ECS | `hecs` | Sparse set, sin sistemas declarativos |
| Physics | `rapier3d` | Rigid bodies, capsule player, static/dynamic/kinematic |
| Audio | `rodio` | Playback de audio |
| Scripting | `mlua` (Lua 5.4) | Scripts en `assets/scripts/` |
| Assets | `gltf 1.x` | `.glb` binario |
| Shaders | GLSL → SPIR-V via `shaderc` en `build.rs` | `include_bytes!` en runtime |
| Plataforma | Linux nativo + macOS (MoltenVK) | |
| UI | `egui` + `egui-winit` | Sidebar de debug/editor, HUD, menú principal |
| Imágenes | `image` (png feature) | Splatmap PNG I/O |

---

## Estructura de módulos

```
src/
├── main.rs              App (ApplicationHandler), game loop, spawn, sistemas ECS
├── asset/
│   ├── mod.rs           Re-exports
│   ├── loader.rs        SceneData, load_glb(), load_multi_glb(), builtin_cube()
│   ├── scene_file.rs    SceneFile, EntityDef, PlacedProp, todas las *Def structs
│   ├── config.rs        AppConfig, load_config(), save_config()
│   └── prefab.rs        PrefabFile, PrefabEntityDef, build_prefab_from_selection()
├── engine/
│   ├── mod.rs           Re-exports (VulkanContext, DrawInstance)
│   ├── vulkan_init.rs   VulkanContext — Vulkan completo, render graph, draw_frame()
│   ├── gpu_resources.rs GpuResourceManager — buffers, imágenes, meshes
│   ├── pipeline.rs      Todos los pipelines: PBR, shadow, cull, skybox, SSAO, bloom,
│   │                    tonemap, wireframe, particle
│   └── vertex.rs        Vertex, WireframeVertex, ParticleVertex
├── scene/
│   ├── mod.rs           Re-exports
│   ├── camera.rs        Camera3D, view(), view_proj()
│   ├── lights.rs        LightingUbo, LightingState, compute_light_mvp()
│   ├── ecs.rs           Todos los ECS components (Transform, MeshRenderer, Physics*,
│   │                    Parent, Children, WorldTransform, ParticleEmitter, Particle,
│   │                    PropertyAnimator, TriggerZone, TriggerAction, Terrain,
│   │                    MaterialOverride, DirectionalLight, PointLight, StreetLight,
│   │                    AudioSource, Vehicle, BoundingBox, etc.)
│   └── gizmo.rs         Gizmo (translate/rotate/scale), ray-AABB, screen_to_ray
├── physics/
│   └── mod.rs           PhysicsWorld — rapier3d wrapper
│                        add_static_box/dynamic_box/kinematic_box/player_capsule
│                        set_body_pose(), set_kinematic_pose(), step_and_collect_impacts()
├── audio/
│   └── mod.rs           AudioEngine — rodio wrapper
├── scripting/
│   └── mod.rs           ScriptEngine — mlua Lua 5.4
├── input/
│   └── mod.rs           InputState — teclado + mouse delta
└── ui/
    ├── mod.rs           Re-exports de estados UI, AppScreen, GameAction
    └── panels.rs        Todas las funciones de render UI (egui):
                         main_menu, hud, sidebar completo con todos los paneles

shaders/
├── triangle.vert/frag   Main pass PBR, GPU-driven via InstanceBuffer
├── shadow.vert/frag     Shadow pass depth-only
├── cull.comp            GPU frustum culling, escribe DrawIndexedIndirectCommand
├── skybox.vert/frag     Cubemap skybox
├── ssao.frag            Screen-space ambient occlusion
├── ssao_blur.frag       Blur separable para SSAO
├── bloom_down.frag      Downsampling bloom
├── bloom_up.frag        Upsampling bloom
├── tonemap.frag         Tonemapper HDR→LDR (Reinhard o ACES)
├── wireframe.vert/frag  Debug physics colliders
└── particle.vert/frag   Billboard partículas con falloff Gaussiano
```

---

## ECS components clave

```rust
// Transform (posición/rotación/escala local)
struct Transform { position: Vec3, rotation: Quat, scale: Vec3 }

// Hierarchy (Fase 38)
struct Parent   { entity: hecs::Entity }
struct Children { entities: Vec<hecs::Entity> }
struct WorldTransform { matrix: Mat4 }  // world = parent_world * local

// Renderizado
struct MeshRenderer { mesh_index: usize, material_set_index: usize }
struct BoundingBox  { min: Vec3, max: Vec3 }  // mesh-local space

// Physics
struct PhysicsBody     { handle: RigidBodyHandle, body_type: PhysicsBodyType }
struct PhysicsCollider { handle: ColliderHandle, shape: ColliderShapeType, half_extents: Vec3 }

// Luces
struct DirectionalLight { direction: Vec3, color: Vec3, intensity: f32 }
struct PointLight       { color: Vec3, intensity: f32, radius: f32 }

// Partículas (Fase 40)
struct ParticleEmitter { max_particles, spawn_rate, lifetime_min/max, initial_velocity,
                         velocity_randomness, gravity_factor, start/end_size, start/end_color,
                         shape, blend_additive, enabled, particles: Vec<Particle>, spawn_accum }
struct Particle { position, velocity, age, lifetime, start/end_size, start/end_color: Vec3 }

// Animación (Fase 41)
struct PropertyAnimator { from_rot_y, to_rot_y, duration, elapsed, easing, playing, loop_anim, reverse }

// Triggers (Fase 42)
struct TriggerZone { shape, size, on_enter, on_exit, once, triggered, player_inside, visible_in_editor }
enum   TriggerAction { PlayAnimation{..}, PlaySound{..}, ToggleEntity{..}, SpawnPrefab{..} }

// Material Override (Fase 44)
struct MaterialOverride { base_color_factor, metallic_factor, roughness_factor,
                          emissive_factor, emissive_intensity, normal_scale, uv_scale }  // todos Option<T>

// Terrain (Fase 43)
struct Terrain { resolution, splatmap: Vec<u8>, dirty, layers: Vec<TerrainLayer>, ... }

// Audio
struct AudioSource { sound_path, volume, looping, max_distance, handle }

// Prefabs (Fase 39)
struct PrefabInstance { prefab_path: String }
```

---

## GPU structs

### InstanceData (SSBO, 160 bytes)

```rust
// binding 5 en set 0 — leído por cull.comp y triangle.frag
#[repr(C)] struct InstanceData {
    model:            [f32; 16],   // offset   0: model matrix
    aabb_min:         [f32; 3],    // offset  64: world AABB min
    mesh_index:       u32,         // offset  76
    aabb_max:         [f32; 3],    // offset  80: world AABB max
    material_index:   u32,         // offset  92
    override_color:   [f32; 4],    // offset  96: base color override
    override_mr:      [f32; 2],    // offset 112: metallic, roughness
    _pad2:            [f32; 2],    // offset 120
    override_emissive:[f32; 4],    // offset 128: emissive RGB + intensity
    override_flags:   u32,         // offset 144: bitmask (1=color,2=metal,4=rough,8=emissive)
    _pad3:            [f32; 3],    // offset 148
}                                  // total: 160 bytes
```

### DrawInstance (CPU side, input a build_draw_list)

```rust
struct DrawInstance {
    model:            Mat4,
    mesh_index:       usize,
    material_index:   usize,
    world_min/max:    Vec3,
    override_flags:   u32,
    override_color:   [f32; 4],
    override_mr:      [f32; 2],
    override_emissive:[f32; 4],
}
// DrawInstance::basic(model, mesh, mat, min, max) — sin overrides
```

### ParticleVertex (36 bytes)

```rust
#[repr(C)] struct ParticleVertex {
    position:  [f32; 3],  // location 0, offset  0  — world-space corner
    color:     [f32; 4],  // location 1, offset 12  — RGBA interpolado
    tex_coord: [f32; 2],  // location 2, offset 28  — UV para el círculo
}
```

### Vertex principal (48 bytes)

```rust
#[repr(C)] struct Vertex {
    position:  [f32; 3],  // location 0, offset  0
    normal:    [f32; 3],  // location 1, offset 12
    tex_coord: [f32; 2],  // location 2, offset 24
    tangent:   [f32; 4],  // location 3, offset 32  (xyz + handedness w)
}
```

---

## Render loop (render graph)

El render graph maneja todas las barreras automáticamente. Cada pass declara sus accesos a recursos y el graph inserta las transiciones necesarias.

```
draw_frame()
  ├── upload particle vertices (256 KiB CpuToGpu, capped)
  ├── upload InstanceData SSBO
  ├── write LightingUbo
  └── record_command_buffer(cmd, frame_idx, view_proj, ...)
        │
        ├── [SHADOW PASS]             depth-only, 2048×2048, ortho light frustum
        │
        ├── [CULL PASS]               compute shader, frustum culling, escribe DrawIndirect
        │
        ├── [MAIN PASS]               PBR + IBL skybox, MSAA resolve si activo
        │   ├── main geometry
        │   └── skybox cubemap
        │
        ├── [PARTICLE PASS]           si hay partículas
        │   ├── hdr_color: SHADER_READ_ONLY → COLOR_ATTACHMENT
        │   ├── additive blend, depth test on, depth write off
        │   └── hdr_color: → SHADER_READ_ONLY
        │
        ├── [SSAO PASS]               si ssao_enabled
        │   ├── ssao_raw
        │   └── ssao_blur (bilateral)
        │
        ├── [BLOOM PASSES]            si bloom_enabled
        │   ├── downsample chain
        │   └── upsample chain
        │
        └── [TONEMAP PASS]            HDR → LDR, Reinhard o ACES, con SSAO + bloom
              └── → swapchain PRESENT_SRC_KHR
```

---

## Descriptor sets

```
Set 0 — Frame global (1 set/frame-in-flight)
  binding 0: UNIFORM_BUFFER        → LightingUbo
  binding 1: COMBINED_IMAGE_SAMPLER → shadow map
  binding 2: COMBINED_IMAGE_SAMPLER → skybox irradiance (IBL diffuse)
  binding 3: COMBINED_IMAGE_SAMPLER → skybox prefiltered (IBL specular)
  binding 4: COMBINED_IMAGE_SAMPLER → BRDF LUT
  binding 5: STORAGE_BUFFER        → InstanceData[]  (leído en vert+frag+comp)

Set 1 — Material (1 set por material glTF + 1 default)
  binding 0: COMBINED_IMAGE_SAMPLER → albedo     (R8G8B8A8_SRGB)
  binding 1: COMBINED_IMAGE_SAMPLER → normal     (R8G8B8A8_UNORM)
  binding 2: COMBINED_IMAGE_SAMPLER → metallic-roughness (R8G8B8A8_UNORM)
```

---

## Sistemas ECS en main.rs (por frame)

```
1. update_world_transforms()       — hierarchy: local → world transforms
2. update_property_animators(dt)   — animar rotation.y + sync physics body
3. update_trigger_zones()          — AABB overlap con player, dispatch actions
4. update_particles(dt)            — spawn, move, age, kill
5. build_draw_list()               — iterar MeshRenderer → DrawInstance[]
6. build_particle_vertices()       — billboard quads → ParticleVertex[]
7. [fixed timestep @ 60Hz]
   └── sync kinematic → rapier
       → physics.step()
       → sync dynamic rapier → ECS
       → sync player pos → camera
8. draw_frame(window, dl, &instances, &particle_vertices, ...)
```

---

## Persistencia de escenas

```
scenes/
├── .last_session.json    Auto-guardado al salir, usado por "Continuar"
├── {nombre}.json         Escenas guardadas explícitamente por el usuario
└── {nombre}_splatmap.png Splatmap del terrain si existe

scene.json                Escena "principal" (legacy, cargada en startup)
assets/prefabs/{nombre}.json  Prefabs guardados
assets/props/default_props.json  Catálogo de props
```

Al salir (Quit in-game, Cmd+Q, cierre de ventana): `save_session()` guarda a `.last_session.json` Y al archivo nombrado si había una escena cargada (`current_scene_name`).

`EntityDef` serializa: transform, physics, collider, audio, PropertyAnimator, TriggerZone, MaterialOverride, parent_index, prefab_path.

---

## Inicialización de VulkanContext (orden)

1. Entry → Instance (Vulkan 1.2, validation en debug)
2. Debug messenger
3. Surface
4. Physical device + logical device + graphics queue
5. Swapchain (FIFO preferido)
6. GpuResourceManager (gpu-allocator)
7. Sampler compartido, default textures 1×1
8. Scene textures (staging upload)
9. Depth buffer (D32 o D24_S8)
10. Shadow map 2048×2048
11. SSAO noise texture + kernel UBO
12. Skybox cubemap (HDR → irradiance + prefiltered)
13. BRDF LUT
14. HDR color buffer (R16G16B16A16_SFLOAT) + MSAA color si activo
15. Bloom images chain
16. SSAO raw + blur buffers
17. Descriptor layouts (frame global + material)
18. Todos los pipelines (PBR, shadow, cull, skybox, SSAO, bloom, tonemap, wireframe, particle)
19. UBO buffers, draw indirect buffers, instance SSBO
20. Particle vertex buffers (256 KiB × frames_in_flight)
21. Descriptor pools + sets (lighting, material)
22. Command pool + buffers
23. Sync objects (semáforos + fences)

---

## GpuResourceManager

```rust
fn create_buffer(size, usage, location) -> BufferHandle
fn create_attachment_image(w, h, format, usage, aspect, layout, ...) -> ImageHandle
fn upload_texture(pixels, w, h, format) -> ImageHandle   // staging → device local
fn upload_mesh(vertices, indices) -> GpuMesh
fn write_buffer(handle, data)
fn get_buffer(handle) -> vk::Buffer          // pub(super)
fn get_image_view(handle) -> vk::ImageView   // pub(super)
fn get_image_raw(handle) -> vk::Image        // pub(super), para barriers
fn destroy_buffer / destroy_image / destroy_mesh / destroy_all
```

---

## Notas MoltenVK (macOS)

- Sin geometry shaders, sin mesh shaders, sin ray tracing
- Particle pipeline: TYPE_1 MSAA (las partículas se renderizan post-resolve en el HDR buffer)
- `mutableComparisonSamplers = false` → shadow sampler manual (`texture().r` + comparación manual en shader)
- Dynamic rendering via `VK_KHR_dynamic_rendering` (extensión, no core 1.3)
- Vulkan 1.2 máximo
