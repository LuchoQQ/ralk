# Gotchas — ralk

Errores reales que se repiten. Si algo te explota y no sabés por qué, buscá acá primero.

---

## Vulkan core

### Orden de destrucción
Vulkan requiere destrucción en orden inverso a la creación. Rust `Drop` NO garantiza orden entre campos de un struct. Si tu struct tiene `device` y `swapchain`, el compilador puede dropear `device` primero y Vulkan crashea.

**Solución:** Drop manual explícito en orden, o wrappers con `ManuallyDrop<T>`.

### Layout transitions olvidadas
El bug más silencioso. Si copiás datos a una `VkImage` sin transicionar de `UNDEFINED` → `TRANSFER_DST_OPTIMAL`, los datos llegan corruptos o no llegan. Validation layers lo reportan como **warning**, no error — fácil de ignorar.

**Regla:** Toda imagen recién creada está en `UNDEFINED`. Antes de cualquier operación, transicioná explícitamente.

### Swapchain outdated en Wayland
En X11, resize de ventana genera `VK_ERROR_OUT_OF_DATE_KHR` limpiamente. En Wayland, algunos compositors **no** lo reportan. Tu frame se renderiza en el tamaño viejo y se estira.

**Solución:** Comparar el tamaño del surface con el de la swapchain cada frame. Si difieren, recrear aunque Vulkan no se queje.

### Push constants: límite de 128 bytes
La spec Vulkan garantiza **mínimo 128 bytes** para push constants. Una `mat4` MVP son 64 bytes — cabe perfecto. Pero si agregás datos extra (color, time, flags), podés pasarte en GPUs con el mínimo.

**Solución:** Verificar `maxPushConstantsSize` del physical device. Para datos que excedan 128 bytes, usar uniform buffer.

### Dynamic rendering vs render passes
Usamos `VK_KHR_dynamic_rendering` (core en Vulkan 1.3, extensión KHR en Vulkan 1.2/MoltenVK). NO crear objetos `VkRenderPass` ni `VkFramebuffer`. Son la API vieja, más verbosa, y dynamic rendering hace lo mismo con menos código.

**Excepción:** Si necesitás subpasses para deferred rendering on-tile (móviles), ahí sí se necesitan render passes. En desktop Linux, no aplica.

### Descriptor set layout mismatch
Si tu shader dice `layout(set=0, binding=1)` pero el descriptor set layout tiene ese recurso en `binding=0`, Vulkan **no crashea** — renderiza basura o negro. Validation layers a veces lo atrapan, a veces no.

**Solución:** Usar spirv-cross2 para reflexión automática de bindings, o ser extremadamente riguroso con los números.

### Semáforos y fences
Vulkan no espera a que la GPU termine nada por defecto. Si submitís frame N+1 mientras frame N todavía se ejecuta y ambos escriben al mismo buffer, corrupción de datos.

**Regla:** Un fence por frame in-flight. `vkWaitForFences` antes de reusar command buffers de ese frame. Semáforos entre acquire → render → present.

### Depth buffer formato
`VK_FORMAT_D32_SFLOAT` es el más preciso pero no todos los GPUs lo soportan como depth attachment. `VK_FORMAT_D24_UNORM_S8_UINT` es el más compatible.

**Solución:** Queryar `vkGetPhysicalDeviceFormatProperties` para `DEPTH_STENCIL_ATTACHMENT` antes de crear.

---

## ash (Rust bindings)

### unwrap() en el render loop
Funciones ash retornan `Result`. Muchos errores son "imposibles" en práctica (ej: `vkBeginCommandBuffer` falla solo si el command buffer es inválido, que no debería pasar). Está bien `.unwrap()` en init, pero en el render loop un panic mata el programa sin cleanup de GPU.

**Solución:** `.unwrap()` en init. En render loop, propagar errores o al menos loggear y intentar recovery.

### gpu-allocator: free antes de drop
`gpu-allocator` **paniquea en debug** si dropeás el allocator con allocations vivas. En release, leakea silenciosamente.

**Solución:** Liberar explícitamente toda allocation (buffers, imágenes) antes de dropear el allocator. Implementar un recurso tracker que lo fuerce.

