# Milestone 5: "El juego" (pendiente)

**Objetivo:** Un juego de conducción mínimo viable que demuestre todas las capacidades del motor. Es el ejemplo de referencia para cualquiera que quiera adoptar vibe-engine. 5-10 minutos de gameplay, 1 circuito, ciclo día/noche, física de vehículo, sonido de motor.

**Entregable:** Abrir el juego → menú principal → "Play" → escena con un auto en un circuito → acelerar/frenar/girar → físicas de vehículo creíbles → ciclo día/noche automático con iluminación dinámica → sonido de motor que cambia con RPM → checkpoints → vuelta completada → tiempo en pantalla. Gamepad funcional. 60+ FPS.

**Por qué un juego de conducción:** Ejercita physics (vehículo, colisiones con barreras), rendering dinámico (sol que se mueve, skybox que cambia, sombras que rotan), audio espacial (motor, derrape, ambiente), scripting Lua (lógica de checkpoints, timer, game states), y el editor (colocar el circuito, barreras, props).

---

### Fase 27 — Vehículo con physics (semana 1-2)

Criterio de éxito: auto que acelera, frena, gira y colisiona con paredes. Se siente como un auto, no como una caja sobre hielo.

- [ ] Modelo de vehículo: rigid body dinámico + 4 "ruedas" simuladas con raycasts hacia abajo
  - Cada rueda: raycast desde punto de suspensión → si hay contacto, aplicar fuerza de suspensión (spring-damper)
  - Suspensión: spring stiffness, damping, rest length configurables
  - NO usar joint-based wheels (inestable en rapier) — raycast vehicle es el estándar
- [ ] Componente `Vehicle` en ECS:
  ```
  Vehicle {
      acceleration: f32,      // fuerza de motor
      brake_force: f32,       // fuerza de freno
      max_steer_angle: f32,   // ángulo máximo de giro (radianes)
      current_speed: f32,     // velocidad actual (para UI y audio)
      current_rpm: f32,       // RPM simuladas (para audio)
      wheel_positions: [Vec3; 4],  // offsets locales de las ruedas
  }
  ```
- [ ] Input → physics: acelerar (W/RT), frenar (S/LT), girar (A-D/stick izquierdo)
  - Aceleración: fuerza forward en la dirección del auto
  - Giro: rotar el auto con torque (no cambiar velocidad angular directo)
  - Frenado: fuerza opuesta a la velocidad, más friction lateral para evitar derrape infinito
- [ ] Friction lateral: cuando el auto gira, aplicar fuerza lateral opuesta al deslizamiento (grip)
  - Grip alto = auto pegado al piso
  - Grip bajo = derrape (drift) — el balance entre grip y velocidad define cómo se siente
- [ ] Collider del auto: cápsula o box que represente el chasis
- [ ] Colisión con paredes: barreras estáticas con colliders, el auto rebota
- [ ] Mesh del auto: buscar un modelo glTF gratis (OpenGameArt, Kenney) o usar un box placeholder
- [ ] Debug: mostrar raycasts de suspensión como líneas en wireframe mode
- [ ] Tunear parámetros de physics en egui: stiffness, damping, grip, acceleration, max speed

---

### Fase 28 — Circuito y terreno (semana 2-3)

Criterio de éxito: un circuito cerrado con curvas, rectas, y barreras. El auto no se cae del mundo. Hay props decorativos (árboles, rocas) para dar contexto.

- [ ] Terreno plano como mesh: quad grande con collider estático, textura de asfalto/pasto
  - Opción simple: un plane con UV tiling
  - Opción mejor: heightmap básico (imagen grayscale → mesh con altura) si da el tiempo
- [ ] Track layout: spline o puntos definidos en `scene.json`, barreras a ambos lados
  - Barreras: boxes estáticos con collider, material de concreto/metal
  - Curvas: serie de boxes rotados que forman la curva
- [ ] Línea de salida/meta: mesh plano en el piso con textura de checkered flag
- [ ] Props decorativos: árboles (modelos glTF simples), rocas, postes de luz
  - Posicionados en `scene.json`, spawneados como entidades con Transform + MeshRenderer
  - Sin collider (solo visual) o con collider estático si están cerca del circuito
- [ ] Skybox HDR: ya existe del M2, pero elegir uno que se vea bien con el circuito (outdoor)
- [ ] Cámara third-person:
  - Seguir al auto desde atrás y arriba (offset configurable)
  - Interpolación suave (lerp) para que no sea brusca
  - Mirar siempre al auto
  - Componente `FollowCamera { target: Entity, offset: Vec3, smoothing: f32 }`
