use anyhow::Result;
use glam::Mat4;

use crate::engine::vertex::Vertex;

// ---------------------------------------------------------------------------
// Asset types
// ---------------------------------------------------------------------------

/// RGBA8 texture data ready for GPU upload.
pub struct TextureData {
    pub pixels: Vec<u8>, // always RGBA8
    pub width: u32,
    pub height: u32,
    /// true → upload as R8G8B8A8_SRGB (albedo); false → R8G8B8A8_UNORM (normals, MR).
    pub is_srgb: bool,
}

/// PBR metallic-roughness material. All texture indices reference `SceneData::textures`.
#[allow(dead_code)]
pub struct MaterialData {
    /// None → default 1×1 white (sRGB).
    pub albedo_tex: Option<usize>,
    /// None → default flat normal [128,128,255,255].
    pub normal_tex: Option<usize>,
    /// None → default metallic=0, roughness=0.5.
    pub metallic_roughness_tex: Option<usize>,
    pub base_color_factor: [f32; 4],
    pub metallic_factor: f32,
    pub roughness_factor: f32,
}

/// CPU-side mesh data ready to be uploaded to the GPU.
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub transform: Mat4,
    /// None → default material.
    pub material_index: Option<usize>,
}

/// Full CPU-side scene.
pub struct SceneData {
    pub meshes: Vec<MeshData>,
    pub textures: Vec<TextureData>,
    pub materials: Vec<MaterialData>,
}

// ---------------------------------------------------------------------------
// glTF loader
// ---------------------------------------------------------------------------

pub fn load_glb(path: &str) -> Result<SceneData> {
    let (document, buffers, raw_images) =
        gltf::import(path).map_err(|e| anyhow::anyhow!("glTF import failed ({path}): {e}"))?;

    // Decode all images to RGBA8 (is_srgb set later in the material pass).
    let mut textures: Vec<TextureData> = raw_images
        .iter()
        .map(|img| {
            let (pixels, width, height) = to_rgba8(img);
            TextureData { pixels, width, height, is_srgb: false }
        })
        .collect();

    // Build a mapping from glTF texture-object index → image index.
    let texture_to_image: Vec<usize> =
        document.textures().map(|t| t.source().index()).collect();

    // Build materials.
    let materials: Vec<MaterialData> = document
        .materials()
        .map(|mat| {
            let pbr = mat.pbr_metallic_roughness();

            let albedo_idx = pbr
                .base_color_texture()
                .map(|t| texture_to_image[t.texture().index()]);
            let normal_idx = mat
                .normal_texture()
                .map(|t| texture_to_image[t.texture().index()]);
            let mr_idx = pbr
                .metallic_roughness_texture()
                .map(|t| texture_to_image[t.texture().index()]);

            MaterialData {
                albedo_tex: albedo_idx,
                normal_tex: normal_idx,
                metallic_roughness_tex: mr_idx,
                base_color_factor: pbr.base_color_factor(),
                metallic_factor: pbr.metallic_factor(),
                roughness_factor: pbr.roughness_factor(),
            }
        })
        .collect();

    // Mark albedo textures as sRGB so the uploader can pick the right VkFormat.
    for mat in &materials {
        if let Some(idx) = mat.albedo_tex {
            if idx < textures.len() {
                textures[idx].is_srgb = true;
            }
        }
    }

    // Collect meshes (recurse node hierarchy).
    let mut meshes = Vec::new();
    for scene in document.scenes() {
        for node in scene.nodes() {
            collect_node(&node, Mat4::IDENTITY, &buffers, &mut meshes);
        }
    }

    if meshes.is_empty() {
        anyhow::bail!("glTF file contains no meshes: {path}");
    }

    log::info!(
        "Loaded glTF '{}': {} mesh(es), {} texture(s), {} material(s)",
        path,
        meshes.len(),
        textures.len(),
        materials.len(),
    );

    Ok(SceneData { meshes, textures, materials })
}

fn collect_node(
    node: &gltf::Node,
    parent_transform: Mat4,
    buffers: &[gltf::buffer::Data],
    out: &mut Vec<MeshData>,
) {
    let local = Mat4::from_cols_array_2d(&node.transform().matrix());
    let world = parent_transform * local;

    if let Some(mesh) = node.mesh() {
        for primitive in mesh.primitives() {
            if let Some((vertices, indices)) = load_primitive(&primitive, buffers) {
                // gltf default material has index() == None
                let material_index = primitive.material().index();
                out.push(MeshData { vertices, indices, transform: world, material_index });
            }
        }
    }

    for child in node.children() {
        collect_node(&child, world, buffers, out);
    }
}

