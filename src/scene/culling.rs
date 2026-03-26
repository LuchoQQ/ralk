use glam::{Mat4, Vec3, Vec4};

/// Extract 6 frustum planes (in world space) from a combined view-projection matrix.
///
/// Each plane is `Vec4(a, b, c, d)` where `a·x + b·y + c·z + d ≥ 0` means the point
/// is on the INSIDE (visible) half-space. Uses the Gribb/Hartmann row-extraction method.
///
/// Vulkan NDC convention: x ∈ [-1,1], y ∈ [-1,1], z ∈ [0,1].
pub fn extract_frustum_planes(vp: Mat4) -> [Vec4; 6] {
    // glam Mat4 is column-major; row(i) returns the i-th row as a Vec4.
    let r0 = vp.row(0);
    let r1 = vp.row(1);
    let r2 = vp.row(2);
    let r3 = vp.row(3);

    [
        r3 + r0, // left:   x/w ≥ -1  →  (row3+row0)·p ≥ 0
        r3 - r0, // right:  x/w ≤  1  →  (row3-row0)·p ≥ 0
        r3 + r1, // bottom: y/w ≥ -1
        r3 - r1, // top:    y/w ≤  1
        r2,      // near:   z/w ≥  0  (Vulkan depth [0,1])
        r3 - r2, // far:    z/w ≤  1
    ]
}

/// Returns true if the world-space AABB is at least partially inside (or touching) the frustum.
///
/// Uses the p-vertex (most positive corner along each plane normal) test.
/// A single p-vertex outside any plane means the AABB is fully outside → cull it.
pub fn is_aabb_visible(aabb_min: Vec3, aabb_max: Vec3, planes: &[Vec4; 6]) -> bool {
    for plane in planes {
        let n = plane.truncate(); // xyz = normal
        let d = plane.w;

        // p-vertex: the AABB corner most in the direction of the plane normal.
        let px = if n.x >= 0.0 { aabb_max.x } else { aabb_min.x };
        let py = if n.y >= 0.0 { aabb_max.y } else { aabb_min.y };
        let pz = if n.z >= 0.0 { aabb_max.z } else { aabb_min.z };

        // If the "most inside" corner is still outside, the whole AABB is outside.
        if n.x * px + n.y * py + n.z * pz + d < 0.0 {
            return false;
        }
    }
    true
}

/// Transform a local-space AABB by a world matrix and return the new world-space AABB.
/// Computes all 8 corners and takes component-wise min/max.
pub fn transform_aabb(local_min: Vec3, local_max: Vec3, transform: Mat4) -> (Vec3, Vec3) {
    let corners = [
        Vec3::new(local_min.x, local_min.y, local_min.z),
        Vec3::new(local_max.x, local_min.y, local_min.z),
        Vec3::new(local_min.x, local_max.y, local_min.z),
        Vec3::new(local_max.x, local_max.y, local_min.z),
        Vec3::new(local_min.x, local_min.y, local_max.z),
        Vec3::new(local_max.x, local_min.y, local_max.z),
        Vec3::new(local_min.x, local_max.y, local_max.z),
        Vec3::new(local_max.x, local_max.y, local_max.z),
    ];

    let mut world_min = Vec3::splat(f32::MAX);
    let mut world_max = Vec3::splat(f32::MIN);
    for c in &corners {
        let w = transform.transform_point3(*c);
        world_min = world_min.min(w);
        world_max = world_max.max(w);
    }
    (world_min, world_max)
}
