/// CPU-side IBL precomputation: equirectangular HDR → skybox cubemap, irradiance,
/// prefiltered env map, BRDF LUT. All output is RGBA16F (skybox/irradiance/prefiltered)
/// or RG16F (BRDF LUT) as little-endian bytes ready for GPU upload.

use glam::{Vec2, Vec3};
use std::f32::consts::PI;

pub const SKYBOX_FACE_SIZE: u32 = 128;
pub const IRRADIANCE_FACE_SIZE: u32 = 16;
pub const PREFILTERED_FACE_SIZE: u32 = 32;
pub const PREFILTERED_MIP_LEVELS: u32 = 5;
pub const BRDF_LUT_SIZE: u32 = 128;

const IRRADIANCE_SAMPLES: u32 = 32;
const PREFILTERED_SAMPLES: u32 = 32;
const BRDF_SAMPLES: u32 = 32;

/// Linear f32 RGB equirectangular environment map.
pub struct EquiRect {
    pub pixels: Vec<[f32; 3]>,
    pub width: u32,
    pub height: u32,
}

/// All precomputed IBL data, ready for GPU upload.
pub struct IblMaps {
    /// 6 faces, RGBA16F, SKYBOX_FACE_SIZE×SKYBOX_FACE_SIZE each.
    pub skybox_faces: Vec<Vec<u8>>,
    /// 6 faces, RGBA16F, IRRADIANCE_FACE_SIZE×IRRADIANCE_FACE_SIZE each.
    pub irr_faces: Vec<Vec<u8>>,
    /// [mip][face] bytes, RGBA16F, base PREFILTERED_FACE_SIZE >> mip per face.
    pub pre_faces: Vec<Vec<Vec<u8>>>,
    /// RG16F, BRDF_LUT_SIZE × BRDF_LUT_SIZE.
    pub brdf_lut: Vec<u8>,
}

/// Try to load a Radiance HDR (.hdr) file. Falls back to a procedural sky on error.
pub fn load_environment(hdr_path: &str) -> EquiRect {
    match load_hdr(hdr_path) {
        Ok(env) => {
            log::info!("Loaded HDR environment: {hdr_path} ({}×{})", env.width, env.height);
            env
        }
        Err(e) => {
            log::info!("HDR not found ({e}), using procedural sky");
            procedural_sky()
        }
    }
}

fn load_hdr(path: &str) -> anyhow::Result<EquiRect> {
    let bytes = std::fs::read(path)?;
    parse_rgbe(&bytes)
}

/// Minimal Radiance RGBE parser. Supports new RLE and uncompressed scanlines.
fn parse_rgbe(data: &[u8]) -> anyhow::Result<EquiRect> {
    if !data.starts_with(b"#?") {
        anyhow::bail!("Not a Radiance HDR file");
    }
    // Skip header lines until blank line
    let mut i = 0;
    while i < data.len() {
        if data[i] == b'\n' {
            i += 1;
            if i < data.len() && data[i] == b'\n' {
                i += 1;
                break;
            }
        } else {
            i += 1;
        }
    }
    // Read size line: "-Y H +X W"
    let rest = &data[i..];
    let nl = rest.iter().position(|&b| b == b'\n')
        .ok_or_else(|| anyhow::anyhow!("No size line in HDR"))?;
    let size_line = std::str::from_utf8(&rest[..nl])?;
    i += nl + 1;
    let parts: Vec<&str> = size_line.split_whitespace().collect();
    if parts.len() < 4 { anyhow::bail!("Invalid size line: {size_line}"); }
    let height: u32 = parts[1].parse()?;
    let width: u32 = parts[3].parse()?;
    let mut pixels = vec![[0.0f32; 3]; (width * height) as usize];
    decode_rgbe_scanlines(&data[i..], width, height, &mut pixels)?;
    Ok(EquiRect { pixels, width, height })
}