- [ ] Reset: si el auto se da vuelta o cae fuera del circuito → tecla R para resetear a último checkpoint

---

### Fase 29 — Ciclo día/noche (semana 3-4)

Criterio de éxito: el sol se mueve por el cielo, las sombras rotan, el skybox cambia de color, la iluminación de la escena cambia de día cálido a noche fría. Un ciclo completo en ~3-5 minutos de juego.

- [ ] Componente `DayNightCycle { time_of_day: f32, cycle_duration: f32, sun_entity: Entity }`
  - `time_of_day`: 0.0 = mediodía, 0.25 = atardecer, 0.5 = medianoche, 0.75 = amanecer, 1.0 = mediodía
  - `cycle_duration`: duración real del ciclo en segundos (ej: 180s = 3 min)
- [ ] Sistema que cada frame: `time_of_day += dt / cycle_duration`
- [ ] Sol (DirectionalLight):
  - Dirección: rotar con time_of_day (arco de este a oeste)
  - Color: mediodía = blanco cálido, atardecer = naranja, noche = azul muy tenue, amanecer = rosa
  - Intensidad: máxima al mediodía, mínima (casi 0) a medianoche
  - Actualizar `DirectionalLight` component → `LightingUbo` se actualiza automáticamente
- [ ] Shadow map: la dirección del shadow cambia con el sol (ya funciona si el sistema mueve el DirectionalLight)
- [ ] Skybox tinting:
  - Opción simple: multiplicar el skybox sample por un color que varía con time_of_day
  - Opción mejor: blend entre 2 cubemaps (día y noche) con factor = time_of_day
  - Pasar factor como uniform al skybox.frag
- [ ] Ambient IBL: escalar irradiance intensity con time_of_day (más débil de noche)
- [ ] Fog/haze (opcional): color de fog que cambia con la hora (dorado al atardecer, azul de noche)
- [ ] Luces artificiales: point lights en los postes del circuito que se encienden de noche
  - `StreetLight { on_threshold: f32 }` — se enciende cuando `time_of_day > 0.35 && time_of_day < 0.65` (rango nocturno)
- [ ] Slider en egui: time_of_day manual (para testing), toggle auto-cycle on/off
- [ ] Game logic en Lua: `engine.get_time_of_day()`, `engine.set_time_of_day(t)`

---

### Fase 30 — Audio del vehículo (semana 4)

Criterio de éxito: sonido de motor que sube de tono al acelerar, baja al soltar, chirrido al frenar fuerte, sonido ambiente de viento. Se siente como manejar.

- [ ] Motor audio: loop de sonido de motor con pitch variable
  - `engine_pitch = base_pitch + (current_rpm / max_rpm) * pitch_range`
  - rodio soporta `speed()` en Sink para cambiar pitch en runtime
  - RPM simuladas: suben con aceleración, bajan con freno, idle cuando no se toca nada
- [ ] Transiciones de RPM: lerp suave para evitar cambios bruscos de pitch
- [ ] Sonido de derrape/chirrido: trigger cuando velocidad lateral > threshold
  - Play loop mientras dure el derrape, stop cuando agarre grip
- [ ] Sonido de colisión: contacto con barrera → impact sound, volumen proporcional a la velocidad de impacto
  - Reusar el sistema de contact events de rapier del M3
- [ ] Ambiente: viento que sube de volumen con la velocidad del auto
  - `wind_volume = clamp(speed / max_speed, 0.05, 0.8)`
- [ ] Assets de audio: buscar sonidos libres (freesound.org, OpenGameArt)
  - engine_loop.ogg, skid.ogg, impact.wav, wind_loop.ogg
- [ ] Controles en egui: master volume, individual volumes (motor, ambiente, efectos)

---

### Fase 31 — Game logic: checkpoints, timer, UI (semana 4-5)

Criterio de éxito: el auto cruza checkpoints en orden, completa una vuelta, el timer muestra el tiempo, pantalla de "Vuelta completa" con el tiempo final.

- [ ] Checkpoints: entidades con collider trigger (sensor en rapier, no bloquea)
  - Posicionados a lo largo del circuito en `scene.json`
  - Componente `Checkpoint { index: u32, is_finish_line: bool }`
  - Mesh: arco o poste visible, o invisible (solo trigger zone)
- [ ] Sistema de checkpoints (Lua o Rust):
  - Trackear `next_checkpoint_index` por vehículo
  - Cuando el auto entra en un trigger: si `checkpoint.index == next_checkpoint_index`, avanzar
  - Si es finish line Y todos los checkpoints previos fueron tocados → vuelta completa
  - Previene atajos: no podés cruzar la meta sin pasar por todos los checkpoints