### bytemuck y repr(C)
`bytemuck::cast_slice` requiere que el struct sea `#[repr(C)]`, `Pod` y `Zeroable`. Sin `repr(C)`, el compilador de Rust reordena los campos — tus datos llegan al shader con offsets incorrectos. El vertex shader lee posición donde hay normales.

**Regla:** Todo struct que va a un buffer GPU lleva:
```rust
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
```

### ash builders y lifetimes
Los builders de ash (ej: `vk::RenderingInfo::default()`) toman references a arrays. Si el array se crea inline en el builder chain, la referencia queda dangling.

```rust
// MAL — el Vec se destruye al final de la línea
let info = vk::RenderingInfo::default()
    .color_attachments(&vec![attachment]); // ← dangling

// BIEN — el binding vive lo suficiente
let attachments = [attachment];
let info = vk::RenderingInfo::default()
    .color_attachments(&attachments);
```

### Strings C en ash
Vulkan espera strings null-terminated (`*const c_char`). Rust strings no son null-terminated. Usar `CStr::from_bytes_with_nul(b"string\0")` o el macro `c"string"` (Rust 1.77+).

---

## Linux

### NVIDIA + Wayland + RenderDoc
RenderDoc **no soporta** `VK_KHR_wayland_surface`. Si necesitás capturar frames en una máquina NVIDIA con Wayland, forzar X11:

```bash
WINIT_UNIX_BACKEND=x11 cargo run
```

Alternativa: usar `NVIDIA NSight Graphics`, que sí soporta Wayland. Pero es más pesado.

### Permisos de gamepad
gilrs usa `/dev/input/event*` vía evdev. Si no detecta el gamepad, el usuario no tiene permisos.

```bash
sudo usermod -aG input $USER
# reloguear
```

### Mesa RADV: variables de entorno útiles
```bash
RADV_DEBUG=info          # info del driver al arrancar
RADV_PERFTEST=aco        # forzar ACO compiler (default, pero útil para toggle)
AMD_VULKAN_ICD=RADV      # forzar RADV si hay múltiples ICDs instalados
```

### Wayland: cursor lock
Para juegos FPS, necesitás capturar el mouse. En Wayland, esto requiere el protocolo `zwp_pointer_constraints_v1`. winit lo soporta pero algunos compositors no lo implementan completamente.

**Fallback:** Si `CursorGrabMode::Locked` falla, intentar `CursorGrabMode::Confined`.

### SPIR-V validation en CI
`spirv-val` (del Vulkan SDK) puede validar shaders compilados sin una GPU. Útil para CI en servidores sin GPU.

```bash
glslangValidator -V shader.vert -o shader.vert.spv
spirv-val shader.vert.spv
```

---

## macOS (MoltenVK)

### Features no disponibles
MoltenVK traduce Vulkan a Metal. Algunas extensiones simplemente no existen en Metal y nunca van a estar disponibles:

- **Ray tracing** (`VK_KHR_ray_tracing_pipeline`, `VK_KHR_acceleration_structure`): no soportado.
- **Geometry shaders** (`geometryShader`): no soportado.
- **Mesh shaders** (`VK_EXT_mesh_shader`): no soportado.
- **Descriptor indexing**: funciona parcialmente, pero los límites son mucho más bajos que en GPUs desktop. `maxPerStageDescriptorUpdateAfterBindSampledImages` puede ser tan bajo como 500K vs millones en NVIDIA/AMD.

### Vulkan SDK y libvulkan
MoltenVK viene incluido en el Vulkan SDK de LunarG. Después de instalar:

```bash
# El installer configura esto, pero verificar si ash no encuentra libvulkan:
export VULKAN_SDK=$HOME/VulkanSDK/<version>/macOS
export DYLD_LIBRARY_PATH=$VULKAN_SDK/lib:$DYLD_LIBRARY_PATH
export VK_ICD_FILENAMES=$VULKAN_SDK/share/vulkan/icd.d/MoltenVK_icd.json
export VK_LAYER_PATH=$VULKAN_SDK/share/vulkan/explicit_layer.d
```

Si `ash::Entry::load()` falla con "No se encontró libvulkan", lo primero es verificar que `DYLD_LIBRARY_PATH` incluya el directorio con `libvulkan.dylib`.

