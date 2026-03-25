#version 450

layout(location = 0) in vec3 inPosition;
// locations 1-3 (normal, texCoord, tangent) are bound but unused in the shadow pass

layout(push_constant) uniform PC {
    mat4 lightMVP;
} pc;

void main() {
    gl_Position = pc.lightMVP * vec4(inPosition, 1.0);
}
