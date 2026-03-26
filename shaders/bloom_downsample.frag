#version 450

// Bloom downsample pass.
// Reads from the source (HDR color or previous bloom level) and writes a half-res filtered result.
// Push constants: texel size (xy) + threshold (z) + unused (w).

layout(set = 0, binding = 0) uniform sampler2D u_src;

layout(push_constant) uniform BloomPc {
    float texel_w;
    float texel_h;
    float threshold;
    float _pad;
} pc;

layout(location = 0) in  vec2 i_uv;
layout(location = 0) out vec4 o_color;

void main() {
    vec2 ts = vec2(pc.texel_w, pc.texel_h);
    // 13-tap Kawase-style downsample
    vec4 c  = texture(u_src, i_uv) * 4.0;
    c += texture(u_src, i_uv + ts * vec2(-1.0, -1.0));
    c += texture(u_src, i_uv + ts * vec2( 1.0, -1.0));
    c += texture(u_src, i_uv + ts * vec2(-1.0,  1.0));
    c += texture(u_src, i_uv + ts * vec2( 1.0,  1.0));
    c += texture(u_src, i_uv + ts * vec2(-2.0,  0.0));
    c += texture(u_src, i_uv + ts * vec2( 2.0,  0.0));
    c += texture(u_src, i_uv + ts * vec2( 0.0, -2.0));
    c += texture(u_src, i_uv + ts * vec2( 0.0,  2.0));
    c /= 13.0;

    // Luminance threshold
    float luma = dot(c.rgb, vec3(0.2126, 0.7152, 0.0722));
    float contrib = max(0.0, luma - pc.threshold) / max(luma, 0.0001);
    o_color = vec4(c.rgb * contrib, 1.0);
}
