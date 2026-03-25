#version 450

layout(location = 0) in vec3 fragWorldPos;
layout(location = 1) in vec2 fragTexCoord;
layout(location = 2) in vec3 fragT;
layout(location = 3) in vec3 fragB;
layout(location = 4) in vec3 fragN;

layout(location = 0) out vec4 outColor;

// Set 0: lighting UBO + shadow map
layout(set = 0, binding = 0) uniform LightingUBO {
    vec4 dirLightDir;      // xyz = toward-light direction (normalized), w unused
    vec4 dirLightColor;    // xyz = color, w = intensity
    vec4 pointLightPos;    // xyz = world position, w unused
    vec4 pointLightColor;  // xyz = color, w = intensity
    vec4 cameraPos;        // xyz = world position, w unused
    mat4 lightMvp;         // light view-projection (applied to world-space positions)
} lights;

layout(set = 0, binding = 1) uniform sampler2D shadowMap;

// Set 1: material textures
layout(set = 1, binding = 0) uniform sampler2D albedoMap;           // R8G8B8A8_SRGB
layout(set = 1, binding = 1) uniform sampler2D normalMap;           // R8G8B8A8_UNORM
layout(set = 1, binding = 2) uniform sampler2D metallicRoughnessMap; // R8G8B8A8_UNORM, G=roughness, B=metallic

const float PI = 3.14159265359;

// GGX/Trowbridge-Reitz normal distribution
float D_GGX(float NdotH, float roughness) {
    float a  = roughness * roughness;
    float a2 = a * a;
    float d  = NdotH * NdotH * (a2 - 1.0) + 1.0;
    return a2 / max(PI * d * d, 1e-7);
}

// Smith + Schlick-GGX geometry term
float G_SchlickGGX(float NdotX, float roughness) {
    float r = roughness + 1.0;
    float k = r * r / 8.0;
    return NdotX / max(NdotX * (1.0 - k) + k, 1e-7);
}

float G_Smith(float NdotV, float NdotL, float roughness) {
    return G_SchlickGGX(NdotV, roughness) * G_SchlickGGX(NdotL, roughness);
}

// Schlick Fresnel approximation
vec3 F_Schlick(float cosTheta, vec3 F0) {
    return F0 + (1.0 - F0) * pow(clamp(1.0 - cosTheta, 0.0, 1.0), 5.0);
}

// Cook-Torrance BRDF contribution for a single light.
vec3 cookTorrance(vec3 N, vec3 V, vec3 L, vec3 lightColor, float intensity,
                  vec3 albedo, float metallic, float roughness) {
    float NdotL = max(dot(N, L), 0.0);
    if (NdotL <= 0.0) return vec3(0.0);

    vec3  H     = normalize(V + L);
    float NdotV = max(dot(N, V), 1e-4);
    float NdotH = max(dot(N, H), 0.0);
    float HdotV = max(dot(H, V), 0.0);

    vec3 F0 = mix(vec3(0.04), albedo, metallic);

    float D = D_GGX(NdotH, roughness);
    float G = G_Smith(NdotV, NdotL, roughness);
    vec3  F = F_Schlick(HdotV, F0);

    vec3 specular = (D * G * F) / max(4.0 * NdotV * NdotL, 1e-7);
    vec3 kD = (vec3(1.0) - F) * (1.0 - metallic);

    return (kD * albedo / PI + specular) * NdotL * intensity * lightColor;
}

void main() {
    // Albedo — GPU converts sRGB→linear automatically when format is R8G8B8A8_SRGB
    vec3 albedo = texture(albedoMap, fragTexCoord).rgb;

    // Metallic-roughness — glTF spec: G=roughness, B=metallic
    vec4  mrSample  = texture(metallicRoughnessMap, fragTexCoord);
    float roughness = max(mrSample.g, 0.04);  // clamp to avoid pure mirror
    float metallic  = mrSample.b;

    // Normal mapping: [0,1] → [-1,1], then transform to world space via TBN
    vec3 normalSample = texture(normalMap, fragTexCoord).rgb * 2.0 - 1.0;
    mat3 TBN = mat3(normalize(fragT), normalize(fragB), normalize(fragN));
    vec3 N = normalize(TBN * normalSample);

    vec3 V = normalize(lights.cameraPos.xyz - fragWorldPos);

    // Ambient (constant approximation — no IBL in Milestone 1)
    vec3 F0  = mix(vec3(0.04), albedo, metallic);
    vec3 kS  = F_Schlick(max(dot(N, V), 0.0), F0);
    vec3 kD  = (1.0 - kS) * (1.0 - metallic);
    vec3 color = kD * albedo * 0.03;

    // Shadow: project world position into light clip space (orthographic).
    // lightMvp is view-projection only; fragWorldPos is already in world space.
    vec4  shadowClip = lights.lightMvp * vec4(fragWorldPos, 1.0);
    vec3  shadowNdc  = shadowClip.xyz / shadowClip.w; // xy in [-1,1], z in [0,1]
    vec2  shadowUV   = shadowNdc.xy * 0.5 + 0.5;
    float shadowRef  = shadowNdc.z - 0.002;           // small bias to reduce acne

    // 3×3 PCF: average 9 samples with manual depth comparison (LESS_OR_EQUAL).
    // MoltenVK does not support mutableComparisonSamplers, so we use sampler2D
    // and compare manually: shadowRef <= storedDepth → 1.0 (lit), else 0.0 (shadow).
    // Areas outside the shadow frustum (UV outside [0,1]) remain fully lit.
    float shadow = 1.0;
    if (shadowUV.x >= 0.0 && shadowUV.x <= 1.0 &&
        shadowUV.y >= 0.0 && shadowUV.y <= 1.0 &&
        shadowRef  >= 0.0 && shadowRef  <= 1.0) {
        vec2 texelSize = 1.0 / vec2(2048.0);
        shadow = 0.0;
        for (int sx = -1; sx <= 1; sx++) {
            for (int sy = -1; sy <= 1; sy++) {
                float storedDepth = texture(shadowMap, shadowUV + vec2(sx, sy) * texelSize).r;
                shadow += (shadowRef <= storedDepth) ? 1.0 : 0.0;
            }
        }
        shadow /= 9.0;
    }

    // Directional light (sun-like, infinite distance) — attenuated by shadow
    vec3 L_dir = normalize(-lights.dirLightDir.xyz);
    color += shadow * cookTorrance(N, V, L_dir,
                                   lights.dirLightColor.xyz, lights.dirLightColor.w,
                                   albedo, metallic, roughness);

    // Point light with quadratic attenuation
    vec3  toLight     = lights.pointLightPos.xyz - fragWorldPos;
    float dist        = length(toLight);
    float attenuation = 1.0 / (1.0 + 0.35 * dist + 0.44 * dist * dist);
    vec3  L_pt        = toLight / dist;
    color += attenuation * cookTorrance(N, V, L_pt,
                                        lights.pointLightColor.xyz, lights.pointLightColor.w,
                                        albedo, metallic, roughness);

    // Reinhard tone mapping
    color    = color / (color + vec3(1.0));
    outColor = vec4(color, 1.0);
}
