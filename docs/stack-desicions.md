# Decisiones de stack — ralk

Cada herramienta fue elegida por una razón concreta. Este documento explica el por qué, qué alternativas se descartaron, y bajo qué condiciones reconsideraríamos.

---

## Vulkan bindings: ash 0.38

**Elegido porque:** Es la fundación de todo el ecosistema Rust Vulkan. wgpu y vulkano dependen de ash internamente. Usando ash, trabajamos al mismo nivel que esos proyectos pero sin su overhead de abstracción. Para un motor que quiere ser Vulkan-first en Linux, necesitamos acceso directo a features como ray tracing pipelines, mesh shaders, descriptor indexing, y dynamic rendering. Ninguna abstracción los expone todos.

**Descartados:**

- **wgpu** (~16.7K stars, 12.8M descargas) — la opción más popular. Implementa WebGPU, una API diseñada para portabilidad (Vulkan + Metal + DX12 + WebGPU). El problema: oculta conceptos Vulkan detrás de una abstracción genérica. Ray tracing es parcial, mesh shaders recién se agregaron en v28, y no podés controlar sincronización, memory barriers, ni pipeline caches manualmente. Para un motor que trata Vulkan como target primario, wgpu es un middleman innecesario. Si en el futuro quisiéramos portabilidad a Web o macOS, wgpu sería la herramienta — pero no es nuestro caso.

- **vulkano** (5K stars, 960K descargas) — wrapper seguro de ash con validación en compile-time y runtime. Buena idea, pero: documentación desactualizada, ecosistema más chico que ash o wgpu, depende de shaderc para su macro de shaders, y las abstracciones de seguridad agregan fricción cuando necesitás hacer algo no estándar. Los variants `_unchecked` existen para saltear la validación, pero si vas a usar unsafe igual, mejor usar ash directamente.

- **erupt** — abandonado. Su README dice textualmente "use ash instead."

- **vulkanalia** — alternativa a ash con un tutorial portado de vulkan-tutorial.com. Útil para aprender, pero ash tiene más adopción, más dependents (14.6M descargas), y contribución activa.

**Reconsiderar si:** Quisiéramos portabilidad a macOS/Web (→ wgpu) o si el equipo creciera y necesitáramos más safety en compile-time (→ vulkano).

---

## Memoria GPU: gpu-allocator

**Elegido porque:** Puro Rust, usado internamente por wgpu (lo que valida su robustez), maneja suballocación con el algoritmo de offset allocator. Vulkan no tiene malloc — vos manejás la memoria, y hacerlo mal es el error #1 en renderers Vulkan. gpu-allocator resuelve eso sin dependencias C.

**Descartados:**

- **VMA (Vulkan Memory Allocator)** de AMD — el estándar en C/C++. Existen bindings Rust (`vk-mem`), pero agregan una dependencia de build C++ pesada. gpu-allocator logra lo mismo sin FFI.

- **Allocación manual** — técnicamente posible, y hay tutoriales que lo enseñan. En la práctica, vas a fragmentar la memoria del GPU, leakear allocations, y pasar semanas debuggeando. No vale la pena para un proyecto que quiere avanzar.

**Reconsiderar si:** Necesitáramos features muy específicas de VMA (defragmentation, budget tracking avanzado) que gpu-allocator no tenga.

---

## Ventana: winit 0.30

**Elegido porque:** Nativo Rust, 34.9M descargas, usado por Bevy y wgpu. Soporta Wayland y X11 con selección automática de backend. Implementa `raw_window_handle` que conecta directamente con ash-window para crear Vulkan surfaces. Mantenimiento activo con reuniones semanales de maintainers.

**Descartados:**

- **rust-sdl2** (2.9K stars, 2.78M descargas) — wrappea la librería C SDL2. Ventaja: gamepad integrado (el mejor de la industria), audio, y décadas de bug fixes en plataformas exóticas. Desventaja: dependencia C que complica builds, API no-Rustera (callbacks estilo C), y duplica funcionalidad con winit para windowing. Decisión: usar winit para ventanas y gilrs para gamepads nos da lo mejor de ambos sin la dependencia C.

- **glfw-rs** — parte del ecosistema Piston, que está en declive. Funciona pero no tiene momentum de comunidad.

