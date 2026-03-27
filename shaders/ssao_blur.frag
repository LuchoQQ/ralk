#version 450

// SSAO blur pass: simple 3×3 box blur to smooth noise from the SSAO pass.
// No bilateral filtering — kept simple for the milestone.

layout(set = 0, binding = 0) uniform sampler2D u_ssao_raw;

layout(location = 0) in  vec2 i_uv;
layout(location = 0) out float o_ao;

void main() {
    vec2 texel = 1.0 / vec2(textureSize(u_ssao_raw, 0));
    float result = 0.0;
    for (int x = -1; x <= 1; x++) {
        for (int y = -1; y <= 1; y++) {
            vec2 offset = vec2(float(x), float(y)) * texel;
            result += texture(u_ssao_raw, i_uv + offset).r;
        }
    }
    o_ao = result / 9.0;
}
