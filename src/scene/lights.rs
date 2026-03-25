use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4};

pub struct DirectionalLight {
    /// Direction light is shining (FROM light TOWARD scene, normalized).
    /// In the shader: L_dir = normalize(-dirLightDir).
    pub direction: Vec3,
    pub color: Vec3,
    pub intensity: f32,
}

pub struct PointLight {
    pub position: Vec3,
    pub color: Vec3,
    pub intensity: f32,
}

pub struct LightingState {
    pub directional: DirectionalLight,
    pub point: PointLight,
}

impl Default for LightingState {
    fn default() -> Self {
        Self {
            directional: DirectionalLight {
                direction: Vec3::new(0.4, -1.0, -0.6).normalize(),
                color: Vec3::ONE,
                intensity: 1.2,
            },
            point: PointLight {
                position: Vec3::new(2.0, 2.0, 2.0),
                color: Vec3::new(1.0, 0.9, 0.7),
                intensity: 3.0,
            },
        }
    }
}

/// GPU layout for the lighting uniform buffer (std140).
/// All vec3s padded to vec4 (16 bytes). Mat4 follows at offset 80.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct LightingUbo {
    pub dir_light_dir:    [f32; 4], // xyz = light-to-scene direction, w unused
    pub dir_light_color:  [f32; 4], // xyz = color, w = intensity
    pub point_light_pos:  [f32; 4], // xyz = world position, w unused
    pub point_light_color:[f32; 4], // xyz = color, w = intensity
    pub camera_pos:       [f32; 4], // xyz = world position, w unused
    pub light_mvp:        [f32; 16], // orthographic light view-projection (column-major)
}                                    // total: 80 + 64 = 144 bytes

impl LightingUbo {
    pub fn from_state(lights: &LightingState, camera_pos: Vec3) -> Self {
        Self {
            dir_light_dir:     lights.directional.direction.extend(0.0).into(),
            dir_light_color:   Vec4::from((lights.directional.color, lights.directional.intensity)).into(),
            point_light_pos:   lights.point.position.extend(0.0).into(),
            point_light_color: Vec4::from((lights.point.color, lights.point.intensity)).into(),
            camera_pos:        camera_pos.extend(0.0).into(),
            light_mvp:         compute_light_mvp(&lights.directional).to_cols_array(),
        }
    }
}

/// Orthographic shadow camera for a directional light.
/// Centers on the world origin with a ±5-unit box, depth 0.1..30.
/// For larger scenes (Sponza) increase the ortho bounds accordingly.
pub fn compute_light_mvp(light: &DirectionalLight) -> Mat4 {
    // Step back along the shine direction to place the shadow camera.
    let shine = light.direction.normalize();
    let light_pos = -shine * 12.0;

    // Stable up vector: avoid gimbal lock when light is nearly vertical.
    let up = if shine.abs().dot(Vec3::Y) > 0.99 { Vec3::Z } else { Vec3::Y };

    let view = Mat4::look_at_rh(light_pos, Vec3::ZERO, up);
    // Vulkan [0,1] depth range — use _zo variant.
    // glam 0.29: orthographic_rh produces [0,1] depth (Vulkan convention).
    let proj = Mat4::orthographic_rh(-5.0, 5.0, -5.0, 5.0, 0.1, 30.0);
    proj * view
}