- **Raw Wayland/X11** (smithay-client-toolkit, x11rb) — control total pero 1000+ líneas de boilerplate solo para abrir una ventana y crear una surface. winit los usa internamente. Solo justificable si necesitáramos protocolos Wayland bleeding-edge que winit no expone.

**Reconsiderar si:** Encontráramos bugs críticos de winit en Wayland que bloqueen desarrollo (posible con NVIDIA), en cuyo caso SDL2 sería el fallback inmediato.

---

## Gamepad: gilrs 0.11

**Elegido porque:** Puro Rust, usa evdev directamente en Linux, soporta hotplug, force feedback, dead zones, y lee `SDL_GAMECONTROLLERCONFIG` para compatibilidad con Steam. Bevy lo usa vía `bevy_gilrs`. No tiene dependencia en SDL2.

**Descartado:**

- **SDL2 GameController** (vía rust-sdl2) — técnicamente superior en cobertura de dispositivos y tiene la base de datos de mapeos más grande. Pero trae toda la dependencia SDL2 solo por gamepads.

**Reconsiderar si:** gilrs no reconociera un gamepad específico que necesitamos, o si adoptáramos SDL2 por otra razón (audio, por ejemplo).

---

## Matemática: glam 0.32

**Elegido porque:** Es el estándar de facto para game math en Rust. 42.8M descargas, usado por Bevy y Rapier. Tipos concretos (`Vec3`, `Mat4`, `Quat`) sin genéricos — la API es directa y predecible. SIMD por defecto en x86_64 (SSE2) y ARM (NEON) sin configuración. En 2025, Rapier (el motor de física dominante en Rust) migró su API pública de nalgebra a glam porque los builds en debug eran 20% más rápidos.

**Descartados:**

- **nalgebra** (58.6M descargas) — más completa matemáticamente: matrices de tamaño dinámico, SVD, decomposiciones, tipos genéricos. Pero para operaciones de game engine (multiplicar matrices, rotar vectores, interpolar quaternions), su sistema de genéricos agrega complejidad sin beneficio. El compilador genera peor código en debug por las capas de abstracción. Si necesitamos álgebra lineal general en algún subsistema (IK, procedural generation), podemos usar nalgebra puntualmente sin reemplazar glam.

- **cgmath** — muerto. Último release 2021, dependencia SIMD rota.

- **ultraviolet** — enfoque SoA (Structure of Arrays) con tipos "wide" que procesan 4 vectores simultáneamente. Puede ser 10x más rápido para operaciones batch (ray tracing, partículas) pero requiere reestructurar algoritmos. No es drop-in replacement. Evaluar para subsistemas compute-heavy en el futuro.

**Reconsiderar si:** Necesitáramos álgebra lineal completa (→ nalgebra como complemento, no reemplazo) o batch processing pesado (→ ultraviolet para ese módulo específico).

---

## Modelos 3D: gltf 1.4

**Elegido porque:** glTF es el estándar de la industria para assets 3D en tiempo real. El crate `gltf` soporta glTF 2.0 completo: meshes, PBR materials, animations, skins, scene graphs, extensiones KHR. Maneja tanto JSON como binary (.glb) con zero-copy data access. Bevy y la mayoría de engines Rust lo usan.

**Descartados:**

- **tobj** (OBJ loader) — funciona para meshes simples pero OBJ no soporta PBR, animaciones, ni scene graphs. El propio maintainer de tobj recomienda glTF.

- **assimp-rs** — wrappea Assimp (C++), que carga 40+ formatos. Pero: dependencia C++ pesada, API insegura, y para un motor nuevo no necesitamos formatos legacy. glTF con una pipeline de conversión (Blender export) cubre el 95% de los casos.

**Reconsiderar si:** Necesitáramos importar formatos legacy en masa (FBX, DAE) — ahí assimp sería justificable como herramienta offline, no como dependencia runtime.

---

## Texturas: image 0.25

**Elegido porque:** 82.6M descargas, decodifica PNG, JPEG, WebP, AVIF, OpenEXR, HDR, DDS, QOI y más. Maduro, bien mantenido, puro Rust. Es la elección obvia — no hay competencia real.

