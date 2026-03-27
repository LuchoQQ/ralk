#version 450

// Per-vertex particle data: position (world-space corner of billboard quad),
// RGBA color, and UV for the circular falloff.
layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec4 inColor;
layout(location = 2) in vec2 inTexCoord;

layout(location = 0) out vec4 fragColor;
layout(location = 1) out vec2 fragTexCoord;

// Push constants: view-projection matrix (64 bytes).
layout(push_constant) uniform PushConstants {
    mat4 view_proj;
} pc;

void main() {
    fragColor    = inColor;
    fragTexCoord = inTexCoord;
    gl_Position  = pc.view_proj * vec4(inPosition, 1.0);
}
