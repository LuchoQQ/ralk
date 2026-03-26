#version 450

// Fullscreen triangle from gl_VertexIndex (no vertex buffer needed).
// Vertices at NDC: (-1,-1), (3,-1), (-1, 3) cover the whole screen.
layout(location = 0) out vec2 o_uv;

void main() {
    vec2 pos = vec2((gl_VertexIndex & 1) * 4.0 - 1.0,
                    (gl_VertexIndex & 2) * 2.0 - 1.0);
    o_uv = pos * 0.5 + 0.5;
    gl_Position = vec4(pos, 0.0, 1.0);
}
