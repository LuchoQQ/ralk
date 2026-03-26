#version 450

layout(location = 0) in  vec3 fragDir;
layout(location = 0) out vec4 outColor;

layout(set = 0, binding = 0) uniform samplerCube skyboxMap;

void main() {
    vec3 color = texture(skyboxMap, normalize(fragDir)).rgb;
    // Reinhard tone mapping — same operator as the main PBR pass.
    color    = color / (color + vec3(1.0));
    outColor = vec4(color, 1.0);
}
