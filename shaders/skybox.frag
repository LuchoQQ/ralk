#version 450

layout(location = 0) in  vec3 fragDir;
layout(location = 0) out vec4 outColor;

layout(set = 0, binding = 0) uniform samplerCube skyboxMap;

// Day/night tint pushed from CPU (offset 64, after the vertex-stage invViewProj mat4).
// rgb = color multiplier, a = overall brightness scale (0=night, 1=full day).
layout(push_constant) uniform PC {
    layout(offset = 64) vec4 skyTint;
} pc;

void main() {
    vec3 color = texture(skyboxMap, normalize(fragDir)).rgb;
    // Apply day/night tint: modulate color and scale brightness.
    color = color * pc.skyTint.rgb * pc.skyTint.a;
    // Reinhard tone mapping — same operator as the main PBR pass.
    color    = color / (color + vec3(1.0));
    outColor = vec4(color, 1.0);
}
