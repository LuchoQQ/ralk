#version 450

layout(push_constant) uniform PC {
    mat4 invViewProj;
} pc;

layout(location = 0) out vec3 fragDir;

// Fullscreen triangle: 3 vertices covering NDC [-1,-1] to [+1,+1].
// No vertex buffer needed — positions are generated from gl_VertexIndex.
void main() {
    const vec2 positions[3] = vec2[3](
        vec2(-1.0, -1.0),
        vec2( 3.0, -1.0),
        vec2(-1.0,  3.0)
    );
    vec2 ndc = positions[gl_VertexIndex];

    // Depth = 1.0 (max) so skybox is occluded by any geometry (depth < 1.0).
    // LESS_OR_EQUAL in the skybox pipeline passes where depth buffer == 1.0 (background).
    gl_Position = vec4(ndc, 1.0, 1.0);

    // Reconstruct world-space view direction by unprojecting near/far plane points.
    // Uses Vulkan depth convention [0, 1]: z=0 → near, z=1 → far.
    vec4 nearH = pc.invViewProj * vec4(ndc, 0.0, 1.0);
    vec4 farH  = pc.invViewProj * vec4(ndc, 1.0, 1.0);
    vec3 nearW = nearH.xyz / nearH.w;
    vec3 farW  = farH.xyz  / farH.w;
    fragDir = farW - nearW; // magnitude doesn't matter; fragment normalizes it
}
