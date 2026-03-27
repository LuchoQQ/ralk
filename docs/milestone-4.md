# Milestone 4: "Production ready" (pendiente)

**Objetivo:** El motor rinde y escala. GPU-driven rendering para escenas grandes, SSAO para calidad visual, profiling para medir, LOD para escalar geometría, asset pipeline async para cargas rápidas, y scripting para que la lógica de juego no requiera recompilar Rust.

**Entregable:** Cargar Sponza (o escena con 100K+ triángulos, 50+ objetos) a 60+ FPS. SSAO visible en esquinas y oclusiones. GPU profiler en egui mostrando tiempo por pass del render graph. LOD automático que reduce triángulos a distancia. Assets cargando async sin bloquear el render. Un script Lua que spawnea objetos y responde a eventos de physics. Validation layers limpias.

---

### Fase 21 — SSAO ✅ COMPLETA

Criterio de éxito: esquinas y cavidades de la escena muestran oscurecimiento sutil. Toggle on/off en egui con diferencia visible. Implementado como pass del render graph.

- [x] Normals reconstruidos desde depth buffer (Opción B)
- [x] Render targets SSAO: R8_UNORM, raw + blurred
- [x] Kernel 16-32 samples hemisférico + noise texture 4×4
- [x] SSAOPass + SSAOBlurPass en render graph
- [x] Composite: ssao_blurred × ambient
- [x] Toggle en egui, sliders radius/bias/power/strength/samples
- [x] SSAO disabled cuando MSAA > 1 (fallback limpio)

---

### Fase 22 — GPU profiling ✅ COMPLETA

Criterio de éxito: egui muestra tiempo en ms de cada pass del render graph, tiempo total GPU del frame, y pipeline statistics.

- [x] `timestampValidBits` query — skip si 0
- [x] `VkQueryPool` TIMESTAMP, 2 queries por pass (begin + end)
- [x] `timestampPeriod` → ns conversion
- [x] Double-buffering de query pools
- [x] `GpuTimingStats`: Vec<(String, f32)>
- [x] `VkQueryPool` PIPELINE_STATISTICS
- [x] Panel egui "GPU Profiler": bar chart horizontal + pipeline stats

---

### Fase 23 — GPU-driven rendering ✅ COMPLETA

Criterio de éxito: toda la geometría se dibuja con un solo `vkCmdDrawIndexedIndirect` (o pocos calls agrupados por pipeline). Compute shader ejecuta frustum culling en GPU.

- [x] Buffer indirect draw commands: `VkDrawIndexedIndirectCommand` por mesh instance
- [x] SSBO instance data: `Mat4 model + u32 material_index` por instancia
- [x] Mega vertex buffer + index buffer global
- [x] `shaders/cull.comp` — frustum culling en GPU
- [x] Render graph: `CullPass` (compute) → `MainPass` (consume indirect buffer)
- [x] `vkCmdDrawIndexedIndirect` por grupo de material
- [x] `LightingUbo` extendido con `view_proj` + `frustum_planes` (304 bytes)
- [x] `shaderDrawParameters` habilitado en DeviceCreateInfo

---

### Fase 24 — LOD system ✅ COMPLETA

Criterio de éxito: objetos lejanos usan meshes con menos triángulos. Toggle + slider en egui. LOD seleccionado en GPU (compute shader). Hard switch.

- [x] `meshopt = "0.3"` en Cargo.toml
- [x] LOD chain: 4 niveles (100%/50%/25%/12.5%) con `meshopt::simplify`
- [x] Todos los LODs en mega index buffer
- [x] `GpuMeshInfo` 48 bytes: `lods[4]` + `vertex_offset` + `lod_count`
- [x] Selección de LOD en compute cull shader
- [x] Push constant 8 bytes: `{instance_count, lod_distance_step}`
- [x] Shadow pass usa LOD 0 siempre
- [x] Slider en egui Settings: "Distance step (m)" 0–50, default 10

---

### Fase 25 — Asset pipeline async ✅ COMPLETA

Criterio de éxito: cargar un modelo glTF grande no bloquea el render loop. Spinner en egui. Botones deshabilitados mientras carga.

- [x] `AssetLoader` en `src/asset/loader.rs`
- [x] `request_load()` → `std::thread::spawn(load_multi_glb)` → mpsc channel
- [x] `poll_complete()` non-blocking; main loop llama cada frame
- [x] `load_scene()` no bloquea: lee scene.json, guarda SceneFile, llama request_load
- [x] `apply_loaded_scene()`: GPU upload + ECS rebuild cuando poll_complete retorna Some
- [x] `SceneUiState.is_loading` sincronizado; panel muestra spinner + botones disabled

---

### Fase 26 — Scripting con Lua ✅ COMPLETA

Criterio de éxito: un archivo `scripts/game.lua` que se ejecuta al arrancar, puede spawnear entidades y se hot-reloadea al guardar.

- [x] `mlua = { version = "0.10", features = ["lua54", "vendored"] }` en Cargo.toml
- [x] `src/scripting/mod.rs` — `ScriptEngine` con bootstrap Lua embebido
- [x] Pure-Lua command queue (`_engine_cmds`, `_engine_timers`) — sin lifetime/Send issues
- [x] API en Lua: `engine.spawn`, `engine.destroy`, `engine.set_position`, `engine.play_sound`, `engine.log`, `engine.every`
- [x] `ScriptEngine::update(dt)` — tick timers, drena comandos, retorna `Vec<ScriptCommand>`
- [x] Hot-reload: `notify` watcher sobre `scripts/`, recarga .lua al cambiar
- [x] Error handling: script error → log en panel, script disabled, motor no crashea
- [x] `scene.json` campo `"scripts": ["scripts/game.lua"]`
- [x] Panel egui "Scripts": lista con ●/○ status, log reciente scrollable
- [x] `scripts/game.lua` ejemplo: spawnea cubos random cada 2s con `engine.every`
- Collision callbacks (`engine.on_collision`): pendiente M5
- `engine.get_position(id)`: pendiente (requiere entity tracking Lua↔hecs)

---

### Entregable final del Milestone 4

Motor con:
- SSAO para calidad visual en oclusiones
- GPU profiler mostrando tiempo por render pass
- GPU-driven rendering: indirect draws + compute culling
- LOD automático con meshoptimizer
- Asset loading async con spinner UI
- Scripting Lua con API de engine (spawn, physics, audio, timers, hot-reload)

**Esto prueba:** el motor rinde con escenas grandes, se puede perfilar, y la lógica de juego se escribe en Lua sin recompilar Rust. Es un motor de verdad.

---

## Después del Milestone 4 (no planificar todavía)

- **M5 "Advanced rendering"** — Ray tracing opcional (VK_KHR_ray_tracing_pipeline), mesh shaders, virtual geometry (Nanite-like), SSR, volumetric fog, global illumination, clustered forward rendering
- **M6 "Ship it"** — Networking (multiplayer), UI system in-game (no egui), packaging/distribution, documentation, example game

---

## Principios del roadmap

1. **Cada fase tiene un test visual.** Si no lo ves en pantalla, no está terminado.
2. **No adelantar fases.** La fase 23 (GPU-driven) asume que la 22 (profiler) funciona para medir.
3. **Commit por fase.**
4. **"Funciona feo" es mejor que "no funciona bonito."**
5. **El profiler es obligatorio antes de optimizar.** Sin datos, estás adivinando.