### Portability subset
MoltenVK requiere `VK_KHR_portability_enumeration` en la instancia y `VK_KHR_portability_subset` en el device. Sin esto, `enumerate_physical_devices()` devuelve 0 devices. El motor ya maneja esto automáticamente.

### Vulkan 1.3 no disponible
MoltenVK soporta Vulkan 1.2. Features de 1.3 como `dynamic_rendering` están disponibles como extensiones KHR (`VK_KHR_dynamic_rendering`), no como core. El motor detecta esto y usa la extensión cuando 1.3 no está disponible.

---

## Shaders GLSL

### Coordenadas Y invertidas
Vulkan tiene Y apuntando hacia abajo (opuesto a OpenGL). Si portás shaders de tutoriales OpenGL, todo aparece dado vuelta.

**Solución preferida:** Viewport con height negativo (requiere `VK_KHR_maintenance1`, core desde Vulkan 1.1):
```rust
let viewport = vk::Viewport {
    y: height as f32,
    height: -(height as f32),
    ..
};
```

### Normal matrix
La normal matrix es `transpose(inverse(mat3(model)))`. Calcularla en CPU y pasarla como uniform es tentador, pero truncar una `mat4` a `mat3` antes de invertir pierde precisión con escalas no uniformes.

**Mejor:** Pasar la model matrix al shader y calcular ahí. La GPU hace inversas rápido.

### Alignment de uniform buffers
GLSL `std140` layout tiene reglas de alignment que no coinciden con Rust:
- `vec3` se alinea a 16 bytes (igual que `vec4`)
- Un struct con un `vec3` seguido de un `float` tiene 16 bytes de padding en el vec3

**Solución:** Usar `vec4` en vez de `vec3` en uniform buffers. O rellenar con `_padding: f32` en el struct Rust.

### Specialization constants vs push constants
Para datos que cambian una vez (feature flags, calidad de sombras), usar specialization constants — se compilan dentro del shader, zero overhead en runtime. Push constants son para datos que cambian cada frame.

### perspective_rh vs perspective_rh_zo
`glam::Mat4::perspective_rh` genera profundidad NDC en `[-1, 1]` (convención OpenGL). Vulkan espera `[0, 1]`. Con la matriz incorrecta, objetos entre `near` y `~2*near*far/(near+far)` tienen `NDC_z < 0` y son clipeados.

**Regla:** Siempre usar `Mat4::perspective_rh_zo` en este motor. Con `near=0.1, far=100` el rango incorrecto afecta objetos a menos de ~0.2 unidades — difícil de notar hasta que se agrega depth buffer en Fase 8.

---

## Assets y paths en runtime

### Working directory de cargo run
El binario usa como CWD el directorio desde donde se invoca `cargo run`, **no** la raíz del proyecto. Si corrés `cargo run` desde `src/asset/`, las rutas relativas como `"assets/modelo.glb"` resuelven a `src/asset/assets/modelo.glb`.

**Regla:** Siempre correr `cargo run` desde la raíz del proyecto (`/repos/ralk`). Documentar esto en README.

### glTF: coordenadas de la jerarquía de nodos
El transform de un nodo en glTF es local a su padre. `node.transform().matrix()` solo da la transform local. Para la posición en world space hay que acumular todo el path desde la raíz multiplicando matrices padre → hijo.

**Solución implementada:** `collect_node` acumula `parent_transform * local` en recursión, igual que un scene graph.

### glTF: primitivas no-triangles
Un mesh glTF puede tener primitivas `LINES`, `POINTS`, etc. además de `TRIANGLES`. El loader ignora primitivas no-`TRIANGLES` silenciosamente (`return None`). Si un modelo aparece incompleto o vacío, verificar que sus primitivas sean triangle lists.

### Normal-as-color: visualización de debug
Antes de tener lighting (Fase 6), los normals remap `[-1,1] → [0,1]` por canal sirven para confirmar que geometría y normals llegaron correctos al shader. Cara apuntando a cámara (+Z normal) → azulada (0.5, 0.5, 1.0). Caras laterales → rojizas/verdosas. Si todo aparece gris uniforme, los normals están corruptos o zeroed.

