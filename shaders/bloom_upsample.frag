#version 450

// Bloom upsample pass.
// Reads from the smaller bloom level (u_src) and additively blends into the current level.
// Push constants: texel size (xy) + blend factor (z) + unused (w).

layout(set = 0, binding = 0) uniform sampler2D u_src;

layout(push_constant) uniform BloomPc {
    float texel_w;
    float texel_h;
    float blend;
    float _pad;
} pc;

layout(location = 0) in  vec2 i_uv;
layout(location = 0) out vec4 o_color;

void main() {
    vec2 ts = vec2(pc.texel_w, pc.texel_h);
    // 9-tap tent filter upsample
    vec4 c = vec4(0.0);
    c += texture(u_src, i_uv + ts * vec2(-1.0, -1.0)) * 0.0625;
    c += texture(u_src, i_uv + ts * vec2( 0.0, -1.0)) * 0.125;
    c += texture(u_src, i_uv + ts * vec2( 1.0, -1.0)) * 0.0625;
    c += texture(u_src, i_uv + ts * vec2(-1.0,  0.0)) * 0.125;
    c += texture(u_src, i_uv                         ) * 0.25;
    c += texture(u_src, i_uv + ts * vec2( 1.0,  0.0)) * 0.125;
    c += texture(u_src, i_uv + ts * vec2(-1.0,  1.0)) * 0.0625;
    c += texture(u_src, i_uv + ts * vec2( 0.0,  1.0)) * 0.125;
    c += texture(u_src, i_uv + ts * vec2( 1.0,  1.0)) * 0.0625;
    o_color = vec4(c.rgb * pc.blend, 1.0);
}
