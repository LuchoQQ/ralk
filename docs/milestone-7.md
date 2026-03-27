# Milestone 7: "Las herramientas" (pendiente)

**Objetivo:** Convertir el sandbox en un motor usable para armar escenarios reales. Todo lo que te falta no es rendering — son las herramientas que hacen que Unity/Godot sean productivos: jerarquía padre-hijo, prefabs reutilizables, partículas, animaciones, triggers, terreno pintable, y editor de materiales.

**Estado actual al empezar M7:** Placement de props desde JSON, gizmos, persistencia, player con salto, menú principal, config screen, day/night. Todo flat en el ECS (sin jerarquía). Sin partículas, sin animaciones, sin triggers, sin terrain painting, sin editor de materiales.

**Entregable:** Armar un escenario completo: terreno pintado (pasto + tierra + roca), árboles como prefabs instanciados, una puerta que se abre cuando el player entra en una zona trigger, antorchas con partículas de fuego, un personaje con animación idle, y materiales editados visualmente en el motor. Todo guardable y cargable.

---

### Fase 38 — Jerarquía padre-hijo (semana 1)

Criterio de éxito: attachar un objeto a otro. Mover el padre mueve los hijos. El scene tree en egui muestra la jerarquía con indent.

- [ ] Componente `Parent { entity: hecs::Entity }` y `Children { entities: Vec<hecs::Entity> }`
- [ ] Al mover un padre: los hijos heredan la transformación
  - `world_transform = parent.to_mat4() * child.to_mat4()`
  - Sistema que recalcula `world_transform` cada frame recorriendo la jerarquía
- [ ] API: `attach_child(parent, child)` → setea Parent en el hijo, agrega a Children del padre
- [ ] API: `detach_child(child)` → remueve Parent, remueve de Children del padre, child conserva su world transform como local
- [ ] Scene tree en egui: mostrar entidades con indent según jerarquía
  - Drag and drop para reparentar
  - Click en flecha para colapsar/expandir hijos
- [ ] Serialización: `scene.json` guarda `parent_index` por entidad (null = root)
- [ ] Gizmos: mover un padre mueve todo el sub-árbol
- [ ] Delete padre: pregunta "¿Borrar con hijos o solo el padre?" (desattachar hijos si solo padre)
- [ ] Ejemplo: attachar un cubo a un poste de luz → mover el poste mueve el cubo

---

### Fase 39 — Prefabs (semana 1-2)

Criterio de éxito: seleccionar un grupo de objetos → "Guardar como prefab" → aparece en el catálogo → colocarlo instancia todo el grupo con un click.

- [ ] Prefab = un mini scene.json guardado en `assets/prefabs/{nombre}.json`
  - Contiene: lista de entidades con transforms locales, jerarquía, componentes
  - Al instanciar: spawnear todas las entidades con offsets relativos a la posición de colocación
- [ ] Crear prefab:
  - Seleccionar 1+ objetos en la escena
  - Click "Guardar como prefab" en egui → pide nombre
  - Serializa las entidades seleccionadas (con hijos) en un `.json` en `assets/prefabs/`
  - Calcula transforms relativos al centro del grupo
- [ ] Instanciar prefab:
  - Aparece en el panel de Items como categoría "Prefabs"
  - Click → modo placement → click para colocar → spawnea todas las entidades del prefab
- [ ] Componente `PrefabInstance { prefab_path: String }` → trackea de qué prefab viene
- [ ] Modificar instancia: si modificás una instancia y "aplicás al prefab", actualiza el `.json`
  - Todas las instancias existentes NO se actualizan automáticamente (eso es avanzado)
  - Pero nuevas instancias usan el prefab actualizado
- [ ] Prefabs anidados: un prefab puede contener instancias de otros prefabs
- [ ] Serialización: `scene.json` guarda `prefab_path` + overrides de transform por instancia
- [ ] Ejemplo: armar un "campsite" (fogata + 3 troncos + luz) → guardar como prefab → colocar 5 campsites en la escena con un click cada uno

---

### Fase 40 — Partículas (semana 2-3)

Criterio de éxito: un emisor de partículas colocable que simula fuego. Toggle en egui. Se ve como fuego de antorcha simple.

- [ ] Crear `src/engine/particles.rs` — sistema de partículas CPU-side
- [ ] `ParticleEmitter` componente:
  ```
  ParticleEmitter {
      max_particles: u32,       // pool size
      spawn_rate: f32,          // partículas por segundo
      lifetime: Range<f32>,     // vida de cada partícula (min-max)
      initial_velocity: Vec3,   // dirección + fuerza base
      velocity_randomness: f32, // variación random
      gravity_factor: f32,      // 0 = sin gravedad, 1 = gravedad normal
      start_size: Range<f32>,   // tamaño inicial (min-max)
      end_size: Range<f32>,     // tamaño al morir
      start_color: [f32; 4],    // RGBA inicio
      end_color: [f32; 4],      // RGBA al morir (fade out)
      emitter_shape: EmitterShape, // Point, Sphere, Cone, Box
  }
  ```
