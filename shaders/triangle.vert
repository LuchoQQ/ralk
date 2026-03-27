#version 460

layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec3 inNormal;
layout(location = 2) in vec2 inTexCoord;
layout(location = 3) in vec4 inTangent;   // xyz = tangent direction, w = handedness (+1 / -1)

layout(location = 0) out vec3 fragWorldPos;
layout(location = 1) out vec2 fragTexCoord;
layout(location = 2) out vec3 fragT;      // tangent  (world space)
layout(location = 3) out vec3 fragB;      // bitangent (world space)
layout(location = 4) out vec3 fragN;      // normal   (world space)

// Fase 23: model matrix comes from the instance SSBO (gl_BaseInstance).
// view_proj is read from LightingUbo so vertex and compute share one source of truth.

// Must match LightingUbo in Rust (std140, 304 bytes).
layout(set = 0, binding = 0) uniform LightingUbo {
    vec4 dir_light_dir;
    vec4 dir_light_color;
    vec4 point_light_pos;
    vec4 point_light_color;
    vec4 camera_pos;
    mat4 light_mvp;
    mat4 view_proj;
    vec4 frustum_planes[6];
} ubo;

// Per-instance data — must match InstanceData in Rust (std430, 112 bytes).
struct InstanceData {
    mat4 model;
    vec4 world_min;
    vec4 world_max;
    uint mesh_index;
    uint _pad0;
    uint _pad1;
    uint _pad2;
};

layout(set = 0, binding = 5) readonly buffer InstanceBuffer {
    InstanceData instances[];
};

void main() {
    mat4 model = instances[gl_BaseInstance].model;

    fragWorldPos = vec3(model * vec4(inPosition, 1.0));
    fragTexCoord = inTexCoord;

    // Normal matrix = transpose(inverse(mat3(model))) — correct for non-uniform scale.
    mat3 normalMat = transpose(inverse(mat3(model)));

    vec3 N = normalize(normalMat * inNormal);
    vec3 T = normalize(normalMat * inTangent.xyz);
    // Gram-Schmidt re-orthogonalize to handle floating-point drift
    T = normalize(T - dot(T, N) * N);
    vec3 B = cross(N, T) * inTangent.w;   // w encodes handedness

    fragT = T;
    fragB = B;
    fragN = N;

    gl_Position = ubo.view_proj * model * vec4(inPosition, 1.0);
}