fn decode_rgbe_scanlines(data: &[u8], width: u32, height: u32, out: &mut [[f32; 3]]) -> anyhow::Result<()> {
    let w = width as usize;
    let mut pos = 0usize;
    for row in 0..height as usize {
        if pos + 4 > data.len() {
            anyhow::bail!("Unexpected EOF at row {row}");
        }
        let (b0, b1, b2, b3) = (data[pos], data[pos+1], data[pos+2], data[pos+3]);
        if b0 == 2 && b1 == 2 && (b2 & 0x80) == 0 {
            // New RLE: each of R, G, B, E stored as separate RLE channel
            let scan_w = ((b2 as usize) << 8) | b3 as usize;
            if scan_w != w { anyhow::bail!("Scanline width mismatch"); }
            pos += 4;
            let mut chan = vec![vec![0u8; w]; 4];
            for c in 0..4 {
                let mut x = 0usize;
                while x < w {
                    if pos >= data.len() { anyhow::bail!("EOF in RLE channel"); }
                    let code = data[pos]; pos += 1;
                    if code > 128 {
                        let count = (code - 128) as usize;
                        if pos >= data.len() { anyhow::bail!("EOF in RLE run"); }
                        let val = data[pos]; pos += 1;
                        for k in 0..count { chan[c][x + k] = val; }
                        x += count;
                    } else {
                        let count = code as usize;
                        if pos + count > data.len() { anyhow::bail!("EOF in RLE literal"); }
                        chan[c][x..x+count].copy_from_slice(&data[pos..pos+count]);
                        pos += count;
                        x += count;
                    }
                }
            }
            for x in 0..w {
                out[row * w + x] = rgbe_to_float(chan[0][x], chan[1][x], chan[2][x], chan[3][x]);
            }
        } else {
            // Uncompressed
            for x in 0..w {
                if pos + 4 > data.len() { anyhow::bail!("EOF in raw scanline"); }
                out[row * w + x] = rgbe_to_float(data[pos], data[pos+1], data[pos+2], data[pos+3]);
                pos += 4;
            }
        }
    }
    Ok(())
}

fn rgbe_to_float(r: u8, g: u8, b: u8, e: u8) -> [f32; 3] {
    if e == 0 { return [0.0, 0.0, 0.0]; }
    let scale = 2.0f32.powi(e as i32 - 128 - 8);
    [r as f32 * scale, g as f32 * scale, b as f32 * scale]
}

/// Procedural sky: blue zenith, orange horizon, dark nadir.
pub fn procedural_sky() -> EquiRect {
    let w = 512u32;
    let h = 256u32;
    let mut pixels = vec![[0.0f32; 3]; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let u = (x as f32 + 0.5) / w as f32;
            let v = (y as f32 + 0.5) / h as f32;
            let phi = u * 2.0 * PI;
            let theta = v * PI;
            let dir = Vec3::new(theta.sin() * phi.cos(), theta.cos(), theta.sin() * phi.sin());
            let t = (dir.y + 1.0) * 0.5;
            let sky    = [0.15f32, 0.45, 1.8];
            let horiz  = [1.6f32,  0.65, 0.15];
            let ground = [0.01f32, 0.01, 0.01];
            let color = if t > 0.5 {
                lerp3(horiz, sky, (t - 0.5) * 2.0)
            } else {
                lerp3(ground, horiz, t * 2.0)
            };
            pixels[(y * w + x) as usize] = color;
        }
    }
    EquiRect { pixels, width: w, height: h }
}

// ---------------------------------------------------------------------------
// Main precomputation entry point
// ---------------------------------------------------------------------------

pub fn precompute_ibl(env: &EquiRect) -> IblMaps {
    log::info!("Precomputing IBL (skybox {}², irradiance {}², prefiltered {}²×{} mips, BRDF {}²)...",
        SKYBOX_FACE_SIZE, IRRADIANCE_FACE_SIZE, PREFILTERED_FACE_SIZE, PREFILTERED_MIP_LEVELS, BRDF_LUT_SIZE);
    let skybox_faces = compute_cubemap_faces(env, SKYBOX_FACE_SIZE);
    let irr_faces    = compute_irradiance(env, IRRADIANCE_FACE_SIZE);
    let pre_faces    = compute_prefiltered(env, PREFILTERED_FACE_SIZE, PREFILTERED_MIP_LEVELS);
    let brdf_lut     = compute_brdf_lut(BRDF_LUT_SIZE);
    log::info!("IBL precomputation done");
    IblMaps { skybox_faces, irr_faces, pre_faces, brdf_lut }
}

// ---------------------------------------------------------------------------
// Cubemap helpers
// ---------------------------------------------------------------------------

/// World-space direction for face [0..6] and pixel (u,v) in [0,1].
/// Face order: +X, -X, +Y, -Y, +Z, -Z (Vulkan cubemap face order).
fn face_dir(face: usize, u: f32, v: f32) -> Vec3 {
    let s = u * 2.0 - 1.0;
    let t = v * 2.0 - 1.0;
    let d = match face {
        0 => Vec3::new( 1.0,  -t,  -s), // +X
        1 => Vec3::new(-1.0,  -t,   s), // -X
        2 => Vec3::new(  s,  1.0,   t), // +Y
        3 => Vec3::new(  s, -1.0,  -t), // -Y
        4 => Vec3::new(  s,   -t, 1.0), // +Z
        5 => Vec3::new( -s,   -t,-1.0), // -Z
        _ => unreachable!(),
    };
    d.normalize()
}

