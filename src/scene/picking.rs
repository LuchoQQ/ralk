use glam::{Vec2, Vec3};
use crate::scene::Camera3D;

/// Unproject a screen-space cursor position to a world-space ray.
/// `mouse_px`: cursor in physical pixels, (0,0) = top-left.
/// `window_size`: (width, height) in physical pixels.
/// Returns `(origin, direction_normalized)`.
pub fn screen_to_ray(mouse_px: Vec2, window_size: (u32, u32), camera: &Camera3D) -> (Vec3, Vec3) {
    let ndc_x =  (2.0 * mouse_px.x / window_size.0 as f32) - 1.0;
    let ndc_y = 1.0 - (2.0 * mouse_px.y / window_size.1 as f32);
    let half_fov_tan = (camera.fov_y * 0.5).tan();
    let forward = camera.forward();
    let right   = camera.right();
    let up      = right.cross(forward);
    let dir = (forward
        + right * (ndc_x * camera.aspect * half_fov_tan)
        + up    * (ndc_y * half_fov_tan))
        .normalize();
    (camera.position, dir)
}

/// Ray vs AABB intersection (slab method).
/// Returns the nearest positive hit distance, or `None` if no intersection.
pub fn ray_aabb(origin: Vec3, dir: Vec3, aabb_min: Vec3, aabb_max: Vec3) -> Option<f32> {
    let inv = Vec3::new(1.0 / dir.x, 1.0 / dir.y, 1.0 / dir.z);
    let t1  = (aabb_min - origin) * inv;
    let t2  = (aabb_max - origin) * inv;
    let t_enter = t1.min(t2);
    let t_exit  = t1.max(t2);
    let near = t_enter.x.max(t_enter.y).max(t_enter.z);
    let far  = t_exit.x.min(t_exit.y).min(t_exit.z);
    if far >= near.max(0.0) { Some(near.max(0.0)) } else { None }
}
