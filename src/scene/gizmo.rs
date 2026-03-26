use glam::{Mat4, Vec2, Vec3};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GizmoMode { Translate, Rotate, Scale }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GizmoAxis { X, Y, Z }

/// State of an in-progress gizmo drag operation.
pub struct GizmoDrag {
    pub axis: GizmoAxis,
    pub mode: GizmoMode,
    /// Normalized screen-space direction of this axis (pixels).
    pub axis_screen_dir: Vec2,
    /// Screen pixels per world unit along this axis.
    pub pixels_per_unit: f32,
}

const GIZMO_LEN:  f32 = 1.2;
const GIZMO_HEAD: f32 = 0.15;
const CIRCLE_SEGS: usize = 32;
const CIRCLE_R:   f32 = GIZMO_LEN * 0.85;

pub const COLOR_X:      [f32; 4] = [1.00, 0.22, 0.22, 1.0];
pub const COLOR_Y:      [f32; 4] = [0.22, 1.00, 0.22, 1.0];
pub const COLOR_Z:      [f32; 4] = [0.22, 0.44, 1.00, 1.0];
pub const COLOR_SELECT: [f32; 4] = [0.0, 1.0, 1.0, 1.0];

pub struct LineGroup {
    pub vertices: Vec<Vec3>,
    pub color:    [f32; 4],
}

fn translate_arrow(origin: Vec3, axis: Vec3, sa: Vec3, sb: Vec3) -> Vec<Vec3> {
    let tip  = origin + axis * GIZMO_LEN;
    let back = -axis * GIZMO_HEAD;
    let s    = GIZMO_HEAD * 0.45;
    let mut v = vec![origin, tip];
    for &side in &[sa, -sa, sb, -sb] {
        v.push(tip);
        v.push(tip + back + side * s);
    }
    v
}

fn circle_lines(origin: Vec3, normal: Vec3, radius: f32) -> Vec<Vec3> {
    let arb = if normal.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
    let t1 = normal.cross(arb).normalize();
    let t2 = normal.cross(t1);
    let mut v = Vec::with_capacity(CIRCLE_SEGS * 2);
    for i in 0..CIRCLE_SEGS {
        let a0 = (i as f32 / CIRCLE_SEGS as f32) * std::f32::consts::TAU;
        let a1 = ((i + 1) as f32 / CIRCLE_SEGS as f32) * std::f32::consts::TAU;
        v.push(origin + (t1 * a0.cos() + t2 * a0.sin()) * radius);
        v.push(origin + (t1 * a1.cos() + t2 * a1.sin()) * radius);
    }
    v
}

fn scale_arrow(origin: Vec3, axis: Vec3, sa: Vec3, sb: Vec3) -> Vec<Vec3> {
    let tip = origin + axis * GIZMO_LEN;
    let s   = GIZMO_HEAD * 0.5;
    let mut v = vec![origin, tip];
    v.push(tip - sa * s); v.push(tip + sa * s);
    v.push(tip - sb * s); v.push(tip + sb * s);
    v.push(tip - axis * s); v.push(tip + axis * s);
    v
}

fn selection_box(center: Vec3, half: Vec3) -> Vec<Vec3> {
    let [hx, hy, hz] = [half.x.abs().max(0.05), half.y.abs().max(0.05), half.z.abs().max(0.05)];
    let c = [
        Vec3::new(-hx,-hy,-hz), Vec3::new( hx,-hy,-hz),
        Vec3::new( hx, hy,-hz), Vec3::new(-hx, hy,-hz),
        Vec3::new(-hx,-hy, hz), Vec3::new( hx,-hy, hz),
        Vec3::new( hx, hy, hz), Vec3::new(-hx, hy, hz),
    ].map(|v| center + v);
    let edges = [(0,1),(1,2),(2,3),(3,0),(4,5),(5,6),(6,7),(7,4),(0,4),(1,5),(2,6),(3,7)];
    let mut v = Vec::with_capacity(24);
    for (a, b) in edges { v.push(c[a]); v.push(c[b]); }
    v
}

pub fn build_selection_group(entity_pos: Vec3, bbox_min: Vec3, bbox_max: Vec3) -> LineGroup {
    let half   = (bbox_max - bbox_min) * 0.5;
    let center = entity_pos + (bbox_min + bbox_max) * 0.5;
    LineGroup { vertices: selection_box(center, half), color: COLOR_SELECT }
}