- [ ] Timer:
  - Empieza al cruzar la línea de salida por primera vez
  - Se detiene al completar la vuelta
  - Formato: MM:SS.mmm
- [ ] HUD in-game (egui o sistema propio):
  - Velocidad actual (km/h): `speed_kmh = current_speed * 3.6`
  - Timer de vuelta
  - Indicador de checkpoint actual / total
  - Mini-mapa (opcional): vista top-down del circuito con posición del auto
  - RPM gauge (barra que sube/baja)
- [ ] Game states:
  - `Menu` → mostrar "PLAY" / "QUIT"
  - `Countdown` → 3... 2... 1... GO! (3 segundos, auto no puede moverse)
  - `Racing` → timer corre, input activo
  - `Finished` → "Vuelta completa! Tiempo: XX:XX.XXX" + "Reintentar" / "Menú"
- [ ] State machine en Lua (o Rust):
  ```lua
  engine.on_state("countdown", function(dt)
      countdown_timer = countdown_timer - dt
      if countdown_timer <= 0 then engine.set_state("racing") end
  end)
  engine.on_state("racing", function(dt)
      -- checkpoints, timer
  end)
  ```
- [ ] Input: Escape → pausar → menú de pausa ("Continuar" / "Reiniciar" / "Salir")
- [ ] Gamepad: todo funcional con gamepad (acelerador en trigger, giro en stick)

---

### Fase 32 — Polish y empaquetado (semana 5-6)

Criterio de éxito: alguien descarga el repo, hace `cargo run --example driving_game`, y juega sin configurar nada. El README del juego explica cómo se hizo con vibe-engine.

- [ ] Estructura del juego como example o subcrate:
  ```
  examples/driving_game/
  ├── Cargo.toml          # dependencia: vibe-engine = { path = "../.." }
  ├── src/main.rs          # entry point del juego
  ├── scripts/
  │   ├── game.lua         # lógica de checkpoints, states
  │   └── vehicle.lua      # tuning de vehículo (opcional)
  ├── assets/
  │   ├── scene.json       # circuito, props, checkpoints, luces
  │   ├── models/          # auto, árboles, barreras, postes
  │   ├── textures/        # asfalto, pasto, cielo
  │   ├── sounds/          # motor, derrape, impacto, viento
  │   └── env.hdr          # skybox outdoor
  └── README.md            # cómo jugar, controles, cómo se hizo
  ```
- [ ] README del juego: controles (teclado + gamepad), screenshots, "Built with vibe-engine"
- [ ] README del motor (raíz): agregar sección "Example game" con screenshot y link al example
- [ ] Performance pass: correr el profiler GPU (fase 22), identificar bottleneck, optimizar si hay algo obvio
- [ ] Verificar en Linux (si tenés acceso): probar que compile y corra
- [ ] Assets: verificar que todos sean libres (CC0, MIT, OFL) — listar licencias en `assets/LICENSES.md`
- [ ] Best lap: guardar el mejor tiempo en un archivo local (JSON)
- [ ] Screenshot/GIF del juego para el README del repo

---

### Entregable final del Milestone 5

Juego de conducción jugable:
- Auto con physics de vehículo (suspensión raycast, grip, drift)
- Circuito cerrado con barreras, props, checkpoints
- Ciclo día/noche con sol, sombras, y skybox dinámicos
- Audio: motor con pitch variable, derrape, colisión, viento, ambiente
- HUD: velocidad, timer, checkpoints, RPM
- Game states: menú → countdown → racing → finished
- Gamepad completo
- Empaquetado como example del motor

**Esto prueba:** vibe-engine puede hacer un juego real. Es el mejor README posible para un motor open source.

---

## Después del Milestone 5

- **M6 "Open source launch"** — Documentación completa (book estilo Fyrox/Bevy), API docs, tutoriales, CI/CD, crates.io publish, website, licencia, contributing guide
- **M7 "Advanced rendering"** — Ray tracing, SSR, volumetric fog, GI, mesh shaders, virtual geometry
- **M8 "Multiplayer"** — Networking, client-server, rollback, lobby

## Principios del roadmap

1. **El juego manda.** Si una feature del motor no sirve para el juego, no se hace ahora.
2. **Assets libres.** Todo CC0/MIT. Nadie quiere problemas legales al clonar el repo.
3. **El juego ES la documentación.** Cada sistema del motor se demuestra en el juego.
4. **Commit por fase.** El juego es jugable (aunque feo) desde la fase 28 en adelante.
5. **Fun first.** Si el auto no se siente bien de manejar, nada más importa. La fase 27 es la más importante.