fn load_primitive(
    primitive: &gltf::Primitive,
    buffers: &[gltf::buffer::Data],
) -> Option<(Vec<Vertex>, Vec<u32>)> {
    if primitive.mode() != gltf::mesh::Mode::Triangles {
        return None;
    }

    let reader = primitive.reader(|buf| Some(&buffers[buf.index()]));

    let positions: Vec<[f32; 3]> = reader.read_positions()?.collect();
    let count = positions.len();

    let normals: Vec<[f32; 3]> = reader
        .read_normals()
        .map(|n| n.collect())
        .unwrap_or_else(|| vec![[0.0, 0.0, 1.0]; count]);

    let tex_coords: Vec<[f32; 2]> = reader
        .read_tex_coords(0)
        .map(|tc| tc.into_f32().collect())
        .unwrap_or_else(|| vec![[0.0, 0.0]; count]);

    let tangents: Vec<[f32; 4]> = reader
        .read_tangents()
        .map(|t| t.collect())
        .unwrap_or_else(|| vec![[1.0, 0.0, 0.0, 1.0]; count]);

    let vertices: Vec<Vertex> = positions
        .into_iter()
        .zip(normals)
        .zip(tex_coords)
        .zip(tangents)
        .map(|(((position, normal), tex_coord), tangent)| Vertex {
            position,
            normal,
            tex_coord,
            tangent,
        })
        .collect();

    let indices: Vec<u32> = reader
        .read_indices()
        .map(|i| i.into_u32().collect())
        .unwrap_or_else(|| (0..count as u32).collect());

    Some((vertices, indices))
}

/// Convert a gltf image to RGBA8. Returns (pixels, width, height).
fn to_rgba8(img: &gltf::image::Data) -> (Vec<u8>, u32, u32) {
    use gltf::image::Format;
    let pixels = match img.format {
        Format::R8G8B8A8 => img.pixels.clone(),
        Format::R8G8B8 => img
            .pixels
            .chunks(3)
            .flat_map(|c| [c[0], c[1], c[2], 255])
            .collect(),
        fmt => {
            log::warn!("Unsupported glTF image format {:?}, using 1×1 white fallback", fmt);
            return (vec![255, 255, 255, 255], 1, 1);
        }
    };
    (pixels, img.width, img.height)
}

// ---------------------------------------------------------------------------
// Built-in fallback: unit cube
// ---------------------------------------------------------------------------

pub fn builtin_cube() -> SceneData {
    #[rustfmt::skip]
    let faces: [([f32; 3], [f32; 3], [[f32; 3]; 4]); 6] = [
        // (normal, tangent_xyz, [v0, v1, v2, v3] positions)
        ([0.0,  0.0,  1.0], [ 1.0, 0.0,  0.0], [[-0.5,-0.5, 0.5],[ 0.5,-0.5, 0.5],[ 0.5, 0.5, 0.5],[-0.5, 0.5, 0.5]]),
        ([0.0,  0.0, -1.0], [-1.0, 0.0,  0.0], [[ 0.5,-0.5,-0.5],[-0.5,-0.5,-0.5],[-0.5, 0.5,-0.5],[ 0.5, 0.5,-0.5]]),
        ([1.0,  0.0,  0.0], [ 0.0, 0.0, -1.0], [[ 0.5,-0.5, 0.5],[ 0.5,-0.5,-0.5],[ 0.5, 0.5,-0.5],[ 0.5, 0.5, 0.5]]),
        ([-1.0, 0.0,  0.0], [ 0.0, 0.0,  1.0], [[-0.5,-0.5,-0.5],[-0.5,-0.5, 0.5],[-0.5, 0.5, 0.5],[-0.5, 0.5,-0.5]]),
        ([0.0,  1.0,  0.0], [ 1.0, 0.0,  0.0], [[-0.5, 0.5, 0.5],[ 0.5, 0.5, 0.5],[ 0.5, 0.5,-0.5],[-0.5, 0.5,-0.5]]),
        ([0.0, -1.0,  0.0], [ 1.0, 0.0,  0.0], [[-0.5,-0.5,-0.5],[ 0.5,-0.5,-0.5],[ 0.5,-0.5, 0.5],[-0.5,-0.5, 0.5]]),
    ];

    let uvs: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);

    for (face_idx, (normal, tangent, positions)) in faces.iter().enumerate() {
        let base = (face_idx * 4) as u32;
        for (i, pos) in positions.iter().enumerate() {
            vertices.push(Vertex {
                position: *pos,
                normal: *normal,
                tex_coord: uvs[i],
                tangent: [tangent[0], tangent[1], tangent[2], 1.0],
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    SceneData {
        meshes: vec![MeshData {
            vertices,
            indices,
            transform: Mat4::IDENTITY,
            material_index: None,
        }],
        textures: vec![],
        materials: vec![],
    }
}