- [ ] `Particle` struct (internal, no ECS): position, velocity, age, lifetime, size, color
- [ ] Update cada frame: spawnear nuevas, mover existentes (velocity + gravity), interpolar size/color, matar las que exceden lifetime
- [ ] Rendering: billboarded quads (siempre miran a la cámara)
  - Vertex buffer dinámico que se actualiza cada frame con las partículas vivas
  - Pipeline con blending aditivo (fuego) o alpha blend (humo)
  - Depth test ON, depth write OFF (transparencia)
  - Texture: un círculo con falloff gaussian (o textura de llama simple)
- [ ] Presets en `assets/particles/`:
  ```json
  { "name": "Fuego", "spawn_rate": 30, "lifetime": [0.3, 0.8], "velocity": [0, 2, 0],
    "randomness": 0.5, "gravity": -0.5, "start_size": [0.2, 0.3], "end_size": [0.0, 0.05],
    "start_color": [1, 0.6, 0.1, 1], "end_color": [1, 0, 0, 0], "blend": "additive" }
  ```
  Otros: humo (gris, alpha blend, sube lento), chispas (amarillo, spawn burst, gravity alto)
- [ ] Colocable como prop: "Emisor de fuego" en el catálogo → spawnea entidad con ParticleEmitter
- [ ] Editor en egui: al seleccionar un emisor, mostrar todos los parámetros con sliders
- [ ] Serialización: ParticleEmitter se guarda en scene.json

---

### Fase 41 — Animación básica (semana 3-4)

Criterio de éxito: un modelo glTF con animación (ej: personaje idle) se reproduce en loop. Una puerta rota de 0° a 90° cuando recibe un evento.

- [ ] Skeletal animation (para modelos con huesos):
  - Parsear animation data de glTF: channels, samplers, keyframes
  - `AnimationClip` struct: nombre, duración, keyframes por joint (translation, rotation, scale)
  - `AnimationPlayer` componente: clip activo, tiempo actual, speed, loop on/off
  - Sistema: cada frame avanzar tiempo, interpolar keyframes (lerp/slerp), aplicar transforms a los joints
  - Skinning: joint matrices → SSBO o UBO → vertex shader multiplica por bone weights
  - Necesita extender Vertex con `joint_indices: [u16; 4]` y `joint_weights: [f32; 4]`
- [ ] Property animation (para objetos sin huesos):
  - `PropertyAnimator` componente: anima campos de Transform (position, rotation, scale) entre valores A y B
  - Parámetros: duración, easing (linear, ease-in-out), loop, ping-pong, play-on-trigger
  - Ejemplo: puerta que rota Y de 0° a 90° en 0.5s con ease-in-out
- [ ] Controles en egui: play/pause/stop por entidad, speed slider, clip selector
- [ ] Presets de property animation en JSON (puerta, plataforma que sube/baja, objeto que rota)
- [ ] Serialización: AnimationPlayer y PropertyAnimator se guardan en scene.json
- [ ] Ejemplo: personaje con idle animation + puerta con rotation de 0→90° on trigger

---

### Fase 42 — Triggers y zonas (semana 4)

Criterio de éxito: colocar una zona invisible, cuando el player entra → se ejecuta una acción (abrir puerta, spawnear objeto, reproducir sonido, activar animación).

- [ ] Componente `TriggerZone`:
  ```
  TriggerZone {
      shape: TriggerShape,    // Box, Sphere
      size: Vec3,             // dimensiones
      on_enter: TriggerAction,
      on_exit: TriggerAction,
      once: bool,             // disparar solo una vez
      triggered: bool,        // ya se disparó
  }
  ```
- [ ] `TriggerAction` enum:
  - `PlayAnimation { target: Entity, clip: String }`
  - `PlaySound { path: String, volume: f32 }`
  - `SpawnPrefab { prefab: String, offset: Vec3 }`
  - `SetProperty { target: Entity, property: String, value: f32 }` (ej: light intensity)
  - `RunScript { script: String, function: String }`
  - `ToggleEntity { target: Entity, enabled: bool }` (show/hide)
- [ ] Detección: rapier sensor collider (no bloquea, solo detecta overlap)
  - Cada frame: verificar si el player está dentro de algún trigger
  - Si entra (estaba fuera, ahora dentro) → ejecutar `on_enter`
  - Si sale (estaba dentro, ahora fuera) → ejecutar `on_exit`
- [ ] Colocable: "Zona trigger" en el catálogo → spawnea con TriggerZone + gizmo de tamaño
  - Visible como wireframe en modo editor, invisible en modo juego
- [ ] Editor en egui: al seleccionar trigger, configurar shape, size, acción, target entity
  - Dropdown de entidades en la escena para seleccionar target
  - Dropdown de acciones disponibles
- [ ] Conectar con animación: trigger on_enter → PlayAnimation de una puerta → puerta se abre
- [ ] Conectar con Lua: trigger on_enter → RunScript → ejecutar función Lua custom
- [ ] Serialización: TriggerZone se guarda en scene.json con referencias a entidades por nombre/id

---

### Fase 43 — Terrain painting (semana 4-5)

