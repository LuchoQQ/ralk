#version 450

// Composite pass: tone-maps HDR + blends bloom → LDR swapchain output.
// Also applies screen-space ambient occlusion from the SSAO pass.
//
// Push constants: bloom_intensity (x), tone_mode (y, 0=Reinhard 1=ACES),
//                 bloom_enabled (z), ssao_strength (w, 0=off 1=full).

layout(set = 0, binding = 0) uniform sampler2D u_hdr;
layout(set = 0, binding = 1) uniform sampler2D u_bloom;
layout(set = 0, binding = 2) uniform sampler2D u_ssao;

layout(push_constant) uniform CompositePc {
    float bloom_intensity;
    float tone_mode;      // 0 = Reinhard, 1 = ACES
    float bloom_enabled;
    float ssao_strength;  // 0 = off, 1 = full AO
} pc;

layout(location = 0) in  vec2 i_uv;
layout(location = 0) out vec4 o_color;

vec3 aces_tonemap(vec3 x) {
    const float a = 2.51, b = 0.03, c = 2.43, d = 0.59, e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), 0.0, 1.0);
}

void main() {
    vec3 hdr   = texture(u_hdr,   i_uv).rgb;
    vec3 bloom = texture(u_bloom, i_uv).rgb;
    float ao   = texture(u_ssao,  i_uv).r;

    vec3 color = hdr;
    if (pc.bloom_enabled > 0.5) {
        color += bloom * pc.bloom_intensity;
    }

    // Apply AO: lerp between full brightness and AO-darkened, controlled by ssao_strength.
    // SSAO only attenuates (darkens occluded areas), never brightens.
    float ao_factor = mix(1.0, ao, pc.ssao_strength);
    color *= ao_factor;

    // Tone mapping
    vec3 ldr;
    if (pc.tone_mode > 0.5) {
        ldr = aces_tonemap(color);
    } else {
        ldr = color / (color + vec3(1.0)); // Reinhard
    }

    // Gamma correction (linear → sRGB)
    ldr = pow(clamp(ldr, 0.0, 1.0), vec3(1.0 / 2.2));
    o_color = vec4(ldr, 1.0);
}