---

## Fase 7: Texturas PBR

### linear: false para imágenes OPTIMAL
gpu-allocator requiere `linear: false` para imágenes con tiling `OPTIMAL`. `linear: true` es solo para buffers y para imágenes con `LINEAR` tiling (raro). Con `linear: true` en una imagen OPTIMAL el allocator puede asignar memoria en el heap equivocado y corromper datos.

### gltf::image::Format: no existen variantes BGR
El enum `gltf::image::Format` NO tiene variantes `B8G8R8` ni `B8G8R8A8`. Si usás `use gltf::image::Format::*` con esos nombres, Rust los trata como variables ligadoras (wildcard), no como variantes — match siempre entra por ahí. Usar `Format::R8G8B8` y `Format::R8G8B8A8` con calificación explícita.

### sRGB vs UNORM: no hacer doble conversión
Cuando una textura se sube como `R8G8B8A8_SRGB`, la GPU convierte sRGB → linear automáticamente al samplear. NO hacer `pow(color, 2.2)` en el shader — resulta en doble conversión y colores demasiado oscuros.

**Regla:** albedo = `R8G8B8A8_SRGB` (hardware convierte), normal map + MR = `R8G8B8A8_UNORM` (ya son datos lineales).

### Tangentes: glTF vec4 con handedness
El accessor `TANGENT` en glTF es `vec4`. `xyz` es la dirección tangente y `w` es el signo de handedness (+1 o -1) para calcular la bitangente: `B = cross(N, T) * w`. Nunca calcular `B = cross(T, N)` sin el signo — la textura se refleja en modelos con UVs simétricas.

### Descriptor sets: pool sizes exactas
El pool de material descriptors tiene `(num_materials + 1) * 3` descriptors de tipo `COMBINED_IMAGE_SAMPLER`. Si allocás más descriptors de los que declaraste en el pool, Vulkan retorna `VK_ERROR_OUT_OF_POOL_MEMORY`. El `+1` es el material default para meshes sin material.

### Sampler con max_lod=0.0 para texturas sin mipmaps
Con una sola mipmap level, poner `max_lod = 0.0`. Si dejás `max_lod = vk::LOD_CLAMP_NONE` (float::MAX), el driver puede intentar samplear niveles que no existen — undefined behavior en algunos drivers, warnings en validation layers.

---

## Fase 8: Depth buffer

### Sin depth buffer los back-faces sobreescriben los front-faces
Sin `VK_COMPARE_OP_LESS`, los triángulos se dibujan en orden del index buffer. Para un cubo con cara trasera dibujada después de la delantera, la cara trasera oscura (NdotL=0, solo ambient) pisa la cara iluminada. El resultado parece "ver solo el interior". No es un bug de normales — es un bug de ordering.

**Regla:** depth buffer siempre antes que cualquier escena 3D con geometría cerrada.

### Depth image: transicionar una vez en creación, no cada frame
La imagen de depth se crea en `UNDEFINED`. Transitionar a `DEPTH_STENCIL_ATTACHMENT_OPTIMAL` una sola vez con un one-shot command buffer. Después, `LOAD_OP_CLEAR` limpia el depth al inicio de cada frame sin necesitar barrera adicional — la imagen ya está en el layout correcto.

Si transicionás desde `UNDEFINED` cada frame, validation layers pueden quejarse de que el layout anterior no coincide con lo que el driver espera.

### Depth attachment en PipelineRenderingCreateInfo
Con dynamic rendering, el depth format hay que declararlo en `PipelineRenderingCreateInfo::depth_attachment_format` además de crear el `PipelineDepthStencilStateCreateInfo`. Si sólo ponés el depth state pero no el formato, la pipeline se crea sin depth — validation layers lo reportan pero el draw no falla silenciosamente en todos los drivers.

### Recrear depth buffer en swapchain resize
El depth image tiene dimensiones fijas al swapchain. Si el swapchain cambia de tamaño y el depth no se recrea, el depth testing falla (el depth image es más chico o más grande que el framebuffer). Destruir el handle viejo y crear uno nuevo con las nuevas dimensiones en `recreate_swapchain`.

---

