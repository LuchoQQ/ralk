use glam::{Mat4, Vec3, Vec4};

use crate::input::InputState;

/// Right-handed perspective matrix with depth mapped to [0, 1] (Vulkan/Metal/DX12).
/// glam 0.29 only ships perspective_rh which maps to [-1, 1] (OpenGL convention).
/// See gotchas.md — using the wrong convention clips geometry near the camera.
fn perspective_rh_zo(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fov_y * 0.5).tan();
    // Column-major. Maps z_eye=-near → NDC_z=0, z_eye=-far → NDC_z=1.
    Mat4::from_cols(
        Vec4::new(f / aspect, 0.0, 0.0, 0.0),
        Vec4::new(0.0, f, 0.0, 0.0),
        Vec4::new(0.0, 0.0, far / (near - far), -1.0),
        Vec4::new(0.0, 0.0, near * far / (near - far), 0.0),
    )
}

pub struct Camera3D {
    pub position: Vec3,
    /// Horizontal rotation, radians. 0 = facing -Z.
    pub yaw: f32,
    /// Vertical rotation, radians. Positive = looking down. Clamped to ±89°.
    pub pitch: f32,

    pub fov_y: f32,
    pub aspect: f32,
    pub near: f32,
    pub far: f32,

    pub move_speed: f32,
    pub mouse_sensitivity: f32,
}

/// Eye offset above the player capsule center (capsule half_height=0.5, radius=0.4 → center at y=0.9 on ground).
/// Camera at 0.9 + 0.7 = 1.6 m eye height.
pub const EYE_OFFSET: f32 = 0.7;
/// Y of the capsule center when standing on the ground (ground top = y=0).
pub const PLAYER_SPAWN_Y: f32 = 0.9;

impl Camera3D {
    pub fn new(aspect: f32) -> Self {
        Self {
            position: Vec3::new(0.0, PLAYER_SPAWN_Y + EYE_OFFSET, 5.0),
            yaw: 0.0,
            pitch: 0.0,
            fov_y: 60.0f32.to_radians(),
            aspect,
            near: 0.1,
            far: 200.0,
            move_speed: 5.0,
            mouse_sensitivity: 0.002,
        }
    }

    /// Unit vector pointing in the camera's look direction (right-handed, -Z forward at yaw=0).
    pub fn forward(&self) -> Vec3 {
        Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            -self.pitch.sin(),
            -self.yaw.cos() * self.pitch.cos(),
        )
    }

    /// Unit vector pointing right relative to the camera.
    pub fn right(&self) -> Vec3 {
        self.forward().cross(Vec3::Y).normalize()
    }

    pub fn view(&self) -> Mat4 {
        Mat4::look_to_rh(self.position, self.forward(), Vec3::Y)
    }

    /// Right-handed perspective, depth [0, 1] (Vulkan convention).
    /// glam 0.29 has no perspective_rh_zo, so we build the matrix manually.
    /// Y flip is handled by the negative-height viewport (see gotchas.md).
    pub fn projection(&self) -> Mat4 {
        perspective_rh_zo(self.fov_y, self.aspect, self.near, self.far)
    }

    /// projection × view — combine with a model matrix per mesh to get the full MVP.
    pub fn view_proj(&self) -> Mat4 {
        self.projection() * self.view()
    }

    /// Update mouse/gamepad look only. Position is driven by physics (see `desired_move_velocity`).
    pub fn update(&mut self, input: &InputState, dt: f32) {
        if input.ui_captured {
            return;
        }
        self.yaw += input.mouse_delta.x * self.mouse_sensitivity;
        self.pitch = (self.pitch + input.mouse_delta.y * self.mouse_sensitivity)
            .clamp(-89.0f32.to_radians(), 89.0f32.to_radians());

        const GAMEPAD_LOOK_SPEED: f32 = 2.0;
        self.yaw += input.gamepad_look.x * GAMEPAD_LOOK_SPEED * dt;
        self.pitch = (self.pitch + input.gamepad_look.y * GAMEPAD_LOOK_SPEED * dt)
            .clamp(-89.0f32.to_radians(), 89.0f32.to_radians());
    }

    /// Desired horizontal movement velocity based on current input and look direction.
    /// The caller applies this to the physics body's XZ velocity each tick.
    pub fn desired_move_velocity(&self, input: &InputState) -> Vec3 {
        if input.ui_captured {
            return Vec3::ZERO;
        }
        let fwd = Vec3::new(self.forward().x, 0.0, self.forward().z).normalize_or_zero();
        let rgt = Vec3::new(self.right().x,   0.0, self.right().z  ).normalize_or_zero();
        let speed = if input.sprint { self.move_speed * 2.0 } else { self.move_speed };

        let mut vel = Vec3::ZERO;
        if input.forward  { vel += fwd; }
        if input.backward { vel -= fwd; }
        if input.left     { vel -= rgt; }
        if input.right    { vel += rgt; }
        vel += fwd * (-input.gamepad_move.y);
        vel += rgt *  input.gamepad_move.x;

        vel.normalize_or_zero() * speed
    }
}
