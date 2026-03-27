use std::collections::HashMap;

use anyhow::{Context, Result};
use ash::vk;
use bytemuck;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, AllocationScheme, Allocator, AllocatorCreateDesc};
use gpu_allocator::MemoryLocation;

use super::vertex::Vertex;

// ---------------------------------------------------------------------------
// Opaque handles — never expose Vulkan raw handles outside src/engine/
// ---------------------------------------------------------------------------

/// Opaque handle to a GPU buffer. Uses a generational ID to detect use-after-free.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BufferHandle(u64);

/// Opaque handle to a GPU image + image view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageHandle(u64);

/// One LOD level of a mesh: offset + count into the mega index buffer.
/// Shared vertex pool — vertex_offset is stored once in GpuMesh / GpuMeshInfo.
#[derive(Clone, Copy)]
pub struct GpuMeshLod {
    pub first_index: u32,
    pub index_count: u32,
}

/// Mesh descriptor in the mega vertex/index buffer (Fase 23 GPU-driven rendering).
/// Fase 24: extended to hold up to 4 LOD levels (indices only; vertices are shared).
pub struct GpuMesh {
    /// LOD levels in order: 0 = full detail, 1 = 50%, 2 = 25%, 3 = 12.5%.
    pub lods:          [GpuMeshLod; 4],
    /// Number of valid entries in `lods` (≥ 1).
    pub lod_count:     u32,
    /// Constant offset added to every index (same pool for all LODs).
    pub vertex_offset: i32,
}

/// Per-instance GPU data written to the instance SSBO every frame.
/// Matches the GLSL `InstanceData` struct in cull.comp / triangle.vert (std430, 112 bytes).
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceData {
    pub model:      [[f32; 4]; 4],  // 64 bytes
    pub world_min:  [f32; 4],       // 16 bytes — world-space AABB min (xyz, w unused)
    pub world_max:  [f32; 4],       // 16 bytes — world-space AABB max (xyz, w unused)
    pub mesh_index: u32,            //  4 bytes
    pub _pad:       [u32; 3],       // 12 bytes
}

/// One LOD level in the GPU mesh-info SSBO. 8 bytes.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuMeshLodInfo {
    pub first_index: u32,  // 4 bytes
    pub index_count: u32,  // 4 bytes
}

/// Static per-mesh data in the mesh-info SSBO (uploaded once at scene load).
/// Matches the GLSL `MeshInfo` struct in cull.comp (std430, 48 bytes).
/// Fase 24: extended from 16 bytes to 48 bytes to hold 4 LOD levels.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuMeshInfo {
    pub lods:          [GpuMeshLodInfo; 4],  // 32 bytes — 4 LOD levels
    pub vertex_offset: i32,                  //  4 bytes, offset 32
    pub lod_count:     u32,                  //  4 bytes, offset 36
    pub _pad:          [u32; 2],             //  8 bytes — align to 16
}  // 48 bytes total

/// Matches `VkDrawIndexedIndirectCommand` exactly (20 bytes).
/// Written by the cull compute shader; consumed by vkCmdDrawIndexedIndirect.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DrawIndirectCommand {
    pub index_count:    u32,
    pub instance_count: u32,
    pub first_index:    u32,
    pub vertex_offset:  i32,
    pub first_instance: u32,
}

// ---------------------------------------------------------------------------
// Internal bookkeeping
// ---------------------------------------------------------------------------

struct BufferEntry {
    buffer: vk::Buffer,
    allocation: Option<Allocation>,
    #[allow(dead_code)]
    size: u64,
}

struct ImageEntry {
    image: vk::Image,
    image_view: vk::ImageView,
    allocation: Option<Allocation>,
}

// ---------------------------------------------------------------------------
// GpuResourceManager
// ---------------------------------------------------------------------------

pub struct GpuResourceManager {
    device: ash::Device,
    allocator: Option<Allocator>,
    queue: vk::Queue,
    transfer_command_pool: vk::CommandPool,
    buffers: HashMap<u64, BufferEntry>,
    next_buffer_id: u64,
    images: HashMap<u64, ImageEntry>,
    next_image_id: u64,
}

