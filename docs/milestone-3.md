# Milestone 3: "El mundo vive" (pendiente)

**Objetivo:** El mundo tiene física, sonido, se puede guardar/cargar, y la arquitectura de rendering escala. Múltiples modelos glTF distintos en una escena persistente, objetos que caen y colisionan, audio espacial, render graph que abstrae los passes, y post-processing (bloom).

**Entregable:** Abrir el motor → cargar una escena desde JSON con 5+ modelos glTF distintos posicionados → objetos con rigid bodies caen por gravedad y colisionan → audio espacial (pasos, impactos) → bloom en luces brillantes → guardar la escena → cerrar → reabrir → la escena se restaura exactamente. Gizmos de transform para mover objetos con el mouse. Validation layers limpias. 60+ FPS.

---

### Fase 15 — Multi-modelo + escena serializada (semana 1-2)

Criterio de éxito: cargar 5+ modelos glTF distintos desde un archivo `scene.json`, guardar cambios hechos con egui, reabrir y la escena se restaura.

- [x] Agregar `serde = { version = "1", features = ["derive"] }` y `serde_json = "1"` a Cargo.toml
- [x] `load_multi_glb(paths)` en `loader.rs` carga N glTF y mergea en SceneData global con offsets de índices
- [x] `VulkanContext` almacena meshes y material descriptor sets de todos los modelos cargados, indexados globalmente
- [x] `MeshRenderer.mesh_index` y `material_set_index` indexan en los arrays globales (no por modelo)
- [x] Definir formato `scene.json`: `models[]`, `entities[]` (mesh_index/material_set_index globales), `directional_light`, `point_lights[]`
- [x] `fn load_scene_file(path)` y `fn save_scene_file(path, scene)` con serde en `src/asset/scene_file.rs`
- [x] Botón "Save Scene" y "Load Scene" en egui (panel "Scene")
- [x] Al cargar: `VulkanContext::reload_scene()` — device_wait_idle → destroy old meshes/textures/pool → upload new → respawnear ECS
- [x] Al guardar: query `(Transform, MeshRenderer)` + lights del World → escribir JSON
- [x] Modelo default si `scene.json` no existe: spawnear el DamagedHelmet como antes

---

### Fase 16 — Physics con rapier (semana 2-3)

Criterio de éxito: spawnear un cubo en el aire, verlo caer por gravedad, colisionar con un piso, y rebotar. Debug wireframes de colliders visibles con toggle en egui.

- [x] Agregar `rapier3d = { version = "0.22", features = ["simd-stable"] }` a Cargo.toml
- [x] Crear `src/physics/mod.rs` — `PhysicsWorld` wrapper: `RigidBodySet`, `ColliderSet`, `IntegrationParameters`, `IslandManager`, `DefaultBroadPhase`, `NarrowPhase`, `ImpulseJointSet`, `MultibodyJointSet`, `CCDSolver`
- [x] `PhysicsWorld::step(dt)` — un step con el timestep fijo (ya tenemos accumulator de fase 13)
- [x] Nuevos componentes en `ecs.rs`:
  - `PhysicsBody { handle: RigidBodyHandle, body_type: PhysicsBodyType }` (Dynamic, Static, Kinematic)
  - `PhysicsCollider { handle: ColliderHandle, shape: ColliderShapeType, half_extents: Vec3 }` (Box)
- [x] `PhysicsWorld::add_dynamic_box` / `add_static_box` helpers
- [x] Sync ECS ↔ Rapier cada frame:
  - Antes de step: query `(Transform, PhysicsBody)` con `body_type == Kinematic` → actualizar rapier body positions desde Transform
  - Después de step: query `(Transform, PhysicsBody)` con `body_type == Dynamic` → actualizar Transform desde rapier body positions
- [x] Spawnear piso como `Static` con collider `Box` grande
- [x] Spawnear cubos como `Dynamic` con collider `Box`, gravedad default (0, -9.81, 0)
- [x] Botón en egui: "Spawn Physics Cube" → crea entidad con Transform + MeshRenderer + PhysicsBody + PhysicsCollider en posición de la cámara
- [x] Debug wireframes: line list de los colliders AABB/shapes, toggle en egui
  - Pipeline separado: topology LINE_LIST, depth test off, color uniforme
  - Iterar entidades `(Transform, PhysicsCollider)` → generar 12 aristas de cada box
- [x] Scene serialization: `RigidBody` y `Collider` se incluyen en `scene.json` (body_type, shape, restitution, friction)
- [x] Physics step se ejecuta dentro del fixed timestep loop (junto con camera update)

---

