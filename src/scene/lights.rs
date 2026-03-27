use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};

/// GPU layout for the lighting uniform buffer (std140).
/// All vec3s padded to vec4 (16 bytes). Mat4 follows at offset 80.
/// Extended in Fase 23 to add view_proj and frustum_planes for GPU-driven rendering.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct LightingUbo {
    pub dir_light_dir:    [f32; 4],    // offset   0 — xyz = light direction, w unused
    pub dir_light_color:  [f32; 4],    // offset  16 — xyz = color, w = intensity
    pub point_light_pos:  [f32; 4],    // offset  32 — xyz = world position, w unused
    pub point_light_color:[f32; 4],    // offset  48 — xyz = color, w = intensity
    pub camera_pos:       [f32; 4],    // offset  64 — xyz = world position, w = tone mode
    pub light_mvp:        [f32; 16],   // offset  80 — orthographic shadow view-projection
    pub view_proj:        [f32; 16],   // offset 144 — camera view-projection (for vertex shader + compute)
    pub frustum_planes:   [[f32; 4]; 6], // offset 208 — world-space frustum planes (a,b,c,d) × 6
}                                      // total: 208 + 96 = 304 bytes

/// Orthographic shadow camera for a directional light.
/// `direction` points FROM the light TOWARD the scene (same convention as DirectionalLight).
/// Centers on the world origin with a ±5-unit box, depth 0.1..30.
pub fn compute_light_mvp(direction: Vec3) -> Mat4 {
    let shine = direction.normalize();
    let light_pos = -shine * 12.0;
    let up = if shine.abs().dot(Vec3::Y) > 0.99 { Vec3::Z } else { Vec3::Y };
    let view = Mat4::look_at_rh(light_pos, Vec3::ZERO, up);
    // glam 0.29: orthographic_rh produces [0,1] depth (Vulkan convention).
    let proj = Mat4::orthographic_rh(-5.0, 5.0, -5.0, 5.0, 0.1, 30.0);
    proj * view
}