impl GpuResourceManager {
    pub fn new(
        instance: &ash::Instance,
        device: ash::Device,
        physical_device: vk::PhysicalDevice,
        queue: vk::Queue,
        queue_family_index: u32,
    ) -> Result<Self> {
        let allocator = Allocator::new(&AllocatorCreateDesc {
            instance: instance.clone(),
            device: device.clone(),
            physical_device,
            debug_settings: gpu_allocator::AllocatorDebugSettings {
                log_memory_information: cfg!(debug_assertions),
                log_leaks_on_shutdown: cfg!(debug_assertions),
                ..Default::default()
            },
            buffer_device_address: false,
            allocation_sizes: Default::default(),
        })
        .context("Failed to create GPU allocator")?;

        // Create a dedicated TRANSIENT command pool for one-shot transfer commands.
        let pool_info = vk::CommandPoolCreateInfo::default()
            .flags(vk::CommandPoolCreateFlags::TRANSIENT)
            .queue_family_index(queue_family_index);

        // SAFETY: device is valid, pool_info is well-formed.
        let transfer_command_pool =
            unsafe { device.create_command_pool(&pool_info, None) }
                .context("Failed to create transfer command pool")?;

        log::info!("GPU resource manager created");

        Ok(Self {
            device,
            allocator: Some(allocator),
            queue,
            transfer_command_pool,
            buffers: HashMap::new(),
            next_buffer_id: 1,
            images: HashMap::new(),
            next_image_id: 1,
        })
    }

    /// Create a buffer with the given size, usage flags, and memory location.
    /// Returns an opaque `BufferHandle`.
    pub fn create_buffer(
        &mut self,
        size: u64,
        usage: vk::BufferUsageFlags,
        location: MemoryLocation,
    ) -> Result<BufferHandle> {
        let buffer_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        // SAFETY: device is valid, buffer_info is well-formed.
        let buffer = unsafe { self.device.create_buffer(&buffer_info, None) }
            .context("Failed to create buffer")?;

        // SAFETY: device and buffer are valid.
        let requirements = unsafe { self.device.get_buffer_memory_requirements(buffer) };

        let allocation = self
            .allocator
            .as_mut()
            .expect("allocator alive")
            .allocate(&AllocationCreateDesc {
                name: "buffer",
                requirements,
                location,
                linear: true,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .context("Failed to allocate buffer memory")?;

        // SAFETY: device, buffer, and allocation are valid. Offset comes from the allocator.
        unsafe {
            self.device
                .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
        }
        .context("Failed to bind buffer memory")?;

        let id = self.next_buffer_id;
        self.next_buffer_id += 1;

        self.buffers.insert(
            id,
            BufferEntry {
                buffer,
                allocation: Some(allocation),
                size,
            },
        );

        Ok(BufferHandle(id))
    }

    /// Destroy a buffer and free its GPU memory.
    pub fn destroy_buffer(&mut self, handle: BufferHandle) {
        if let Some(mut entry) = self.buffers.remove(&handle.0) {
            // SAFETY: device is valid, buffer was created by us.
            unsafe {
                self.device.destroy_buffer(entry.buffer, None);
            }
            if let Some(allocation) = entry.allocation.take() {
                self.allocator
                    .as_mut()
                    .expect("allocator alive")
                    .free(allocation)
                    .expect("Failed to free buffer allocation");
            }
        }
    }

    /// Upload data to a device-local buffer via staging.
    ///
    /// 1. Creates a HOST_VISIBLE staging buffer, maps and copies data
    /// 2. Creates a DEVICE_LOCAL destination buffer
    /// 3. Records and executes a one-shot copy command
    /// 4. Waits with a fence, destroys the staging buffer
    /// 5. Returns the handle to the device-local buffer
    pub fn upload_to_device_local(
        &mut self,
        data: &[u8],
        usage: vk::BufferUsageFlags,
    ) -> Result<BufferHandle> {
        let size = data.len() as u64;

        // --- Staging buffer (host-visible) ---
        let staging = self.create_buffer(
            size,
            vk::BufferUsageFlags::TRANSFER_SRC,
            MemoryLocation::CpuToGpu,
        )?;

        // Map, copy, unmap
        {
            let entry = self.buffers.get(&staging.0).expect("staging exists");
            let allocation = entry.allocation.as_ref().expect("allocation exists");
            let mapped = allocation
                .mapped_ptr()
                .context("Staging buffer is not host-mapped")?
                .as_ptr() as *mut u8;
            // SAFETY: mapped pointer is valid for `size` bytes (we just allocated it).
            unsafe {
                std::ptr::copy_nonoverlapping(data.as_ptr(), mapped, data.len());
            }
        }

        // --- Device-local buffer ---
        let device_local = self.create_buffer(
            size,
            usage | vk::BufferUsageFlags::TRANSFER_DST,
            MemoryLocation::GpuOnly,
        )?;

        // --- One-shot copy command ---
        let staging_vk = self.buffers[&staging.0].buffer;
        let dst_vk = self.buffers[&device_local.0].buffer;

        self.execute_one_shot(|device, cmd| {
            let region = vk::BufferCopy {
                src_offset: 0,
                dst_offset: 0,
                size,
            };
            // SAFETY: cmd is recording, both buffers are valid.
            unsafe {
                device.cmd_copy_buffer(cmd, staging_vk, dst_vk, &[region]);
            }
        })?;

        // Staging no longer needed
        self.destroy_buffer(staging);

        log::info!("Buffer uploaded to device-local memory ({size} bytes)");
        Ok(device_local)
    }

    /// Destroy ALL buffers and images, then drop the allocator.
    /// Must be called before dropping Self to avoid gpu-allocator panic in debug.
    pub fn destroy_all(&mut self) {
        // Destroy images
        let img_ids: Vec<u64> = self.images.keys().copied().collect();
        for id in img_ids {
            if let Some(mut entry) = self.images.remove(&id) {
                // SAFETY: device is valid, image/view were created by us.
                unsafe {
                    self.device.destroy_image_view(entry.image_view, None);
                    self.device.destroy_image(entry.image, None);
                }
                if let Some(allocation) = entry.allocation.take() {
                    self.allocator
                        .as_mut()
                        .expect("allocator alive")
                        .free(allocation)
                        .expect("Failed to free image allocation");
                }
            }
        }

        // Destroy buffers
        let ids: Vec<u64> = self.buffers.keys().copied().collect();
        for id in ids {
            if let Some(mut entry) = self.buffers.remove(&id) {
                // SAFETY: device is valid, buffer was created by us.
                unsafe {
                    self.device.destroy_buffer(entry.buffer, None);
                }
                if let Some(allocation) = entry.allocation.take() {
                    self.allocator
                        .as_mut()
                        .expect("allocator alive")
                        .free(allocation)
                        .expect("Failed to free allocation during destroy_all");
                }
            }
        }

        // Drop the allocator (no live allocations → no panic)
        drop(self.allocator.take());

        // Destroy the transfer command pool
        // SAFETY: device is valid, pool was created by us, no commands in flight
        // (caller must ensure device_wait_idle before calling destroy_all).
        unsafe {
            self.device
                .destroy_command_pool(self.transfer_command_pool, None);
        }

        log::info!("GPU resource manager destroyed (all allocations freed)");
    }

    // ---------------------------------------------------------------------------
    // Buffer I/O
    // ---------------------------------------------------------------------------

    /// Write bytes into a HOST_VISIBLE (CpuToGpu) buffer.
    /// Panics if the buffer is not persistently mapped or data exceeds buffer size.
    pub fn write_buffer(&self, handle: BufferHandle, data: &[u8]) {
        let entry = self.buffers.get(&handle.0).expect("buffer exists");
        let allocation = entry.allocation.as_ref().expect("allocation exists");
        let mapped = allocation
            .mapped_ptr()
            .expect("write_buffer: buffer must be CpuToGpu (host-visible)")
            .as_ptr() as *mut u8;
        debug_assert!(data.len() <= entry.size as usize, "write exceeds buffer size");
        // SAFETY: mapped ptr is valid for `size` bytes; debug_assert guards the bound.
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr(), mapped, data.len());
        }
    }

