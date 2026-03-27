#version 450

layout(location = 0) in vec3 fragWorldPos;
layout(location = 1) in vec2 fragTexCoord;
layout(location = 2) in vec3 fragT;
layout(location = 3) in vec3 fragB;
layout(location = 4) in vec3 fragN;
layout(location = 5) flat in uint instanceIndex; // Fase 44: from vertex shader

layout(location = 0) out vec4 outColor;

// Set 0: lighting UBO, shadow map, IBL maps
layout(set = 0, binding = 0) uniform LightingUBO {
    vec4 dirLightDir;      // xyz = toward-light direction (normalized), w = ibl_scale
    vec4 dirLightColor;    // xyz = color, w = intensity
    vec4 pointLightPos;    // xyz = world position, w unused
    vec4 pointLightColor;  // xyz = color, w = intensity
    vec4 cameraPos;        // xyz = world position, w = tone mode (0=Reinhard, 1=ACES)
    mat4 lightMvp;         // light view-projection (applied to world-space positions)
} lights;

layout(set = 0, binding = 1) uniform sampler2D   shadowMap;
layout(set = 0, binding = 2) uniform samplerCube irradianceMap;   // diffuse IBL
layout(set = 0, binding = 3) uniform samplerCube prefilteredMap;  // specular IBL (mipped)
layout(set = 0, binding = 4) uniform sampler2D   brdfLut;         // split-sum BRDF LUT

// Fase 44: per-instance material override data (must match InstanceData in triangle.vert).
struct InstanceData {
    mat4 model;
    vec4 world_min;
    vec4 world_max;
    uint mesh_index;
    uint override_flags;  // bit 0=color, 1=metallic, 2=roughness, 3=emissive, 4=uv_scale
    uint _pad1;
    uint _pad2;
    vec4 override_color;    // rgb = albedo override
    vec4 override_mr;       // x=metallic, y=roughness
    vec4 override_emissive; // xyz=emissive color, w=intensity
};

layout(set = 0, binding = 5) readonly buffer InstanceBuffer {
    InstanceData instances[];
};

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
    // Read per-instance override data (Fase 44)
    uint flags = instances[instanceIndex].override_flags;

    // Albedo — GPU converts sRGB→linear automatically when format is R8G8B8A8_SRGB
    vec3 albedo = texture(albedoMap, fragTexCoord).rgb;
    if ((flags & 1u) != 0u) {
        albedo = instances[instanceIndex].override_color.rgb;
    }

    // Metallic-roughness — glTF spec: G=roughness, B=metallic
    vec4  mrSample  = texture(metallicRoughnessMap, fragTexCoord);
    float roughness = max(mrSample.g, 0.04);  // clamp to avoid pure mirror
    float metallic  = mrSample.b;
    if ((flags & 2u) != 0u) metallic  = instances[instanceIndex].override_mr.x;
    if ((flags & 4u) != 0u) roughness = max(instances[instanceIndex].override_mr.y, 0.04);

    // Normal mapping: [0,1] → [-1,1], then transform to world space via TBN
    vec3 normalSample = texture(normalMap, fragTexCoord).rgb * 2.0 - 1.0;
    mat3 TBN = mat3(normalize(fragT), normalize(fragB), normalize(fragN));
    vec3 N = normalize(TBN * normalSample);

    vec3 V = normalize(lights.cameraPos.xyz - fragWorldPos);

    // IBL ambient — split-sum approximation (diffuse irradiance + specular prefiltered env).
    vec3  F0     = mix(vec3(0.04), albedo, metallic);
    float NdotV  = max(dot(N, V), 0.0);
    vec3  F_amb  = F_Schlick(NdotV, F0);
    vec3  kD_ibl = (1.0 - F_amb) * (1.0 - metallic);

    // Diffuse: irradiance from preconvolved env (stores mean radiance over cosine hemisphere).
    vec3 irradiance  = texture(irradianceMap, N).rgb;
    vec3 diffuse_ibl = kD_ibl * albedo * irradiance;

    // Specular: prefiltered env (mip = roughness * max_mip) + BRDF scale/bias from LUT.
    vec3  R               = reflect(-V, N);
    const float MAX_MIP   = float(4); // PREFILTERED_MIP_LEVELS - 1 = 5 - 1
    vec3  prefColor       = textureLod(prefilteredMap, R, roughness * MAX_MIP).rgb;
    vec2  brdf            = texture(brdfLut, vec2(NdotV, roughness)).rg;
    vec3  specular_ibl    = prefColor * (F_amb * brdf.x + brdf.y);

    // ibl_scale is packed into dirLightDir.w (0 = no IBL, 1 = full IBL).
    float ibl_scale = lights.dirLightDir.w;
    vec3 color = (diffuse_ibl + specular_ibl) * ibl_scale;

    // Shadow: project world position into light clip space (orthographic).
    vec4  shadowClip = lights.lightMvp * vec4(fragWorldPos, 1.0);
    vec3  shadowNdc  = shadowClip.xyz / shadowClip.w;
    vec2  shadowUV   = shadowNdc.xy * 0.5 + 0.5;
    float shadowRef  = shadowNdc.z - 0.002;

    // 3×3 PCF: average 9 samples with manual depth comparison (LESS_OR_EQUAL).
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

    // Fase 44: emissive contribution
    if ((flags & 8u) != 0u) {
        vec4 em = instances[instanceIndex].override_emissive;
        color += em.xyz * em.w;
    }

    // Tone mapping: Reinhard (mode 0) or ACES Filmic (mode 1).
    if (lights.cameraPos.w >= 0.5) {
        // ACES Filmic (Hill approximation)
        const float a = 2.51, b = 0.03, c = 2.43, d = 0.59, e = 0.14;
        color = clamp((color * (a * color + b)) / (color * (c * color + d) + e), 0.0, 1.0);
    } else {
        // Reinhard
        color = color / (color + vec3(1.0));
    }
    outColor = vec4(color, 1.0);
}