### Fase 17 — Audio con rodio (semana 3-4)

Criterio de éxito: caminar por la escena y escuchar un sonido ambiente (loop), spawnear un cubo de physics y escuchar el impacto contra el piso. Volumen del impacto depende de distancia a la cámara.

- [x] Agregar `rodio = "0.19"` a Cargo.toml
- [x] Crear `src/audio/mod.rs` — `AudioEngine` struct: `OutputStream`, `OutputStreamHandle`, pool de `Sink`
- [x] `AudioEngine::play_sound(path, volume, looping) -> SoundHandle`
- [x] `AudioEngine::play_spatial(path, position, listener_pos, max_distance)` — atenuación por distancia: `volume = (1 - distance/max_distance).clamp(0, 1)`, fire-and-forget
- [x] `AudioEngine::stop(handle)`, `AudioEngine::set_volume(handle, volume)`
- [x] Nuevo componente en `ecs.rs`: `AudioSource { sound_path: String, volume: f32, looping: bool, max_distance: f32, handle: Option<SoundHandle> }`
- [x] Audio system cada frame: query `(Transform, AudioSource)`, iniciar sonido en primer frame, actualizar volumen por distancia a cámara
- [x] Sonido de impacto: `ContactCollector` (rapier `EventHandler`) en `step_and_collect_impacts()` → posiciones de contacto drenadas al final del fixed-step loop → `play_spatial()` por cada impacto
- [x] Sonido ambiente: entidad con `AudioSource { looping: true }` spawneada en `resumed()` en Vec3::ZERO
- [x] Panel en egui: master volume slider, mute toggle
- [x] Assets de audio: `ensure_sample_sounds()` genera `assets/sounds/ambient.wav` y `assets/sounds/impact.wav` en WAV 16-bit PCM al arrancar si no existen
- [x] Scene serialization: `AudioSource` se incluye en `scene.json` vía `AudioSourceDef`

---

### Fase 18 — Render graph (semana 4-5)

Criterio de éxito: los passes actuales (shadow, main, skybox, egui, debug wireframe) están declarados como nodos de un render graph. Agregar un pass nuevo es declarar inputs/outputs, no copiar barriers manualmente.

- [x] Crear `src/engine/render_graph.rs`
- [x] `ResourceAccess` struct: required_layout, final_layout, enter/exit src+dst stage+access masks
  - Presets: `color_init`, `depth_init`, `shadow_write`, `shader_read`, `color_attachment`, `present`
- [x] `RenderGraph` struct: images[] (TrackedImage with layout), passes[] (PassNode), cursor
- [x] `RenderGraph::add_resource(image, aspect, initial_layout)` → `ResourceId`
- [x] `RenderGraph::add_pass(name, accesses)` — registra un pass con sus resource accesses
- [x] `RenderGraph::compile()` — valida que resources requeridos en SHADER_READ_ONLY tienen un productor o initial_layout válido
- [x] `RenderGraph::begin_pass(device, cmd)` — emite barriers de entrada (current → required_layout)
- [x] `RenderGraph::end_pass(device, cmd)` — emite barriers de salida (required → final_layout), avanza cursor
- [x] Migrar passes existentes al graph (declarados en record_command_buffer cada frame):
  - `FrameInit` pseudo-pass: swapchain + MSAA color UNDEFINED → COLOR_ATTACHMENT, MSAA depth UNDEFINED → DEPTH_STENCIL
  - `Shadow`: shadow_map SHADER_READ_ONLY → DEPTH_STENCIL (enter) → SHADER_READ_ONLY (exit auto)
  - `Main+Skybox`: swapchain/MSAA color COLOR_ATTACHMENT (no barrier), shadow_map SHADER_READ_ONLY (no barrier)
  - `Wireframe` (conditional): swapchain COLOR_ATTACHMENT (no barrier)
  - `Egui` (conditional): swapchain COLOR_ATTACHMENT (no barrier)
  - `Present` pseudo-pass: swapchain COLOR_ATTACHMENT → PRESENT_SRC
- [x] `record_command_buffer` simplificado: 5 `cmd_pipeline_barrier` calls eliminados, reemplazados por `begin_pass`/`end_pass`
- [x] Validación: el graph detecta si un pass requiere SHADER_READ_ONLY en un resource con layout UNDEFINED → error en compile

---

### Fase 19 — Post-processing: bloom (semana 5)

Criterio de éxito: luces brillantes (emisivos, IBL specular intenso) producen glow visible. Toggle bloom on/off en egui. Implementado como pass del render graph.

