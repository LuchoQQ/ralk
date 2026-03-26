#version 450

layout(location = 0) in vec3 inPosition;

layout(push_constant) uniform PushConstants {
    mat4 view_proj;
    vec4 color;
} pc;

layout(location = 0) out vec4 fragColor;

void main() {
    gl_Position = pc.view_proj * vec4(inPosition, 1.0);
    fragColor = pc.color;
}