pub fn build_axis_groups(origin: Vec3, mode: GizmoMode, hovered: Option<GizmoAxis>) -> [LineGroup; 3] {
    let data: [(Vec3, Vec3, Vec3, [f32;4], GizmoAxis); 3] = [
        (Vec3::X, Vec3::Y, Vec3::Z, COLOR_X, GizmoAxis::X),
        (Vec3::Y, Vec3::X, Vec3::Z, COLOR_Y, GizmoAxis::Y),
        (Vec3::Z, Vec3::X, Vec3::Y, COLOR_Z, GizmoAxis::Z),
    ];
    data.map(|(axis, sa, sb, base, gaxis)| {
        let verts = match mode {
            GizmoMode::Translate => translate_arrow(origin, axis, sa, sb),
            GizmoMode::Rotate    => circle_lines(origin, axis, CIRCLE_R),
            GizmoMode::Scale     => scale_arrow(origin, axis, sa, sb),
        };
        let color = if hovered == Some(gaxis) {
            [
                (base[0] * 1.5_f32).min(1.0),
                (base[1] * 1.5_f32).min(1.0),
                (base[2] * 1.5_f32).min(1.0),
                1.0,
            ]
        } else {
            base
        };
        LineGroup { vertices: verts, color }
    })
}

pub fn world_to_screen(pos: Vec3, view_proj: Mat4, screen: Vec2) -> Option<Vec2> {
    let clip = view_proj * pos.extend(1.0);
    if clip.w <= 0.0 { return None; }
    let ndc = clip.truncate() / clip.w;
    Some(Vec2::new(
        (ndc.x + 1.0) * 0.5 * screen.x,
        (1.0 - ndc.y) * 0.5 * screen.y,
    ))
}

fn seg_dist_sq(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let len_sq = ab.length_squared();
    if len_sq < 1e-6 { return (p - a).length_squared(); }
    let t = ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0);
    (p - (a + ab * t)).length_squared()
}

/// Returns `Some((axis, axis_screen_dir_normalized, pixels_per_world_unit))` if mouse hovers an axis.
pub fn hit_test_gizmo(
    mouse: Vec2,
    origin: Vec3,
    mode: GizmoMode,
    view_proj: Mat4,
    screen: Vec2,
    threshold_px: f32,
) -> Option<(GizmoAxis, Vec2, f32)> {
    let origin_s = world_to_screen(origin, view_proj, screen)?;
    let t_sq = threshold_px * threshold_px;
    let axes: [(Vec3, GizmoAxis); 3] = [
        (Vec3::X, GizmoAxis::X),
        (Vec3::Y, GizmoAxis::Y),
        (Vec3::Z, GizmoAxis::Z),
    ];
    let mut best: Option<(f32, GizmoAxis, Vec2, f32)> = None;
    for (axis_dir, gaxis) in axes {
        let tip_w = origin + axis_dir * GIZMO_LEN;
        let Some(tip_s) = world_to_screen(tip_w, view_proj, screen) else { continue };
        let d_sq = match mode {
            GizmoMode::Rotate => {
                let arb = if axis_dir.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
                let t1 = axis_dir.cross(arb).normalize();
                let t2 = axis_dir.cross(t1);
                let mut min = f32::MAX;
                for i in 0..16usize {
                    let a = (i as f32 / 16.0) * std::f32::consts::TAU;
                    let wp = origin + (t1 * a.cos() + t2 * a.sin()) * CIRCLE_R;
                    if let Some(sp) = world_to_screen(wp, view_proj, screen) {
                        min = min.min((mouse - sp).length_squared());
                    }
                }
                min
            }
            _ => seg_dist_sq(mouse, origin_s, tip_s),
        };
        if d_sq < t_sq {
            let raw = tip_s - origin_s;
            let len = raw.length().max(0.001);
            let ppu = len / GIZMO_LEN;
            if best.as_ref().map(|(bd, ..)| d_sq < *bd).unwrap_or(true) {
                best = Some((d_sq, gaxis, raw / len, ppu));
            }
        }
    }
    best.map(|(_, ax, dir, ppu)| (ax, dir, ppu))
}

pub fn drag_axis_dir(axis: GizmoAxis) -> Vec3 {
    match axis { GizmoAxis::X => Vec3::X, GizmoAxis::Y => Vec3::Y, GizmoAxis::Z => Vec3::Z }
}
