# ralk

Motor 3D escrito en Rust con Vulkan, hecho para Linux.

## Por qué

No existe un motor de videojuegos que trate Linux y Vulkan como ciudadanos de primera clase. Los motores grandes (Unreal, Unity) son DirectX-first. Los motores Rust existentes o pasan por capas de abstracción que ocultan Vulkan (Bevy usa wgpu), o usan OpenGL (Fyrox), o fueron abandonados (Kajiya). Mientras tanto, los drivers Vulkan de Linux son hoy mejores que nunca: Mesa RADV es el driver oficial de AMD desde mayo 2025, Vulkan 1.4 es conformante en AMD, Intel y NVIDIA, y la Steam Deck corre Vulkan nativamente.

Hay un hueco real y este proyecto lo llena.

## Qué es

Un motor 3D con control directo sobre Vulkan, pensado para renderizar escenas 3D con iluminación PBR, carga de modelos glTF, cámara libre, y un game loop completo. No es un wrapper ni un framework — es un motor que habla Vulkan sin intermediarios.

## Stack

| Capa | Herramienta |
|------|-------------|
| Gráficos | ash (Vulkan 1.3+) + gpu-allocator |
| Ventana | winit (Wayland + X11) |
| Input | winit + gilrs (gamepad) |
| Matemática | glam |
| Modelos | gltf |
| Texturas | image |
| Shaders | GLSL → SPIR-V via shaderc |

## Cómo funciona

El motor se construye en 12 fases incrementales, cada una con un entregable visible:

1. Ventana + Vulkan init → pantalla de color sólido
2. Triángulo → primer draw call
3. Resource manager → buffers y texturas abstraídos
4. Cámara 3D → movimiento WASD + mouse
5. Carga glTF → modelos 3D en pantalla
6. Iluminación Blinn-Phong → volumen visual
7. Texturas PBR → materiales realistas
8. Depth + multi-objeto → escenas completas
9. Game loop → timing fijo, input robusto, gamepad
10. ECS → organización de entidades
11. Render graph + sombras → multi-pass rendering
12. Skybox, debug UI, hot-reload de shaders

Cada fase es funcional por sí sola. El detalle de cada una está en `docs/strategy.md`.

## Requisitos

- Linux con Vulkan 1.3+ (Mesa 22.0+ o NVIDIA propietario)
- Rust 1.75+ (edición 2021)
- Drivers: RADV (AMD), ANV (Intel), o NVIDIA propietario
- Para gamepads: usuario en grupo `input`

## Uso

```bash
cargo run                              # debug con validation layers
cargo run --release                    # release
WINIT_UNIX_BACKEND=x11 cargo run       # forzar X11
```

## Estado

Proyecto en construcción activa. Ver `docs/strategy.md` para el roadmap.

## Licencia

MIT