- [ ] Crear `shaders/bloom_downsample.frag` — threshold + downsample (13-tap filter o bilinear)
- [ ] Crear `shaders/bloom_upsample.frag` — upsample + blend con nivel anterior
- [ ] Crear `shaders/composite.frag` — mezclar scene color + bloom result antes de tone mapping
- [ ] Crear `shaders/fullscreen.vert` — fullscreen triangle reutilizable (compartir con skybox si es el mismo)
- [ ] Bloom chain: 5-6 mip levels de downsample, luego upsample acumulativo
  - Render target intermedio: `R16G16B16A16_SFLOAT`, half-res → quarter-res → ...
  - Cada level es un pass en el render graph
- [ ] El main pass ahora renderiza a un render target intermedio (no directo a swapchain)
- [ ] Composite pass: scene + bloom → swapchain (acá va tone mapping)
- [ ] Mover tone mapping (Reinhard/ACES) de `triangle.frag` a `composite.frag`
- [ ] `triangle.frag` trabaja en HDR linear (sin tone mapping ni gamma)
- [ ] Toggle en egui: bloom on/off, bloom intensity slider, bloom threshold slider
- [ ] Agregar passes al render graph:
  - `MainPass` → output = hdr_color (SFLOAT, no swapchain)
  - `BloomDownsamplePass[0..5]` → chain de downsamples
  - `BloomUpsamplePass[0..5]` → chain de upsamples
  - `CompositePass` → input = hdr_color + bloom_result, output = swapchain

---

### Fase 20 — Editor: gizmos + object picking (semana 5-6)

Criterio de éxito: click en un objeto en la escena lo selecciona (highlight), aparece gizmo de translate/rotate/scale para moverlo con el mouse. Cambios se reflejan en el ECS y se pueden guardar.

- [x] Object picking vía raycast:
  - Click del mouse → unproject a ray en world space desde la cámara
  - Testear ray contra AABB de cada entidad con `(Transform, BoundingBox)`
  - Seleccionar la entidad más cercana al hit
  - Alternativa: rapier `cast_ray()` si tiene colliders
- [x] Highlight de selección: renderizar el mesh seleccionado con un outline (stencil pass o wireframe overlay con color)
- [x] Gizmo de translate:
  - 3 flechas (X rojo, Y verde, Z azul) renderizadas como meshes simples en la posición del objeto seleccionado
  - Click+drag en una flecha → mover el objeto en ese eje
  - Calcular movimiento: project mouse delta al eje del gizmo en world space
- [x] Gizmo de rotate: 3 arcos/círculos por eje, click+drag rota
- [x] Gizmo de scale: 3 cubitos en los extremos de las flechas, click+drag escala en ese eje
- [x] Toggle en egui: modo Translate / Rotate / Scale (o teclas W/E/R)
- [x] Gizmos se renderizan con depth test off (siempre visibles) en un pass del render graph
- [x] La entidad seleccionada se muestra en el panel scene de egui con sus componentes editables
- [x] Mover un objeto con el gizmo actualiza el `Transform` en el ECS → se refleja en physics (si tiene `RigidBody` Kinematic)

---

### Entregable final del Milestone 3

Motor con:
- Escena persistente: guardar/cargar JSON con múltiples modelos glTF
- Physics: rigid bodies dinámicos/estáticos, colliders, gravedad, debug wireframes
- Audio espacial: sonidos de impacto por distancia, ambiente en loop
- Render graph: passes declarativos con barriers automáticos
- Post-processing: bloom HDR via render graph
- Editor: object picking + gizmos de transform + outline de selección

**Esto prueba:** el motor soporta contenido interactivo. Se puede construir una escena con múltiples objetos, física, y sonido, guardarla, y volver a ella. El render graph escala a N passes sin copiar barriers. Los gizmos hacen que iterar en la escena sea productivo.

---

## Después del Milestone 3 (no planificar todavía)

- **M4 "Production ready"** — GPU-driven rendering (indirect draws, compute culling), LOD system, streaming de assets, profiling integrado, scripting (Lua o WASM)
- **M5 "Advanced rendering"** — Ray tracing opcional, mesh shaders, virtual geometry, SSAO, SSR, volumetric fog, global illumination

---

## Principios del roadmap

1. **Cada fase tiene un test visual.** Si no lo ves en pantalla, no está terminado.
2. **No adelantar fases.** La fase 19 asume que la 18 funciona.
3. **Commit por fase.**
4. **"Funciona feo" es mejor que "no funciona bonito."**
5. **El render graph es la fase más importante del M3.** Una vez que funciona, agregar passes (bloom, SSAO, SSR) se vuelve trivial.