/// Nearest-neighbor sample of equirectangular map.
fn sample_equirect(env: &EquiRect, dir: Vec3) -> [f32; 3] {
    let phi   = dir.z.atan2(dir.x);
    let theta = dir.y.clamp(-1.0, 1.0).asin();
    let u = (phi / (2.0 * PI) + 0.5).rem_euclid(1.0);
    let v = 0.5 - theta / PI;
    let px = ((u * env.width  as f32) as u32).min(env.width  - 1);
    let py = ((v * env.height as f32) as u32).min(env.height - 1);
    env.pixels[(py * env.width + px) as usize]
}

fn compute_cubemap_faces(env: &EquiRect, face_size: u32) -> Vec<Vec<u8>> {
    (0..6).map(|face| {
        let mut buf = Vec::with_capacity((face_size * face_size * 8) as usize);
        for y in 0..face_size {
            for x in 0..face_size {
                let dir = face_dir(face, (x as f32 + 0.5) / face_size as f32,
                                          (y as f32 + 0.5) / face_size as f32);
                let [r, g, b] = sample_equirect(env, dir);
                push_rgba16f(&mut buf, r, g, b, 1.0);
            }
        }
        buf
    }).collect()
}

// ---------------------------------------------------------------------------
// Hammersley / GGX importance sampling
// ---------------------------------------------------------------------------

fn radical_inverse_vdc(mut bits: u32) -> f32 {
    bits = bits.reverse_bits();
    bits as f32 * 2.328_306_4e-10
}

fn hammersley(i: u32, n: u32) -> Vec2 {
    Vec2::new(i as f32 / n as f32, radical_inverse_vdc(i))
}

fn importance_sample_ggx(xi: Vec2, n: Vec3, roughness: f32) -> Vec3 {
    let a = roughness * roughness;
    let phi = 2.0 * PI * xi.x;
    let cos_theta = ((1.0 - xi.y) / (1.0 + (a * a - 1.0) * xi.y)).max(0.0).sqrt();
    let sin_theta = (1.0 - cos_theta * cos_theta).max(0.0).sqrt();
    let h = Vec3::new(phi.cos() * sin_theta, phi.sin() * sin_theta, cos_theta);
    let up = if n.z.abs() < 0.999 { Vec3::Z } else { Vec3::X };
    let tangent   = up.cross(n).normalize();
    let bitangent = n.cross(tangent);
    (tangent * h.x + bitangent * h.y + n * h.z).normalize()
}

// ---------------------------------------------------------------------------
// Irradiance (diffuse convolution via cosine-weighted sampling)
// ---------------------------------------------------------------------------

fn compute_irradiance(env: &EquiRect, face_size: u32) -> Vec<Vec<u8>> {
    (0..6).map(|face| {
        let mut buf = Vec::with_capacity((face_size * face_size * 8) as usize);
        for y in 0..face_size {
            for x in 0..face_size {
                let n = face_dir(face, (x as f32 + 0.5) / face_size as f32,
                                        (y as f32 + 0.5) / face_size as f32);
                let up        = if n.z.abs() < 0.999 { Vec3::Z } else { Vec3::X };
                let tangent   = up.cross(n).normalize();
                let bitangent = n.cross(tangent);
                let mut irr = Vec3::ZERO;
                for i in 0..IRRADIANCE_SAMPLES {
                    let xi = hammersley(i, IRRADIANCE_SAMPLES);
                    let phi       = 2.0 * PI * xi.x;
                    let cos_theta = xi.y.sqrt();
                    let sin_theta = (1.0 - xi.y).sqrt();
                    let dir = (tangent   * phi.cos() * sin_theta
                             + bitangent * phi.sin() * sin_theta
                             + n         * cos_theta).normalize();
                    irr += Vec3::from(sample_equirect(env, dir));
                }
                irr /= IRRADIANCE_SAMPLES as f32;
                // Stores mean(L); shader does: diffuse = kD * albedo * irr
                // (equivalent to kD * albedo/π * ∫L·cosθ·dω = kD * albedo * E_irr/π * π)
                push_rgba16f(&mut buf, irr.x, irr.y, irr.z, 1.0);
            }
        }
        buf
    }).collect()
}

// ---------------------------------------------------------------------------
// Prefiltered env map (specular GGX convolution, per mip = per roughness)
// ---------------------------------------------------------------------------

