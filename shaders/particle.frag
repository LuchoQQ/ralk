#version 450

layout(location = 0) in vec4 fragColor;
layout(location = 1) in vec2 fragTexCoord;

layout(location = 0) out vec4 outColor;

void main() {
    // Map UV [0,1]^2 → [-1,1]^2 and compute radial distance for a circle.
    vec2 uv   = fragTexCoord * 2.0 - 1.0;
    float d2  = dot(uv, uv);
    if (d2 > 1.0) discard;

    // Gaussian falloff: bright at center, fades to zero at edge.
    float alpha = exp(-d2 * 3.0) * fragColor.a;

    // Additive blend: pre-multiply color by alpha so the blend equation
    // (src + dst) gives the correct result without double-counting.
    outColor = vec4(fragColor.rgb * alpha, alpha);
}
