#version 450

// SSAO pass: reconstruct view-space normals from depth gradients,
// sample a hemispherical kernel, accumulate occlusion.
//
// Set 0:
//   binding 0 = depth buffer (sampler2D, R = NDC depth in [0,1])
//   binding 1 = noise texture (4×4 R8G8_UNORM tiled, RG = random XY in [-1,1])
//   binding 2 = SsaoUbo (std140)

layout(set = 0, binding = 0) uniform sampler2D u_depth;
layout(set = 0, binding = 1) uniform sampler2D u_noise;

layout(set = 0, binding = 2, std140) uniform SsaoUbo {
    mat4  proj;           // perspective projection (no Y-flip baked in)
    mat4  inv_proj;       // inverse of proj
    vec4  kernel[32];     // hemisphere samples in tangent space (xyz used, w unused)
    float radius;         // hemisphere radius in view space
    float bias;           // depth bias to avoid self-occlusion
    float power;          // contrast exponent
    float sample_count;   // number of samples to use (float for convenience)
    vec2  noise_scale;    // screen_size / 4.0  — tiles the 4×4 noise texture
    vec2  _pad;
} u;

layout(location = 0) in  vec2 i_uv;
layout(location = 0) out float o_ao;

// Reconstruct view-space position from a screen UV and its depth value.
// Accounts for the Y-flip viewport used in the main pass:
//   UV (u, 0) = screen top = NDC Y=+1  →  ndc_y = +(1 - 2*uv.y)
vec3 reconstruct_position(vec2 uv, float depth) {
    vec4 clip = vec4(uv.x * 2.0 - 1.0,
                     -(uv.y * 2.0 - 1.0),   // Y-flip
                     depth,
                     1.0);
    vec4 view = u.inv_proj * clip;
    return view.xyz / view.w;
}

// View-space normal from cross product of depth-gradient vectors.
// Uses one-sided differences; may artifact at depth discontinuities.
vec3 reconstruct_normal(vec2 uv) {
    vec2 texel = 1.0 / vec2(textureSize(u_depth, 0));
    float d   = texture(u_depth, uv).r;
    float dx  = texture(u_depth, uv + vec2(texel.x, 0.0)).r;
    float dy  = texture(u_depth, uv + vec2(0.0, texel.y)).r;

    vec3 p  = reconstruct_position(uv,                         d);
    vec3 px = reconstruct_position(uv + vec2(texel.x, 0.0),   dx);
    vec3 py = reconstruct_position(uv + vec2(0.0, texel.y),   dy);

    // dX = right screen (+X view), dY = down screen (−Y view with Y-flip).
    // cross(dY, dX) gives a vector pointing toward the camera (+Z view space).
    vec3 dX = px - p;
    vec3 dY = py - p;
    return normalize(cross(dY, dX));
}

void main() {
    float depth = texture(u_depth, i_uv).r;
    if (depth >= 1.0) {
        // Sky / far plane — no occlusion.
        o_ao = 1.0;
        return;
    }

    vec3 frag_pos = reconstruct_position(i_uv, depth);
    vec3 normal   = reconstruct_normal(i_uv);

    // Random rotation vector from tiled noise texture.
    vec2 rand_rg  = texture(u_noise, i_uv * u.noise_scale).rg * 2.0 - 1.0;
    vec3 rand_vec = vec3(rand_rg, 0.0);

    // Build TBN to orient kernel hemisphere along the surface normal.
    vec3 tangent   = normalize(rand_vec - normal * dot(rand_vec, normal));
    vec3 bitangent = cross(normal, tangent);
    mat3 tbn       = mat3(tangent, bitangent, normal);

    int   n  = int(u.sample_count);
    float ao = 0.0;
    for (int i = 0; i < n; i++) {
        // Sample in view space.
        vec3 samp = tbn * u.kernel[i].xyz;
        samp = frag_pos + samp * u.radius;

        // Project sample to screen UV.
        vec4 offset = u.proj * vec4(samp, 1.0);
        offset.xyz /= offset.w;
        // NDC → UV (with Y-flip):  uv_y = 0.5 - ndc_y * 0.5
        vec2 sample_uv = vec2(offset.x * 0.5 + 0.5,
                              -offset.y * 0.5 + 0.5);

        // Clamp to valid range.
        if (any(lessThan(sample_uv, vec2(0.0))) ||
            any(greaterThan(sample_uv, vec2(1.0)))) {
            continue;
        }

        float sample_depth = texture(u_depth, sample_uv).r;
        vec3  sample_pos   = reconstruct_position(sample_uv, sample_depth);

        // Range check: suppress occlusion from surfaces far away from the fragment.
        float range_check = smoothstep(0.0, 1.0,
            u.radius / (abs(frag_pos.z - sample_pos.z) + 0.001));

        // In right-handed view space, z is negative toward the scene.
        // If sample_pos.z >= samp.z + bias, the real surface is "closer to camera"
        // (higher z, less negative) than our sample → sample is behind geometry → occluded.
        ao += (sample_pos.z >= samp.z + u.bias ? 1.0 : 0.0) * range_check;
    }

    ao /= float(n);
    o_ao = pow(1.0 - ao, u.power);
}
