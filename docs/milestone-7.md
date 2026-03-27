# Milestone 7: "El mundo vivo" — COMPLETO (2026-03-27)

**Objetivo:** Convertir el sandbox en un motor usable para armar escenarios reales.

**Estado al entrar:** Placement de props desde JSON, gizmos, persistencia, player con salto, menú principal, config screen, day/night. Todo flat en el ECS sin jerarquía. Sin partículas, sin animaciones, sin triggers, sin terrain painting, sin editor de materiales.

---

## Fases completadas

### Fase 38 — Jerarquía padre-hijo ✓

Componentes ECS en `src/scene/ecs.rs`:
- `Parent { entity: hecs::Entity }` — referencia al padre
- `Children { entities: Vec<hecs::Entity> }` — lista de hijos
- `WorldTransform { matrix: Mat4 }` — transform world calculado

Sistema `update_world_transforms()` en `main.rs`: dos queries separadas para evitar borrow conflicts (primero collect local mats, luego collect parent map como HashMap, luego procesar). Solo un nivel de profundidad — recursión completa omitida por ahora.

`build_draw_list()` usa `WorldTransform` cuando está presente en vez del `Transform` local.

Scene Tree en sidebar muestra entidades con indent para hijos. Labels descriptivos: `[42] Mesh (2.0, 0.5, 3.0)`.

Serialización: `parent_index` en `EntityDef`.

---

### Fase 39 — Prefabs ✓

`PrefabFile` / `PrefabEntityDef` en `src/asset/prefab.rs` — JSON en `assets/prefabs/`.

`build_prefab_from_selection()`: calcula offsets relativos al centroide del grupo.

`PrefabInstance { prefab_path: String }` — componente ECS que trackea el origen.

`parent_index` añadido a `EntityDef` y `PlacedProp`.

---

### Fase 40 — Sistema de partículas ✓

CPU emitter → `ParticleEmitter` ECS component en `src/scene/ecs.rs`:
- `fire_preset()` — naranja subiendo con anti-gravedad leve
- `smoke_preset()` — gris subiendo despacio

`update_particles(dt)` en `main.rs`: LCG random, spawn acumulativo, gravedad, `retain_mut`.

`build_particle_vertices()`: usa `camera.view()` para extraer right/up. 6 verts/partícula (dos triángulos billboard).

**Pipeline de partículas** (`pipeline.rs`):
- `create_particle_pipeline()`: push constant 64 bytes (view_proj), additive blend (SRC=ONE, DST=ONE, BlendOp::ADD), depth test ON, depth write OFF, TYPE_1 MSAA
- **CRÍTICO:** El pipeline usa `HDR_FORMAT` (`R16G16B16A16_SFLOAT`), NO `swapchain_format` (`B8G8R8A8_SRGB`). Si se usa el formato del swapchain, R y B se intercambian → fuego naranja aparece azul.
- Sin descriptor sets — solo push constants

`particle_vertex_buffers`: 256 KiB por frame-in-flight (CpuToGpu).

Particle pass en render graph: `bloom_overwrite()` resource access (`SHADER_READ_ONLY → COLOR_ATTACHMENT → SHADER_READ_ONLY`). Se renderiza DESPUÉS de main+skybox, ANTES de SSAO.

`has_depth_for_particles = msaa_depth.is_none() && !ssao_enabled` — el depth solo se adjunta cuando está disponible como attachment (no cuando SSAO lo tiene en SHADER_READ_ONLY).

Shaders: `shaders/particle.vert` + `shaders/particle.frag` (falloff Gaussiano circular, discard fuera del círculo).

---

### Fase 41 — Animación ✓

`PropertyAnimator` ECS component:
- `from_rot_y`, `to_rot_y`, `duration`, `elapsed`, `easing`, `playing`, `loop_anim`, `reverse`
- `door_open()` preset: 0° → 90°, 0.5s, EaseInOut
- `EasingType`: Linear, EaseInOut (`t * t * (3 - 2t)`)

`update_property_animators(dt)`:
1. Collect entidades animando (query separado)
2. Por cada entidad: avanzar elapsed, calcular rot_y, aplicar a `Transform`
3. **Sync physics**: si la entidad tiene `PhysicsBody`, llamar `physics.set_body_pose()` para que el collider siga la rotación visual

`AnimationPlayer` / `AnimationClip` structs presentes para skeletal animation — rendering no implementado (stubs).

Editor en sidebar: play/pause checkbox, from/to/duration sliders, easing selector.

---

### Fase 42 — Trigger zones ✓

`TriggerZone` ECS component (`TriggerShape::Box` o `Sphere`, `size`, `on_enter`, `on_exit`, `once`, `triggered`, `player_inside`).