    // ---------------------------------------------------------------------------
    // Mega-buffer helpers (Fase 23 GPU-driven rendering)
    // ---------------------------------------------------------------------------

    /// Build the scene-wide mega vertex + index buffers from pre-concatenated data.
    ///
    /// All mesh geometry is packed into two device-local buffers.  Individual meshes are
    /// addressed via `first_index` / `vertex_offset` stored in each `GpuMesh`.
    pub fn build_mega_buffers(
        &mut self,
        all_vertices: &[Vertex],
        all_indices:  &[u32],
    ) -> Result<(BufferHandle, BufferHandle)> {
        let vb = self.upload_to_device_local(
            bytemuck::cast_slice(all_vertices),
            vk::BufferUsageFlags::VERTEX_BUFFER,
        )?;
        let ib = self.upload_to_device_local(
            bytemuck::cast_slice(all_indices),
            vk::BufferUsageFlags::INDEX_BUFFER,
        )?;
        log::info!(
            "Mega buffers built: {} vertices ({} KiB), {} indices ({} KiB)",
            all_vertices.len(),
            all_vertices.len() * std::mem::size_of::<Vertex>() / 1024,
            all_indices.len(),
            all_indices.len() * 4 / 1024,
        );
        Ok((vb, ib))
    }

    // ---------------------------------------------------------------------------
    // Texture helpers
    // ---------------------------------------------------------------------------