Criterio de éxito: seleccionar un brush, pintar textura de pasto/tierra/roca sobre el terreno, cambiar tamaño del brush, blend suave entre texturas.

- [ ] Splatmap: textura RGBA donde cada canal = peso de una textura del terreno
  - R = textura 1 (pasto), G = textura 2 (tierra), B = textura 3 (roca), A = textura 4 (arena)
  - Resolución: 512×512 o 1024×1024 (configurable)
  - Shader del terreno: `color = tex1 * splat.r + tex2 * splat.g + tex3 * splat.b + tex4 * splat.a`
- [ ] Crear `shaders/terrain.frag` con splatmap sampling
- [ ] Pipeline de terreno separado (o variante del main pipeline)
- [ ] Herramienta de painting:
  - Botón "Paint terrain" en egui → entra en modo painting
  - Elegir textura activa (1-4) con botones o teclas 1-2-3-4
  - Click + drag sobre el terreno → escribir en la splatmap
  - Brush: radio (slider), intensidad (slider), falloff (hard/soft)
  - El brush modifica la textura splatmap en CPU → re-upload a GPU cada frame que se pinta
- [ ] Texturas de terreno definidas en `terrain.json`:
  ```json
  {
    "name": "Pradera",
    "size": [100, 100],
    "splatmap_resolution": 512,
    "layers": [
      { "name": "Pasto", "albedo": "grass.png", "normal": "grass_n.png", "scale": 10.0 },
      { "name": "Tierra", "albedo": "dirt.png", "normal": "dirt_n.png", "scale": 8.0 },
      { "name": "Roca", "albedo": "rock.png", "normal": "rock_n.png", "scale": 6.0 },
      { "name": "Arena", "albedo": "sand.png", "normal": "sand_n.png", "scale": 12.0 }
    ]
  }
  ```
- [ ] Splatmap se guarda como PNG en `scenes/{nombre}_splatmap.png`
- [ ] Al cargar escena: cargar splatmap, crear textura GPU, aplicar al terreno
- [ ] Preview del brush: círculo en el terreno que sigue el mouse (shader o line overlay)

---

### Fase 44 — Editor de materiales (semana 5-6)

Criterio de éxito: seleccionar un objeto, abrir panel de material, ajustar roughness/metallic/color con sliders y ver el cambio en tiempo real.

- [ ] Panel "Material" en egui (aparece al seleccionar un objeto):
  - Color base: color picker RGB × albedo texture
  - Metallic: slider 0.0 → 1.0 (overridea el valor del glTF)
  - Roughness: slider 0.0 → 1.0
  - Normal intensity: slider 0.0 → 2.0 (escala el normal map)
  - Emissive color: color picker (para objetos que brillan)
  - Emissive intensity: slider 0.0 → 10.0
  - Tiling: slider para UV scale
  - Preview: esfera con el material actual (render-to-texture mini)
- [ ] Implementación: los sliders modifican un `MaterialOverride` componente en el ECS
  ```
  MaterialOverride {
      base_color_factor: Option<[f32; 4]>,
      metallic_factor: Option<f32>,
      roughness_factor: Option<f32>,
      emissive_factor: Option<[f32; 3]>,
      normal_scale: Option<f32>,
      uv_scale: Option<f32>,
  }
  ```
- [ ] En el render: si la entidad tiene `MaterialOverride`, pasar los valores como push constants o UBO adicional al shader
  - `triangle.frag` ya usa `base_color_factor`, `metallic_factor`, `roughness_factor` del glTF
  - Agregar overrides que se aplican encima
- [ ] "Guardar material": exportar el override como `assets/materials/{nombre}.json`
- [ ] "Aplicar material": dropdown con materiales guardados → aplicar a la entidad seleccionada
- [ ] Serialización: `MaterialOverride` se guarda en scene.json por entidad
- [ ] Emissive: agregar `emissive` al fragment shader si no existe → `color += emissive * emissive_intensity`
  - Los objetos emissivos contribuyen al bloom (ya existe del M3)

---

### Entregable final del Milestone 7

Herramientas de motor real:
- Jerarquía padre-hijo con drag-and-drop en scene tree
- Prefabs: guardar grupo de objetos → colocar instancias con un click
- Partículas: fuego, humo, chispas colocables con editor de parámetros
- Animación: skeletal (personajes) + property (puertas, plataformas)
- Triggers: zonas que disparan acciones (abrir puerta, sonido, spawn, script Lua)
- Terrain painting: pintar hasta 4 texturas sobre el terreno con brush
- Editor de materiales: roughness/metallic/color/emissive en vivo

**Esto prueba:** vibe-engine tiene las herramientas que hacen productivo a un motor. No es solo un renderer — es una herramienta para armar mundos.

---

## Principios

1. **Herramientas > features.** Una herramienta mediocre que funciona vale más que una feature brillante que nadie puede usar.
2. **Todo editable en egui.** Si tenés que editar JSON a mano para configurar algo, falta un panel.
3. **Todo serializable.** Si lo pusiste en la escena, tiene que guardarse y cargarse.
4. **Composición.** Prefab + trigger + animación = puerta que se abre. No hace falta código custom.