`TriggerAction` enum: `PlayAnimation`, `PlaySound`, `ToggleEntity`, `SpawnPrefab`.

`update_trigger_zones()`: collect trigger data (evitar borrow), AABB overlap con player, dispatch en enter/exit. `dispatch_trigger_action()` ejecuta la acción.

Detección via AABB overlap manual (no rapier sensors): `d.x.abs() <= size.x && ...`

Editor en sidebar: size drag values, once/visible checkboxes, status player_inside/triggered, botón Reset.

---

### Fase 43 — Terrain painting ✓

`Terrain` / `TerrainLayer` ECS components. Splatmap RGBA8 con 4 layers (Pasto/Tierra/Roca/Arena).

`Terrain::paint()` pinta en CPU splatmap. `save_png()` / `load_png()` con crate `image` (feature `png`).

`build_blended_rgba()` blendea los 4 canales para upload GPU.

*Nota: el paint con mouse drag en el viewport no está implementado en el sidebar — el splatmap existe en ECS pero no hay herramienta de brush interactivo.*

---

### Fase 44 — Editor de materiales ✓

`MaterialOverride` ECS component:
- `base_color_factor`, `metallic_factor`, `roughness_factor`, `emissive_factor`, `emissive_intensity`, `normal_scale`, `uv_scale` — todos `Option<T>`
- `flags()` → bitmask u32 para GPU (bit 0=color, 1=metallic, 2=roughness, 3=emissive)

`InstanceData` extendido a 160 bytes (era 112):
- `override_color: [f32; 4]`
- `override_mr: [f32; 2]`
- `_pad2: [f32; 2]`
- `override_emissive: [f32; 4]`
- `override_flags: u32`

`DrawInstance::basic()` constructor para los campos sin override.

Fragment shader lee overrides via `flat out uint instanceIndex` (varying desde vertex). Usa `InstanceBuffer` (set 0, binding 5) con `instanceIndex` para lookup.

Los tres shaders que usan InstanceData fueron actualizados: `triangle.vert`, `triangle.frag`, `cull.comp`.

---

## UX del editor (post-M7)

**Selección de entidades:**
- Click en el mundo 3D (ray-AABB)
- O click en Scene Tree (sidebar, Escape para abrir)

**Scene Tree:** Labels descriptivos `[ID] Tipo (x, y, z)` donde Tipo es DirLight/PointLight/Particles/Mesh/Entity.

**Paneles contextuales** (solo si la entidad tiene el componente, O con botón para añadirlo):
- **Material Override** — aparece para cualquier Mesh. Botón `+ Añadir Material Override` si no tiene uno.
- **Particle Emitter** — siempre visible. Botones `+ Fuego` / `+ Humo` para añadir al seleccionado.
- **Animator** — siempre visible. `+ Añadir Animator (puerta)` para añadir PropertyAnimator.
- **Trigger Zone** — siempre visible. `+ Añadir Trigger Zone (caja)`.

**Serialización de componentes M7:** `save_session()` serializa PropertyAnimator, MaterialOverride y TriggerZone. `spawn_from_scene_file()` los restaura. Auto-save al salir actualiza tanto `.last_session.json` como el archivo de escena nombrado.

---

## Bugs corregidos post-M7

**Partículas azules:** El pipeline usaba `swapchain_format` (`B8G8R8A8`) para renderizar en el HDR buffer (`R16G16B16A16`). Los canales R/B se intercambian. Fix: usar `HDR_FORMAT` en `create_particle_pipeline()`.

**Colisiones no siguen la rotación:** `update_property_animators` actualizaba `Transform` pero no el cuerpo de rapier. Fix: `PhysicsWorld::set_body_pose()` que llama `rb.set_position(iso, true)` — funciona para static/dynamic/kinematic.

**Auto-save no actualizaba la escena nombrada:** Al salir solo se actualizaba `.last_session.json`. Fix: `save_session()` también guarda a `scenes/{current_scene_name}.json`. Y `current_scene_name` se setea correctamente al cargar una escena desde el menú.

---

## Entidades demo en sandbox

Spawneadas en `spawn_sandbox_scene()` para poder probar M7 inmediatamente:
- `(0, 0.5, -6)` — cubo con `ParticleEmitter::fire_preset()`
- `(2, 0.5, -6)` — cubo con `ParticleEmitter::smoke_preset()`
- `(-6, 1, 0)` — cubo alargado con `PropertyAnimator` girando 90° en loop
- `(6, 0.5, 0)` — cubo rojo metálico con `MaterialOverride` emisivo
- `(0, 1, 3)` — `TriggerZone` Box cerca del spawn