fn compute_prefiltered(env: &EquiRect, base_size: u32, mip_levels: u32) -> Vec<Vec<Vec<u8>>> {
    (0..mip_levels).map(|mip| {
        let roughness = mip as f32 / (mip_levels - 1) as f32;
        let size = (base_size >> mip).max(1);
        (0..6usize).map(|face| {
            let mut buf = Vec::with_capacity((size * size * 8) as usize);
            for y in 0..size {
                for x in 0..size {
                    let n = face_dir(face, (x as f32 + 0.5) / size as f32,
                                           (y as f32 + 0.5) / size as f32);
                    let r = n; // split-sum: V = R = N
                    let (mut color, mut weight) = (Vec3::ZERO, 0.0f32);
                    if roughness < 1e-5 {
                        // mip 0: mirror reflection — just sample directly
                        let [sr, sg, sb] = sample_equirect(env, n);
                        push_rgba16f(&mut buf, sr, sg, sb, 1.0);
                        continue;
                    }
                    for i in 0..PREFILTERED_SAMPLES {
                        let xi = hammersley(i, PREFILTERED_SAMPLES);
                        let h = importance_sample_ggx(xi, n, roughness);
                        let l = (2.0 * r.dot(h) * h - r).normalize();
                        let ndotl = n.dot(l).max(0.0);
                        if ndotl > 0.0 {
                            let s = sample_equirect(env, l);
                            color  += Vec3::from(s) * ndotl;
                            weight += ndotl;
                        }
                    }
                    if weight > 0.0 { color /= weight; }
                    push_rgba16f(&mut buf, color.x, color.y, color.z, 1.0);
                }
            }
            buf
        }).collect()
    }).collect()
}

// ---------------------------------------------------------------------------
// BRDF LUT (split-sum approximation: scale and bias for Schlick-GGX)
// ---------------------------------------------------------------------------

fn compute_brdf_lut(size: u32) -> Vec<u8> {
    let mut rg16 = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let n_dot_v  = (x as f32 + 0.5) / size as f32;
            let roughness = (y as f32 + 0.5) / size as f32;
            let (scale, bias) = integrate_brdf(n_dot_v, roughness);
            push_rg16f(&mut rg16, scale, bias);
        }
    }
    rg16
}

fn integrate_brdf(n_dot_v: f32, roughness: f32) -> (f32, f32) {
    let v   = Vec3::new((1.0 - n_dot_v * n_dot_v).max(0.0).sqrt(), 0.0, n_dot_v);
    let n   = Vec3::Z;
    let (mut a, mut b) = (0.0f32, 0.0f32);
    for i in 0..BRDF_SAMPLES {
        let xi   = hammersley(i, BRDF_SAMPLES);
        let h    = importance_sample_ggx(xi, n, roughness);
        let l    = (2.0 * v.dot(h) * h - v).normalize();
        let ndotl = l.z.max(0.0);
        let ndoth = h.z.max(0.0);
        let vdoth = v.dot(h).max(0.0);
        if ndotl > 0.0 {
            let g     = g_smith_ibl(n_dot_v, ndotl, roughness);
            let g_vis = (g * vdoth) / (ndoth * n_dot_v).max(1e-7);
            let fc    = (1.0 - vdoth).powi(5);
            a += (1.0 - fc) * g_vis;
            b += fc * g_vis;
        }
    }
    (a / BRDF_SAMPLES as f32, b / BRDF_SAMPLES as f32)
}

fn g_smith_ibl(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    // IBL variant: k = a²/2 (no roughness+1 bias)
    let k   = roughness * roughness / 2.0;
    let g1v = n_dot_v / (n_dot_v * (1.0 - k) + k).max(1e-7);
    let g1l = n_dot_l / (n_dot_l * (1.0 - k) + k).max(1e-7);
    g1v * g1l
}

// ---------------------------------------------------------------------------
// f32 → f16 conversion and byte helpers
// ---------------------------------------------------------------------------

/// Convert a 32-bit float to IEEE 754 half-precision bits.
pub fn f32_to_f16(v: f32) -> u16 {
    let bits = v.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp  = ((bits >> 23) & 0xFF) as i32;
    let mant = bits & 0x007F_FFFF;
    if exp == 255 {
        if mant == 0 { return sign | 0x7C00; } // inf
        return sign | 0x7E00 | ((mant >> 13) as u16); // NaN
    }
    let exp16 = exp - 127 + 15;
    if exp16 >= 31 { return sign | 0x7C00; } // overflow → inf
    if exp16 <= 0 {
        if exp16 < -10 { return sign; } // underflow → zero
        let m = (mant | 0x0080_0000) >> (14 - exp16);
        return sign | (m as u16);
    }
    sign | ((exp16 as u16) << 10) | ((mant >> 13) as u16)
}

fn push_rgba16f(buf: &mut Vec<u8>, r: f32, g: f32, b: f32, a: f32) {
    for &v in &[r, g, b, a] {
        let h = f32_to_f16(v);
        buf.push(h as u8);
        buf.push((h >> 8) as u8);
    }
}

fn push_rg16f(buf: &mut Vec<u8>, r: f32, g: f32) {
    for &v in &[r, g] {
        let h = f32_to_f16(v);
        buf.push(h as u8);
        buf.push((h >> 8) as u8);
    }
}

// ---------------------------------------------------------------------------
// Misc helpers
// ---------------------------------------------------------------------------

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0]-a[0])*t, a[1] + (b[1]-a[1])*t, a[2] + (b[2]-a[2])*t]
}