**Complemento:** `image_dds` para texturas GPU-compressed (BCn/DXT). En producción, los assets se precomprimen a BC7 y se cargan sin decodificar en CPU — la GPU los descomprime en hardware. `image` decodifica los formatos fuente; `image_dds` maneja los formatos GPU.

---

## Shaders: shaderc-rs

**Elegido porque:** Wrappea Google shaderc, el compilador de referencia para GLSL/HLSL → SPIR-V. Maduro, completo, maneja todas las extensiones GLSL. Para un motor ash-based que escribe shaders en GLSL, shaderc es el camino directo.

**Descartados:**

- **naga** (parte de wgpu) — puro Rust, 13x más rápido que Tint para traducción, zero dependencias de build. Pero su lenguaje primario es WGSL, y su soporte GLSL es un frontend secundario con limitaciones en extensiones avanzadas. Para un motor wgpu, naga es ideal. Para ash + GLSL, shaderc es más completo.

- **glslang-rs** — más bajo nivel que shaderc, sin la capa de conveniencia. shaderc wrappea glslang internamente.

- **rust-gpu** (shaders en Rust) — compila Rust a SPIR-V, permitiendo compartir tipos entre CPU y GPU. Fascinante pero: requiere nightly Rust, no tiene backwards compatibility, y la cobertura de GLSL features es parcial. Es una apuesta a futuro, no una herramienta de producción hoy.

**Desventaja conocida:** shaderc trae una dependencia C++ que tarda ~3 minutos en compilar la primera vez. El feature `bundled` lo compila from source. Es molesto pero se cachea después del primer build.

**Reconsiderar si:** rust-gpu madurara lo suficiente para escribir todos los shaders en Rust (probablemente 1-2 años), o si migráramos a WGSL (→ naga).

---

## Reflexión de shaders: spirv-cross2

**Elegido porque:** Permite inspeccionar un shader SPIR-V compilado y extraer automáticamente: descriptor set layouts, bindings, tipos de uniforms, push constant ranges. En vez de hardcodear `set=0, binding=1` en el código Rust (y rezar que coincida con el shader), spirv-cross2 lee el SPIR-V y genera los layouts.

**Alternativa:** hacerlo manual. Funciona para proyectos chicos, pero con 10+ shaders y materiales variados, el error humano es inevitable.

---

## Versión Vulkan: 1.3 mínimo

**Por qué 1.3 y no 1.4:**

Vulkan 1.3 es conformante en Mesa desde 2022 (Mesa 22.0). Está disponible en literalmente toda GPU de los últimos 5 años en Linux. Incluye todo lo que necesitamos:

- `VK_KHR_dynamic_rendering` — elimina render pass objects
- Timeline semaphores — sincronización más simple
- Synchronization2 — barriers simplificados
- Descriptor indexing — bindless resources

Vulkan 1.4 tiene cosas buenas (push descriptors core, maintenance5) pero Mesa 25.0 recién lo trajo a principios de 2025. Exigir 1.4 excluiría a usuarios con drivers más viejos sin ganar mucho.

**Ray tracing** (`VK_KHR_ray_tracing_pipeline`) es una extensión opcional sobre 1.3, disponible en NVIDIA RTX y AMD RDNA2+. Lo tratamos como feature path, no como requisito.

---

## Resumen de decisiones

| Decisión | Elegido | Razón principal |
|----------|---------|-----------------|
| Vulkan bindings | ash | Control directo, zero overhead, base del ecosistema |
| Memoria GPU | gpu-allocator | Puro Rust, validado por wgpu, sin FFI |
| Ventana | winit | Nativo Rust, Wayland+X11, raw_window_handle |
| Gamepad | gilrs | evdev directo, sin dependencia SDL2 |
| Math | glam | SIMD default, tipos concretos, estándar de facto |
| Modelos | gltf | Estándar de industria, PBR, animaciones, zero-copy |
| Texturas | image + image_dds | Universal, 82M+ descargas, sin competencia |
| Shaders | shaderc-rs | Compilador de referencia para GLSL→SPIR-V |
| Reflexión | spirv-cross2 | Descriptor layouts automáticos |
| Vulkan version | 1.3 mínimo | Universalmente soportado, tiene todo lo necesario |