## Fase 8: Shadow maps

### Layout inicial del shadow map: SHADER_READ_ONLY_OPTIMAL
El shadow map arranca en `SHADER_READ_ONLY_OPTIMAL` (no `UNDEFINED`). Así, la barrera en el primer frame tiene un `old_layout` definido: `SHADER_READ_ONLY → DEPTH_STENCIL_ATTACHMENT`. Si arrancara en `UNDEFINED`, el primer frame funcionaría igual, pero en frames siguientes el driver esperaría `SHADER_READ_ONLY` (el layout que dejamos al final del frame anterior) y habría mismatch.

**Regla:** Todo image que se alterna entre attachment y sampled siempre debe tener un `old_layout` consistente con el estado real.

### orthographic_rh en glam 0.29 ya produce [0,1] depth
En glam 0.29, `Mat4::orthographic_rh` produce profundidad NDC en `[0,1]` (convención Vulkan). `orthographic_rh_gl` produce `[-1,1]` (convención OpenGL). La variante `orthographic_rh_zo` no existe en esta versión — `rh` sin sufijo ya es la variante Vulkan. Consistent con `perspective_rh` vs `perspective_rh_gl`.

### Viewport del shadow pass: sin flip de Y
El viewport del shadow pass usa height positivo (sin el flip negativo que usa el main pass). Si flippeás Y en el shadow pass, las UVs calculadas en el fragment shader (`NDC.xy * 0.5 + 0.5`) no coinciden con lo que se renderizó y las sombras aparecen en las posiciones incorrectas.

**Regla:** El flip de viewport solo va en el main pass. El shadow pass usa viewport estándar (y=0, height=2048).

### sampler2DShadow + LESS_OR_EQUAL: 1.0 = lit, 0.0 = shadow
Con `compare_op = LESS_OR_EQUAL`, `texture(shadowMap, vec3(uv, ref))` retorna `1.0` cuando `ref <= texel_depth` (el fragmento está más cerca de la luz que lo que está en el shadow map → iluminado). Retorna `0.0` cuando `ref > texel_depth` (el fragmento está detrás de algo → en sombra).

Usar `BORDER_COLOR_FLOAT_OPAQUE_WHITE` con `CLAMP_TO_BORDER` para que las áreas fuera del frustum de la luz no queden en sombra (shadow=1.0 = iluminado).

### Self-shadowing (shadow acne): depth bias en pipeline + offset en shader
Para evitar acne, combinar dos mecanismos:
- **Pipeline depth bias**: `depth_bias_constant_factor = 4.0`, `depth_bias_slope_factor = 1.5`. Desplaza los valores de depth escritos en el shadow map.
- **Shader offset**: `shadowRef = shadowNdc.z - 0.002`. Pequeño delta en la comparación.

Usar solo uno de los dos introduce artefactos en algunos ángulos. Ajustar los valores si aparece acne o peter panning.

### Descriptor pool de lighting: incluir COMBINED_IMAGE_SAMPLER
El pool del set 0 (lighting) necesita descriptores de tipo `COMBINED_IMAGE_SAMPLER` además de `UNIFORM_BUFFER`, uno por frame-in-flight, para el shadow map en binding 1. Sin esto, `allocate_descriptor_sets` falla con `VK_ERROR_OUT_OF_POOL_MEMORY`.

### mutableComparisonSamplers: no disponible en MoltenVK (macOS)
`VkPhysicalDevicePortabilitySubsetFeaturesKHR::mutableComparisonSamplers` es `VK_FALSE` en MoltenVK. Crear un sampler con `compareEnable = VK_TRUE` genera un validation error en `vkUpdateDescriptorSets`.

**Solución:** Crear el shadow sampler sin `compare_enable`/`compare_op` (sampler regular con `NEAREST`). Cambiar el shader de `sampler2DShadow` + `texture(map, vec3(uv, ref))` a `sampler2D` + comparación manual:
```glsl
float storedDepth = texture(shadowMap, uv).r;
float lit = (shadowRef <= storedDepth) ? 1.0 : 0.0;
```
La semántica `LESS_OR_EQUAL` se preserva: `shadowRef <= storedDepth` → 1.0 (lit).