    /// Upload RGBA8 pixel data to a device-local VkImage.
    ///
    /// `format` must be `R8G8B8A8_SRGB` (albedo) or `R8G8B8A8_UNORM` (normal/metallic).
    /// Uses a staging buffer + layout transitions (see gotchas.md — layout transitions).
    pub fn upload_texture(
        &mut self,
        pixels: &[u8],
        width: u32,
        height: u32,
        format: vk::Format,
    ) -> Result<ImageHandle> {
        let size = (width * height * 4) as u64;
        assert_eq!(pixels.len() as u64, size, "pixels must be RGBA8 ({size} bytes)");

        // --- Staging buffer ---
        let staging = self.create_buffer(
            size,
            vk::BufferUsageFlags::TRANSFER_SRC,
            MemoryLocation::CpuToGpu,
        )?;
        {
            let entry = self.buffers.get(&staging.0).expect("staging exists");
            let mapped = entry
                .allocation
                .as_ref()
                .unwrap()
                .mapped_ptr()
                .expect("staging is host-visible")
                .as_ptr() as *mut u8;
            // SAFETY: mapped ptr valid for `size` bytes.
            unsafe { std::ptr::copy_nonoverlapping(pixels.as_ptr(), mapped, pixels.len()) };
        }

        // Compute mip levels: floor(log2(max(w,h))) + 1.
        let mip_levels = (width.max(height) as f32).log2().floor() as u32 + 1;

        // --- VkImage (device-local, optimal tiling) ---
        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D { width, height, depth: 1 })
            .mip_levels(mip_levels)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            // TRANSFER_SRC needed to blit mip N-1 → N during mip generation.
            .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::TRANSFER_SRC | vk::ImageUsageFlags::SAMPLED)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        // SAFETY: device is valid, image_info is well-formed.
        let image = unsafe { self.device.create_image(&image_info, None) }
            .context("Failed to create image")?;

        // SAFETY: device and image are valid.
        let requirements = unsafe { self.device.get_image_memory_requirements(image) };

        let allocation = self
            .allocator
            .as_mut()
            .expect("allocator alive")
            .allocate(&AllocationCreateDesc {
                name: "texture",
                requirements,
                location: MemoryLocation::GpuOnly,
                linear: false, // false = tiled (OPTIMAL) — see gotchas.md
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .context("Failed to allocate image memory")?;

        // SAFETY: image and allocation are valid.
        unsafe {
            self.device
                .bind_image_memory(image, allocation.memory(), allocation.offset())
        }
        .context("Failed to bind image memory")?;

        // --- VkImageView (covers all mip levels) ---
        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: mip_levels,
                base_array_layer: 0,
                layer_count: 1,
            });

        // SAFETY: device, image, and format are valid.
        let image_view = unsafe { self.device.create_image_view(&view_info, None) }
            .context("Failed to create image view")?;

        // --- Upload: layout transition → copy mip 0 → blit chain → transition to shader-read ---
        let staging_vk = self.buffers[&staging.0].buffer;
        let full_range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: mip_levels,
            base_array_layer: 0,
            layer_count: 1,
        };

        self.execute_one_shot(|device, cmd| {
            // Transition ALL mip levels: UNDEFINED → TRANSFER_DST_OPTIMAL
            let to_transfer = vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(full_range);
            // SAFETY: cmd is recording.
            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[], &[], &[to_transfer],
                );
            }

            // Copy staging buffer → mip 0
            let region = vk::BufferImageCopy {
                buffer_offset: 0,
                buffer_row_length: 0,
                buffer_image_height: 0,
                image_subresource: vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                },
                image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
                image_extent: vk::Extent3D { width, height, depth: 1 },
            };
            // SAFETY: cmd is recording, staging buffer and image are valid.
            unsafe {
                device.cmd_copy_buffer_to_image(
                    cmd, staging_vk, image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[region],
                );
            }

            // Blit chain: generate mip levels 1..N from the previous level.
            let mut mip_w = width;
            let mut mip_h = height;
            for i in 1..mip_levels {
                let src_range = vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: i - 1,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                };

                // Transition mip i-1: TRANSFER_DST → TRANSFER_SRC (blit source)
                let to_src = vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                    .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .image(image)
                    .subresource_range(src_range);
                // SAFETY: cmd is recording.
                unsafe {
                    device.cmd_pipeline_barrier(
                        cmd,
                        vk::PipelineStageFlags::TRANSFER,
                        vk::PipelineStageFlags::TRANSFER,
                        vk::DependencyFlags::empty(),
                        &[], &[], &[to_src],
                    );
                }

                let next_w = (mip_w / 2).max(1);
                let next_h = (mip_h / 2).max(1);

                let blit = vk::ImageBlit {
                    src_subresource: vk::ImageSubresourceLayers {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        mip_level: i - 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    },
                    src_offsets: [
                        vk::Offset3D { x: 0, y: 0, z: 0 },
                        vk::Offset3D { x: mip_w as i32, y: mip_h as i32, z: 1 },
                    ],
                    dst_subresource: vk::ImageSubresourceLayers {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        mip_level: i,
                        base_array_layer: 0,
                        layer_count: 1,
                    },
                    dst_offsets: [
                        vk::Offset3D { x: 0, y: 0, z: 0 },
                        vk::Offset3D { x: next_w as i32, y: next_h as i32, z: 1 },
                    ],
                };
                // SAFETY: cmd is recording, both mip levels are in valid layouts.
                unsafe {
                    device.cmd_blit_image(
                        cmd,
                        image, vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                        image, vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        &[blit],
                        vk::Filter::LINEAR,
                    );
                }

                // Transition mip i-1: TRANSFER_SRC → SHADER_READ_ONLY (done with it)
                let to_shader = vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_READ)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ)
                    .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                    .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .image(image)
                    .subresource_range(src_range);
                // SAFETY: cmd is recording.
                unsafe {
                    device.cmd_pipeline_barrier(
                        cmd,
                        vk::PipelineStageFlags::TRANSFER,
                        vk::PipelineStageFlags::FRAGMENT_SHADER,
                        vk::DependencyFlags::empty(),
                        &[], &[], &[to_shader],
                    );
                }

                mip_w = next_w;
                mip_h = next_h;
            }

            // Transition the last mip level: TRANSFER_DST → SHADER_READ_ONLY
            let last_range = vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: mip_levels - 1,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            };
            let last_to_shader = vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(last_range);
            // SAFETY: cmd is recording.
            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(),
                    &[], &[], &[last_to_shader],
                );
            }
        })?;

        self.destroy_buffer(staging);

        let id = self.next_image_id;
        self.next_image_id += 1;
        self.images.insert(id, ImageEntry { image, image_view, allocation: Some(allocation) });

        log::debug!("Texture uploaded ({width}×{height}, {size} bytes)");
        Ok(ImageHandle(id))
    }

    /// Upload raw bytes to a device-local 2D image (any format, any bytes-per-pixel).
    ///
    /// Unlike `upload_texture`, this function imposes no constraint on format or
    /// bytes-per-pixel — `data.len()` is used as the staging size directly.
    pub fn upload_image_raw(
        &mut self,
        data: &[u8],
        width: u32,
        height: u32,
        format: vk::Format,
    ) -> Result<ImageHandle> {
        let size = data.len() as u64;

        let staging = self.create_buffer(size, vk::BufferUsageFlags::TRANSFER_SRC, MemoryLocation::CpuToGpu)?;
        {
            let entry = self.buffers.get(&staging.0).expect("staging exists");
            let mapped = entry.allocation.as_ref().unwrap().mapped_ptr()
                .expect("staging is host-visible").as_ptr() as *mut u8;
            // SAFETY: mapped ptr valid for `size` bytes.
            unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), mapped, data.len()) };
        }

        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D { width, height, depth: 1 })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        // SAFETY: device is valid.
        let image = unsafe { self.device.create_image(&image_info, None) }
            .context("Failed to create raw image")?;

        let requirements = unsafe { self.device.get_image_memory_requirements(image) };
        let allocation = self.allocator.as_mut().expect("allocator alive")
            .allocate(&AllocationCreateDesc {
                name: "raw_image",
                requirements,
                location: MemoryLocation::GpuOnly,
                linear: false,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .context("Failed to allocate raw image memory")?;

        // SAFETY: image and allocation are valid.
        unsafe { self.device.bind_image_memory(image, allocation.memory(), allocation.offset()) }
            .context("Failed to bind raw image memory")?;

        let subresource_range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        };
        let view_info = vk::ImageViewCreateInfo::default()
            .image(image).view_type(vk::ImageViewType::TYPE_2D).format(format)
            .subresource_range(subresource_range);

        // SAFETY: device, image, and format are valid.
        let image_view = unsafe { self.device.create_image_view(&view_info, None) }
            .context("Failed to create raw image view")?;

        let staging_vk = self.buffers[&staging.0].buffer;
        self.execute_one_shot(|device, cmd| {
            let to_transfer = vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image).subresource_range(subresource_range);
            // SAFETY: cmd is recording.
            unsafe {
                device.cmd_pipeline_barrier(cmd,
                    vk::PipelineStageFlags::TOP_OF_PIPE, vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(), &[], &[], &[to_transfer]);
                device.cmd_copy_buffer_to_image(cmd, staging_vk, image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL, &[vk::BufferImageCopy {
                        buffer_offset: 0, buffer_row_length: 0, buffer_image_height: 0,
                        image_subresource: vk::ImageSubresourceLayers {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            mip_level: 0, base_array_layer: 0, layer_count: 1,
                        },
                        image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
                        image_extent: vk::Extent3D { width, height, depth: 1 },
                    }]);
            }
            let to_shader = vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image).subresource_range(subresource_range);
            // SAFETY: cmd is recording.
            unsafe {
                device.cmd_pipeline_barrier(cmd,
                    vk::PipelineStageFlags::TRANSFER, vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(), &[], &[], &[to_shader]);
            }
        })?;

        self.destroy_buffer(staging);

        let id = self.next_image_id;
        self.next_image_id += 1;
        self.images.insert(id, ImageEntry { image, image_view, allocation: Some(allocation) });

        log::debug!("Raw image uploaded ({width}×{height}, {size} bytes, {format:?})");
        Ok(ImageHandle(id))
    }

    /// Upload a cubemap with one or more mip levels.
    ///
    /// `mip_faces[mip][face]` holds the pre-encoded bytes for that mip/face.
    /// `base_face_size` is the width/height of mip 0; each successive mip is half that.
    /// `format` must be consistent with the byte layout (e.g. R16G16B16A16_SFLOAT = 8 bytes/px).
    pub fn upload_cubemap_mips(
        &mut self,
        mip_faces: &[Vec<Vec<u8>>],
        base_face_size: u32,
        format: vk::Format,
    ) -> Result<ImageHandle> {
        let mip_levels = mip_faces.len() as u32;

        // Concatenate all mip/face data into one staging buffer.
        let total_size: u64 = mip_faces.iter()
            .flat_map(|faces| faces.iter())
            .map(|f| f.len() as u64)
            .sum();

        let staging = self.create_buffer(total_size, vk::BufferUsageFlags::TRANSFER_SRC, MemoryLocation::CpuToGpu)?;
        {
            let entry = self.buffers.get(&staging.0).expect("staging exists");
            let mapped = entry.allocation.as_ref().unwrap().mapped_ptr()
                .expect("host-visible").as_ptr() as *mut u8;
            let mut offset = 0usize;
            for faces in mip_faces {
                for face_data in faces {
                    // SAFETY: mapped pointer covers total_size bytes, offset stays in bounds.
                    unsafe {
                        std::ptr::copy_nonoverlapping(face_data.as_ptr(), mapped.add(offset), face_data.len());
                    }
                    offset += face_data.len();
                }
            }
        }

        // Create VkImage with CUBE_COMPATIBLE flag.
        let image_info = vk::ImageCreateInfo::default()
            .flags(vk::ImageCreateFlags::CUBE_COMPATIBLE)
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D { width: base_face_size, height: base_face_size, depth: 1 })
            .mip_levels(mip_levels)
            .array_layers(6)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        // SAFETY: device is valid.
        let image = unsafe { self.device.create_image(&image_info, None) }
            .context("Failed to create cubemap image")?;

        let requirements = unsafe { self.device.get_image_memory_requirements(image) };
        let allocation = self.allocator.as_mut().expect("allocator alive")
            .allocate(&AllocationCreateDesc {
                name: "cubemap",
                requirements,
                location: MemoryLocation::GpuOnly,
                linear: false,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .context("Failed to allocate cubemap memory")?;

        // SAFETY: image and allocation are valid.
        unsafe { self.device.bind_image_memory(image, allocation.memory(), allocation.offset()) }
            .context("Failed to bind cubemap memory")?;

        // Build one BufferImageCopy per mip × face.
        let mut copies: Vec<vk::BufferImageCopy> = Vec::new();
        let mut buf_offset = 0u64;
        for (mip, faces) in mip_faces.iter().enumerate() {
            let face_size = (base_face_size >> mip).max(1);
            for (face, face_data) in faces.iter().enumerate() {
                copies.push(vk::BufferImageCopy {
                    buffer_offset: buf_offset,
                    buffer_row_length: 0,
                    buffer_image_height: 0,
                    image_subresource: vk::ImageSubresourceLayers {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        mip_level: mip as u32,
                        base_array_layer: face as u32,
                        layer_count: 1,
                    },
                    image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
                    image_extent: vk::Extent3D { width: face_size, height: face_size, depth: 1 },
                });
                buf_offset += face_data.len() as u64;
            }
        }

        let full_range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: mip_levels,
            base_array_layer: 0,
            layer_count: 6,
        };
        let staging_vk = self.buffers[&staging.0].buffer;

        self.execute_one_shot(|device, cmd| {
            let to_transfer = vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image).subresource_range(full_range);
            // SAFETY: cmd is recording.
            unsafe {
                device.cmd_pipeline_barrier(cmd,
                    vk::PipelineStageFlags::TOP_OF_PIPE, vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(), &[], &[], &[to_transfer]);
                device.cmd_copy_buffer_to_image(cmd, staging_vk, image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL, &copies);
            }
            let to_shader = vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image).subresource_range(full_range);
            // SAFETY: cmd is recording.
            unsafe {
                device.cmd_pipeline_barrier(cmd,
                    vk::PipelineStageFlags::TRANSFER, vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(), &[], &[], &[to_shader]);
            }
        })?;

        self.destroy_buffer(staging);

        // CUBE image view spanning all layers and mip levels.
        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::CUBE)
            .format(format)
            .subresource_range(full_range);

        // SAFETY: device, image, and format are valid.
        let image_view = unsafe { self.device.create_image_view(&view_info, None) }
            .context("Failed to create cubemap image view")?;

        let id = self.next_image_id;
        self.next_image_id += 1;
        self.images.insert(id, ImageEntry { image, image_view, allocation: Some(allocation) });

        log::info!("Cubemap uploaded ({base_face_size}×{base_face_size}, {mip_levels} mips, {format:?})");
        Ok(ImageHandle(id))
    }

    /// Destroy a texture image and free its memory.
    pub fn destroy_image(&mut self, handle: ImageHandle) {
        if let Some(mut entry) = self.images.remove(&handle.0) {
            // SAFETY: device is valid, image/view were created by us.
            unsafe {
                self.device.destroy_image_view(entry.image_view, None);
                self.device.destroy_image(entry.image, None);
            }
            if let Some(alloc) = entry.allocation.take() {
                self.allocator
                    .as_mut()
                    .expect("allocator alive")
                    .free(alloc)
                    .expect("Failed to free image allocation");
            }
        }
    }

    // ---------------------------------------------------------------------------
    // Internal helpers
    // ---------------------------------------------------------------------------

    /// Get the raw vk::Buffer for a handle. Only for use within src/engine/.
    pub(super) fn get_buffer(&self, handle: BufferHandle) -> vk::Buffer {
        self.buffers[&handle.0].buffer
    }

    /// Get the VkImageView for a handle. Only for use within src/engine/.
    pub(super) fn get_image_view(&self, handle: ImageHandle) -> vk::ImageView {
        self.images[&handle.0].image_view
    }

    /// Get the raw VkImage for a handle (needed for image memory barriers).
    /// Only for use within src/engine/.
    pub(super) fn get_image_raw(&self, handle: ImageHandle) -> vk::Image {
        self.images[&handle.0].image
    }

    /// Create a device-local image suitable for use as an attachment (depth or color).
    /// Transitions to the requested `initial_layout` immediately via a one-shot command.
    pub fn create_attachment_image(
        &mut self,
        width: u32,
        height: u32,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
        aspect: vk::ImageAspectFlags,
        initial_layout: vk::ImageLayout,
        dst_stage: vk::PipelineStageFlags,
        dst_access: vk::AccessFlags,
    ) -> Result<ImageHandle> {
        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D { width, height, depth: 1 })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(usage)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        // SAFETY: device is valid.
        let image = unsafe { self.device.create_image(&image_info, None) }
            .context("Failed to create attachment image")?;

        // SAFETY: device and image are valid.
        let requirements = unsafe { self.device.get_image_memory_requirements(image) };

        let allocation = self
            .allocator
            .as_mut()
            .expect("allocator alive")
            .allocate(&AllocationCreateDesc {
                name: "attachment",
                requirements,
                location: MemoryLocation::GpuOnly,
                linear: false,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .context("Failed to allocate attachment image memory")?;

        // SAFETY: image and allocation are valid.
        unsafe {
            self.device
                .bind_image_memory(image, allocation.memory(), allocation.offset())
        }
        .context("Failed to bind attachment image memory")?;

        let subresource_range = vk::ImageSubresourceRange {
            aspect_mask: aspect,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        };

        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(subresource_range);

        // SAFETY: device, image, and format are valid.
        let image_view = unsafe { self.device.create_image_view(&view_info, None) }
            .context("Failed to create attachment image view")?;

        // Transition UNDEFINED → initial_layout once, so each frame the layout is stable.
        self.execute_one_shot(|device, cmd| {
            let barrier = vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(dst_access)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(initial_layout)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(subresource_range);
            // SAFETY: cmd is recording.
            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    dst_stage,
                    vk::DependencyFlags::empty(),
                    &[], &[], &[barrier],
                );
            }
        })?;

        let id = self.next_image_id;
        self.next_image_id += 1;
        self.images.insert(id, ImageEntry { image, image_view, allocation: Some(allocation) });

        log::debug!("Attachment image created ({width}×{height}, {format:?})");
        Ok(ImageHandle(id))
    }

    /// Create a MSAA image (multisampled, no initial layout transition).
    /// The caller is responsible for transitioning via barriers before use.
    pub fn create_msaa_image(
        &mut self,
        width: u32,
        height: u32,
        format: vk::Format,
        samples: vk::SampleCountFlags,
        usage: vk::ImageUsageFlags,
        aspect: vk::ImageAspectFlags,
    ) -> Result<ImageHandle> {
        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D { width, height, depth: 1 })
            .mip_levels(1)
            .array_layers(1)
            .samples(samples)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(usage)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        // SAFETY: device is valid.
        let image = unsafe { self.device.create_image(&image_info, None) }
            .context("Failed to create MSAA image")?;

        // SAFETY: device and image are valid.
        let requirements = unsafe { self.device.get_image_memory_requirements(image) };

        let allocation = self
            .allocator
            .as_mut()
            .expect("allocator alive")
            .allocate(&AllocationCreateDesc {
                name: "msaa",
                requirements,
                location: MemoryLocation::GpuOnly,
                linear: false,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .context("Failed to allocate MSAA image memory")?;

        // SAFETY: image and allocation are valid.
        unsafe {
            self.device.bind_image_memory(image, allocation.memory(), allocation.offset())
        }
        .context("Failed to bind MSAA image memory")?;

        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: aspect,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        // SAFETY: device, image, and format are valid.
        let image_view = unsafe { self.device.create_image_view(&view_info, None) }
            .context("Failed to create MSAA image view")?;

        let id = self.next_image_id;
        self.next_image_id += 1;
        self.images.insert(id, ImageEntry { image, image_view, allocation: Some(allocation) });

        // Transition to the working layout once at creation so the render graph
        // never needs to emit an UNDEFINED old_layout barrier on frame 1+.
        // (Transitioning from UNDEFINED each frame confuses MoltenVK validation.)
        let initial_layout = if aspect.contains(vk::ImageAspectFlags::DEPTH) {
            vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL
        } else {
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
        };
        let dst_stage = if aspect.contains(vk::ImageAspectFlags::DEPTH) {
            vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS
        } else {
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
        };
        let dst_access = if aspect.contains(vk::ImageAspectFlags::DEPTH) {
            vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
        } else {
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE
        };
        self.execute_one_shot(|device, cmd| {
            let barrier = vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(dst_access)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(initial_layout)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: aspect,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });
            // SAFETY: cmd and device are valid for this one-shot buffer.
            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    dst_stage,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[barrier],
                );
            }
        })?;

        log::debug!("MSAA image created ({width}×{height}, {format:?}, {samples:?})");
        Ok(ImageHandle(id))
    }

    /// Record and execute a one-shot command buffer, wait with a fence.
    fn execute_one_shot(&self, record: impl FnOnce(&ash::Device, vk::CommandBuffer)) -> Result<()> {
        let alloc_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(self.transfer_command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);

        // SAFETY: device and pool are valid.
        let cmd = unsafe { self.device.allocate_command_buffers(&alloc_info) }
            .context("Failed to allocate one-shot command buffer")?[0];

        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        // SAFETY: cmd is freshly allocated.
        unsafe {
            self.device.begin_command_buffer(cmd, &begin_info)?;
        }

        record(&self.device, cmd);

        // SAFETY: cmd is recording.
        unsafe {
            self.device.end_command_buffer(cmd)?;
        }

        let cmd_buffers = [cmd];
        let submit_info = vk::SubmitInfo::default().command_buffers(&cmd_buffers);

        let fence_info = vk::FenceCreateInfo::default();
        // SAFETY: device is valid.
        let fence = unsafe { self.device.create_fence(&fence_info, None) }
            .context("Failed to create transfer fence")?;

        // SAFETY: queue, submit_info, and fence are valid.
        unsafe {
            self.device
                .queue_submit(self.queue, &[submit_info], fence)?;
            self.device.wait_for_fences(&[fence], true, u64::MAX)?;
            self.device.destroy_fence(fence, None);
            self.device
                .free_command_buffers(self.transfer_command_pool, &cmd_buffers);
        }

        Ok(())
    }
}
