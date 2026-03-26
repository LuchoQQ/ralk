---
name: M3 Fases 15-16 completadas
description: Global mesh/material indexing, scene.json save/load, reload_scene GPU, rapier3d physics, wireframe debug pass
type: project
---

Fase 15 completada. Arquitectura implementada:

**Global indexing**: `load_multi_glb(paths)` en `src/asset/loader.rs` mergea N glTF files en un único `SceneData` aplicando offsets de índices. `MeshRenderer.mesh_index` y `material_set_index` apuntan a los arrays globales `VulkanContext::gpu_meshes` y `material_sets`.

**scene.json**: Formato en `src/asset/scene_file.rs` (serde). Entidades guardan `mesh_index`/`material_set_index` globales + opcional `rigid_body`/`collider` para física. `load_scene_file` / `save_scene_file` en el mismo archivo.

**GPU reload**: `VulkanContext::reload_scene(&mut self, scene_data)` en `vulkan_init.rs`:
1. `device_wait_idle()`
2. Destruye gpu_meshes, scene_textures, material_descriptor_pool
3. Sube nuevas texturas
4. Crea nuevo pool + descriptor sets usando `write_material_set()` (free fn compartida con init)
5. Sube nuevos meshes

**Flujo de load en main loop**: reload es POSTERIOR a `draw_frame()` en el mismo frame para no invalidar el draw_list ya construido.

**UI**: `SceneUiState` + `scene_panel` en `src/ui/`. Botones "Save Scene"/"Load Scene".

---

Fase 16 completada. Arquitectura implementada:

**PhysicsWorld**: `src/physics/mod.rs`. Wrapper de rapier3d con todos los subsistemas. Métodos: `add_dynamic_box`, `add_static_box`, `step(dt)`, `set_kinematic_pose`, `get_dynamic_pose`.

**ECS physics components** en `src/scene/ecs.rs`:
- `PhysicsBody { handle, body_type }` — enlaza entidad a rapier RigidBody
- `PhysicsCollider { handle, shape, half_extents }` — enlaza entidad a rapier Collider + guarda half_extents para wireframe
- `PhysicsBodyType`: Dynamic, Static, Kinematic
- `ColliderShapeType`: Box

**Sync ECS ↔ Rapier en fixed-step loop** (60 Hz):
1. Kinematic: `set_kinematic_pose(body.handle, transform.position, transform.rotation)` → rapier
2. `physics.step(TICK_RATE)`
3. Dynamic: `get_dynamic_pose(body.handle)` → transform.position/rotation

**Piso estático**: spawneado en `App::new()` como static box en y=-1.0, half_extents=(20, 0.5, 20).

**Spawn cube**: `spawn_physics_cube()` crea cubo en posición de cámara + 3m al frente. Usa `cube_mesh_index` (último mesh en scene_data, el cube builtin).

**Wireframe debug pass**: `src/engine/pipeline.rs::create_wireframe_pipeline`:
- Pipeline separado: LINE_LIST, depth_test=false, depth_write=false, 1 sample
- Push constant: view_proj (mat4, 64B) + color (vec4, 16B) = 80 bytes, VERTEX stage
- No descriptor sets
- Target: swapchain con LOAD_OP_LOAD (igual que egui), sin depth attachment
- WireframeVertex: 12 bytes (vec3 position)
- Per-frame vertex buffers CpuToGpu de 64KiB en VulkanContext

**Wireframe generation**: `build_wireframe_lines()` itera entidades con `(Transform, PhysicsCollider)`, genera 12 aristas × 2 vértices = 24 vértices por box collider.

**Why:** Física para que objetos caigan y colisionen. Wireframes para debug visual de colliders. Arquitectura: physics world es independiente del render, sync explícito antes/después del step.

**How to apply:** Al agregar nuevos modelos en fases futuras, los índices del ECS deben ser globales. Los physics bodies se crean vía `physics.add_*` y se linkan al ECS con PhysicsBody/PhysicsCollider. Siempre resetear `self.physics = PhysicsWorld::new()` al hacer load_scene.
