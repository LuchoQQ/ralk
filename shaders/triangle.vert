#version 450

layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec3 inNormal;
layout(location = 2) in vec2 inTexCoord;
layout(location = 3) in vec4 inTangent;   // xyz = tangent direction, w = handedness (+1 / -1)

layout(location = 0) out vec3 fragWorldPos;
layout(location = 1) out vec2 fragTexCoord;
layout(location = 2) out vec3 fragT;      // tangent  (world space)
layout(location = 3) out vec3 fragB;      // bitangent (world space)
layout(location = 4) out vec3 fragN;      // normal   (world space)

layout(push_constant) uniform PC {
    mat4 mvp;    // bytes 0..64
    mat4 model;  // bytes 64..128
} pc;

void main() {
    fragWorldPos = vec3(pc.model * vec4(inPosition, 1.0));
    fragTexCoord = inTexCoord;

    // Normal matrix = transpose(inverse(mat3(model))) — correct for non-uniform scale.
    mat3 normalMat = transpose(inverse(mat3(pc.model)));

    vec3 N = normalize(normalMat * inNormal);
    vec3 T = normalize(normalMat * inTangent.xyz);
    // Gram-Schmidt re-orthogonalize to handle floating-point drift
    T = normalize(T - dot(T, N) * N);
    vec3 B = cross(N, T) * inTangent.w;   // w encodes handedness

    fragT = T;
    fragB = B;
    fragN = N;

    gl_Position = pc.mvp * vec4(inPosition, 1.0);
}
