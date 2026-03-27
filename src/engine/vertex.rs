use ash::vk;
use bytemuck::{Pod, Zeroable};

/// Vertex type for the particle pipeline (Fase 40). 36 bytes.
/// Billboard quads are built on CPU; this vertex carries world-space position,
/// RGBA color, and UV coordinates for the circular falloff shader.
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct ParticleVertex {
    pub position:  [f32; 3],  // location 0, offset  0, 12 bytes (world-space corner)
    pub color:     [f32; 4],  // location 1, offset 12, 16 bytes (RGBA)
    pub tex_coord: [f32; 2],  // location 2, offset 28,  8 bytes
}                              // stride 36

impl ParticleVertex {
    pub fn binding_description() -> vk::VertexInputBindingDescription {
        vk::VertexInputBindingDescription {
            binding: 0,
            stride: std::mem::size_of::<Self>() as u32, // 36
            input_rate: vk::VertexInputRate::VERTEX,
        }
    }

    pub fn attribute_descriptions() -> [vk::VertexInputAttributeDescription; 3] {
        [
            vk::VertexInputAttributeDescription {
                location: 0,
                binding: 0,
                format: vk::Format::R32G32B32_SFLOAT,
                offset: 0,
            },
            vk::VertexInputAttributeDescription {
                location: 1,
                binding: 0,
                format: vk::Format::R32G32B32A32_SFLOAT,
                offset: 12,
            },
            vk::VertexInputAttributeDescription {
                location: 2,
                binding: 0,
                format: vk::Format::R32G32_SFLOAT,
                offset: 28,
            },
        ]
    }
}

/// Vertex type used by the wireframe debug pipeline (position-only, 12 bytes).
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct WireframeVertex {
    pub position: [f32; 3],
}

impl WireframeVertex {
    pub fn binding_description() -> vk::VertexInputBindingDescription {
        vk::VertexInputBindingDescription {
            binding: 0,
            stride: std::mem::size_of::<Self>() as u32, // 12
            input_rate: vk::VertexInputRate::VERTEX,
        }
    }

    pub fn attribute_descriptions() -> [vk::VertexInputAttributeDescription; 1] {
        [vk::VertexInputAttributeDescription {
            location: 0,
            binding: 0,
            format: vk::Format::R32G32B32_SFLOAT,
            offset: 0,
        }]
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct Vertex {
    pub position:  [f32; 3],  // location 0, offset  0, 12 bytes
    pub normal:    [f32; 3],  // location 1, offset 12, 12 bytes
    pub tex_coord: [f32; 2],  // location 2, offset 24,  8 bytes
    pub tangent:   [f32; 4],  // location 3, offset 32, 16 bytes (xyz + handedness w)
}                             //              stride 48

impl Vertex {
    pub fn binding_description() -> vk::VertexInputBindingDescription {
        vk::VertexInputBindingDescription {
            binding: 0,
            stride: std::mem::size_of::<Self>() as u32, // 48
            input_rate: vk::VertexInputRate::VERTEX,
        }
    }

    pub fn attribute_descriptions() -> [vk::VertexInputAttributeDescription; 4] {
        [
            vk::VertexInputAttributeDescription {
                location: 0,
                binding: 0,
                format: vk::Format::R32G32B32_SFLOAT,
                offset: 0,
            },
            vk::VertexInputAttributeDescription {
                location: 1,
                binding: 0,
                format: vk::Format::R32G32B32_SFLOAT,
                offset: 12,
            },
            vk::VertexInputAttributeDescription {
                location: 2,
                binding: 0,
                format: vk::Format::R32G32_SFLOAT,
                offset: 24,
            },
            vk::VertexInputAttributeDescription {
                location: 3,
                binding: 0,
                format: vk::Format::R32G32B32A32_SFLOAT,
                offset: 32,
            },
        ]
    }
}
