use std::ffi::{c_char, CStr};

use anyhow::{Context, Result};
use ash::vk;
use bytemuck;
use egui_ash_renderer::{DynamicRendering, Options, Renderer as EguiRenderer};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::Window;

use gpu_allocator::MemoryLocation;

use super::gpu_resources::{BufferHandle, GpuMesh, GpuResourceManager, ImageHandle};
use super::render_graph::{RenderGraph, ResourceAccess};
use super::vertex::Vertex;
use crate::asset::SceneData;
use crate::scene::LightingUbo;

/// Push constant layout (128 bytes = Vulkan minimum).
/// bytes  0..64 = MVP matrix, bytes 64..128 = model matrix.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct MeshPushConstants {
    mvp:   glam::Mat4,
    model: glam::Mat4,
}

const MAX_FRAMES_IN_FLIGHT: usize = 2;

/// HDR render target format for the main pass and bloom chain.
pub const HDR_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;
/// Number of bloom downsample/upsample levels (half-res to 1/32-res).
const BLOOM_LEVELS: usize = 5;

#[allow(dead_code)] // entry keeps Vulkan loaded; queue_family_index used in later phases
pub struct VulkanContext {
    // Stored in creation order — destroyed in reverse (see `destroy()`)
    entry: ash::Entry,
    instance: ash::Instance,
    debug_utils: Option<(ash::ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,
    surface_loader: ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
    physical_device: vk::PhysicalDevice,
    device: ash::Device,
    graphics_queue: vk::Queue,
    queue_family_index: u32,
    resource_manager: Option<GpuResourceManager>,
    gpu_meshes: Vec<GpuMesh>,
    material_sets: Vec<vk::DescriptorSet>,
    // Lighting (set 0)
    lighting_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    descriptor_sets: Vec<vk::DescriptorSet>, // one per frame-in-flight
    ubo_buffers: Vec<BufferHandle>,           // one per frame-in-flight, CpuToGpu
    // Material textures (set 1)
    sampler: vk::Sampler,
    material_set_layout: vk::DescriptorSetLayout,
    material_descriptor_pool: vk::DescriptorPool,
    scene_textures: Vec<ImageHandle>,
    default_albedo: ImageHandle,
    default_normal: ImageHandle,
    default_mr: ImageHandle,
    depth_image: ImageHandle,
    depth_format: vk::Format,
    shadow_map: ImageHandle,
    shadow_sampler: vk::Sampler,
    shadow_pipeline_layout: vk::PipelineLayout,
    shadow_pipeline: vk::Pipeline,
    // IBL images
    ibl_sampler: vk::Sampler,
    skybox_image: ImageHandle,
    irradiance_image: ImageHandle,
    prefiltered_image: ImageHandle,
    brdf_lut_image: ImageHandle,
    // Skybox pipeline + descriptor set (static, not per-frame)
    skybox_set_layout: vk::DescriptorSetLayout,
    skybox_descriptor_pool: vk::DescriptorPool,
    skybox_descriptor_set: vk::DescriptorSet,
    skybox_pipeline_layout: vk::PipelineLayout,
    skybox_pipeline: vk::Pipeline,
    wireframe_pipeline_layout: vk::PipelineLayout,
    wireframe_pipeline: vk::Pipeline,
    wireframe_vertex_buffers: Vec<BufferHandle>,
    gizmo_vertex_buffers: Vec<BufferHandle>,
    pipeline_layout: vk::PipelineLayout,
    graphics_pipeline: vk::Pipeline,
    swapchain_loader: ash::khr::swapchain::Device,
    dynamic_rendering_loader: ash::khr::dynamic_rendering::Device,
    swapchain: vk::SwapchainKHR,
    swapchain_images: Vec<vk::Image>,
    swapchain_image_views: Vec<vk::ImageView>,
    swapchain_format: vk::Format,
    swapchain_extent: vk::Extent2D,
    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,
    image_available_semaphores: Vec<vk::Semaphore>,
    render_finished_semaphores: Vec<vk::Semaphore>,
    in_flight_fences: Vec<vk::Fence>,
    current_frame: usize,
    acquire_semaphore_index: usize,
    pub framebuffer_resized: bool,
    // MSAA
    msaa_samples: vk::SampleCountFlags,
    msaa_max: vk::SampleCountFlags,
    msaa_color: Option<ImageHandle>,
    msaa_depth: Option<ImageHandle>,
    // HDR render target (main pass output, input to bloom/composite)
    hdr_color: ImageHandle,
    // Bloom post-processing
    bloom_sampler: vk::Sampler,
    bloom_images: Vec<ImageHandle>,          // BLOOM_LEVELS images, half-res to 1/32-res
    bloom_extents: Vec<vk::Extent2D>,        // extents for each bloom level
    bloom_set_layout: vk::DescriptorSetLayout,
    bloom_descriptor_pool: vk::DescriptorPool,
    bloom_downsample_sets: Vec<vk::DescriptorSet>,  // [BLOOM_LEVELS]: ds[0]=hdr, ds[i]=bloom[i-1]
    bloom_upsample_sets: Vec<vk::DescriptorSet>,    // [BLOOM_LEVELS-1]: us[i] reads bloom[i+1]
    composite_set_layout: vk::DescriptorSetLayout,
    composite_descriptor_pool: vk::DescriptorPool,
    composite_descriptor_set: vk::DescriptorSet,
    bloom_downsample_pipeline_layout: vk::PipelineLayout,
    bloom_downsample_pipeline: vk::Pipeline,
    bloom_upsample_pipeline_layout: vk::PipelineLayout,
    bloom_upsample_pipeline: vk::Pipeline,
    composite_pipeline_layout: vk::PipelineLayout,
    composite_pipeline: vk::Pipeline,
    // egui renderer (Option so we can take+drop it before device destruction)
    egui_renderer: Option<EguiRenderer>,
    // Per-frame-in-flight lists of egui texture IDs to free after the GPU finishes.
    egui_textures_to_free: [Vec<egui::TextureId>; MAX_FRAMES_IN_FLIGHT],
}

// ---------------------------------------------------------------------------
// Init helpers (free functions, each does one thing)
// ---------------------------------------------------------------------------

fn create_instance(entry: &ash::Entry, window: &Window) -> Result<ash::Instance> {
    let app_info = vk::ApplicationInfo::default()
        .application_name(c"ralk")
        .application_version(vk::make_api_version(0, 0, 1, 0))
        .engine_name(c"ralk")
        .engine_version(vk::make_api_version(0, 0, 1, 0))
        .api_version(vk::API_VERSION_1_2);

    let display_handle = window
        .display_handle()
        .map_err(|e| anyhow::anyhow!("Display handle unavailable: {e}"))?;

    let mut extensions = ash_window::enumerate_required_extensions(display_handle.as_raw())
        .context("Failed to enumerate required surface extensions")?
        .to_vec();

    if cfg!(debug_assertions) {
        extensions.push(ash::ext::debug_utils::NAME.as_ptr());
    }

    // macOS: MoltenVK requires portability enumeration to be discoverable.
    #[cfg(target_os = "macos")]
    {
        extensions.push(c"VK_KHR_portability_enumeration".as_ptr());
    }

    // Only enable validation layer if actually installed.
    // SAFETY: entry is valid.
    let available_layers = unsafe { entry.enumerate_instance_layer_properties()? };
    let has_validation = cfg!(debug_assertions)
        && available_layers.iter().any(|layer| {
            // SAFETY: layer_name is a null-terminated fixed-size array from the loader.
            let name = unsafe { CStr::from_ptr(layer.layer_name.as_ptr()) };
            name == c"VK_LAYER_KHRONOS_validation"
        });

    if cfg!(debug_assertions) && !has_validation {
        log::warn!("Validation layers not available — install Vulkan SDK for debug diagnostics");
    }

    let layer_names: Vec<*const c_char> = if has_validation {
        vec![c"VK_LAYER_KHRONOS_validation".as_ptr()]
    } else {
        vec![]
    };

    let mut flags = vk::InstanceCreateFlags::empty();
    #[cfg(target_os = "macos")]
    {
        flags |= vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR;
    }

    let create_info = vk::InstanceCreateInfo::default()
        .flags(flags)
        .application_info(&app_info)
        .enabled_extension_names(&extensions)
        .enabled_layer_names(&layer_names);

    // SAFETY: entry is valid, create_info references live data on the stack.
    let instance = unsafe { entry.create_instance(&create_info, None)? };

    log::info!("Vulkan instance created (API 1.2)");
    Ok(instance)
}

fn setup_debug_messenger(
    entry: &ash::Entry,
    instance: &ash::Instance,
) -> Option<(ash::ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)> {
    if !cfg!(debug_assertions) {
        return None;
    }

    let loader = ash::ext::debug_utils::Instance::new(entry, instance);

    let create_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
        .message_severity(
            vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
        )
        .message_type(
            vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
        )
        .pfn_user_callback(Some(vulkan_debug_callback));

    // SAFETY: instance is valid, create_info is well-formed.
    let messenger = match unsafe { loader.create_debug_utils_messenger(&create_info, None) } {
        Ok(m) => m,
        Err(e) => {
            log::warn!("Failed to create debug messenger: {e}");
            return None;
        }
    };

    log::info!("Debug messenger enabled (validation layers active)");
    Some((loader, messenger))
}

unsafe extern "system" fn vulkan_debug_callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _user_data: *mut std::ffi::c_void,
) -> vk::Bool32 {
    // SAFETY: callback_data is guaranteed non-null by the Vulkan spec.
    let message = unsafe { CStr::from_ptr((*callback_data).p_message) }.to_string_lossy();

    if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) {
        log::error!("[Vulkan] {message}");
    } else if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::WARNING) {
        log::warn!("[Vulkan] {message}");
    } else {
        log::debug!("[Vulkan] {message}");
    }

    vk::FALSE
}

fn create_surface(
    entry: &ash::Entry,
    instance: &ash::Instance,
    window: &Window,
) -> Result<(ash::khr::surface::Instance, vk::SurfaceKHR)> {
    let surface_loader = ash::khr::surface::Instance::new(entry, instance);

    let display_handle = window
        .display_handle()
        .map_err(|e| anyhow::anyhow!("Display handle unavailable: {e}"))?;
    let window_handle = window
        .window_handle()
        .map_err(|e| anyhow::anyhow!("Window handle unavailable: {e}"))?;

    // SAFETY: entry, instance, and window handles are valid and outlive the surface.
    let surface = unsafe {
        ash_window::create_surface(
            entry,
            instance,
            display_handle.as_raw(),
            window_handle.as_raw(),
            None,
        )?
    };

    log::info!("Vulkan surface created");
    Ok((surface_loader, surface))
}

/// Returns (physical_device, queue_family_index, supports_vulkan_13).
fn select_physical_device(
    instance: &ash::Instance,
    surface_loader: &ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
) -> Result<(vk::PhysicalDevice, u32, bool)> {
    // SAFETY: instance is valid.
    let devices = unsafe { instance.enumerate_physical_devices()? };

    let mut best: Option<(vk::PhysicalDevice, u32, bool)> = None;
    let mut best_is_discrete = false;

    for device in devices {
        // SAFETY: instance and device are valid.
        let props = unsafe { instance.get_physical_device_properties(device) };

        // Require Vulkan 1.2 minimum.
        if props.api_version < vk::API_VERSION_1_2 {
            continue;
        }

        // SAFETY: instance and device are valid.
        let extensions =
            unsafe { instance.enumerate_device_extension_properties(device)? };

        let has_extension = |name: &CStr| -> bool {
            extensions.iter().any(|ext| {
                // SAFETY: extension_name is a null-terminated fixed-size array from the driver.
                let ext_name = unsafe { CStr::from_ptr(ext.extension_name.as_ptr()) };
                ext_name == name
            })
        };

        // VK_KHR_swapchain is always required.
        if !has_extension(ash::khr::swapchain::NAME) {
            continue;
        }

        // Dynamic rendering: core in 1.3, or via extension on 1.2 (MoltenVK).
        let supports_13 = props.api_version >= vk::API_VERSION_1_3;
        if !supports_13 && !has_extension(ash::khr::dynamic_rendering::NAME) {
            continue;
        }

        // Find a queue family with graphics + present support.
        // SAFETY: instance and device are valid.
        let queue_families =
            unsafe { instance.get_physical_device_queue_family_properties(device) };

        let queue_family = queue_families.iter().enumerate().find_map(|(i, qf)| {
            let graphics = qf.queue_flags.contains(vk::QueueFlags::GRAPHICS);
            // SAFETY: device, surface, and surface_loader are valid.
            let present = unsafe {
                surface_loader
                    .get_physical_device_surface_support(device, i as u32, surface)
                    .unwrap_or(false)
            };
            if graphics && present {
                Some(i as u32)
            } else {
                None
            }
        });

        let Some(queue_family_index) = queue_family else {
            continue;
        };

        let is_discrete = props.device_type == vk::PhysicalDeviceType::DISCRETE_GPU;

        if best.is_none() || (is_discrete && !best_is_discrete) {
            // SAFETY: device_name is a null-terminated fixed-size array.
            let name = unsafe { CStr::from_ptr(props.device_name.as_ptr()) };
            let device_type = match props.device_type {
                vk::PhysicalDeviceType::DISCRETE_GPU => "Discrete",
                vk::PhysicalDeviceType::INTEGRATED_GPU => "Integrated",
                vk::PhysicalDeviceType::VIRTUAL_GPU => "Virtual",
                vk::PhysicalDeviceType::CPU => "CPU",
                _ => "Unknown",
            };
            log::info!("Selected GPU: {} ({device_type})", name.to_string_lossy());
            best = Some((device, queue_family_index, supports_13));
            best_is_discrete = is_discrete;
        }
    }

    best.context("No suitable Vulkan 1.2+ GPU found (need swapchain + dynamic rendering)")
}

fn create_logical_device(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    queue_family_index: u32,
    supports_vulkan_13: bool,
) -> Result<(ash::Device, vk::Queue)> {
    let queue_priority = [1.0f32];
    let queue_create_info = vk::DeviceQueueCreateInfo::default()
        .queue_family_index(queue_family_index)
        .queue_priorities(&queue_priority);
    let queue_create_infos = [queue_create_info];

    // VK_KHR_swapchain always required. VK_KHR_dynamic_rendering needed on Vulkan 1.2.
    let mut extension_names = vec![ash::khr::swapchain::NAME.as_ptr()];
    if !supports_vulkan_13 {
        extension_names.push(ash::khr::dynamic_rendering::NAME.as_ptr());
    }

    // MoltenVK: must enable portability_subset if the device advertises it.
    // SAFETY: instance and physical_device are valid.
    let device_extensions =
        unsafe { instance.enumerate_device_extension_properties(physical_device)? };
    let has_portability = device_extensions.iter().any(|ext| {
        // SAFETY: extension_name is a null-terminated fixed-size array.
        let name = unsafe { CStr::from_ptr(ext.extension_name.as_ptr()) };
        name == c"VK_KHR_portability_subset"
    });
    if has_portability {
        extension_names.push(c"VK_KHR_portability_subset".as_ptr());
    }

    // Enable dynamic rendering: Vulkan13Features for 1.3, extension features for 1.2.
    let mut dynamic_rendering_features =
        vk::PhysicalDeviceDynamicRenderingFeatures::default().dynamic_rendering(true);
    let mut vulkan_13_features =
        vk::PhysicalDeviceVulkan13Features::default().dynamic_rendering(true);

    let device_create_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queue_create_infos)
        .enabled_extension_names(&extension_names);

    // Vulkan spec: do NOT chain both Vulkan13Features and DynamicRenderingFeatures.
    let device_create_info = if supports_vulkan_13 {
        device_create_info.push_next(&mut vulkan_13_features)
    } else {
        device_create_info.push_next(&mut dynamic_rendering_features)
    };

    // SAFETY: instance, physical_device are valid; create_info references live data.
    let device =
        unsafe { instance.create_device(physical_device, &device_create_info, None)? };

    // SAFETY: device is valid, queue family/index are correct.
    let queue = unsafe { device.get_device_queue(queue_family_index, 0) };

    log::info!(
        "Logical device created (queue family {queue_family_index}, {})",
        if supports_vulkan_13 {
            "Vulkan 1.3"
        } else {
            "Vulkan 1.2 + KHR extensions"
        }
    );
    Ok((device, queue))
}

fn create_swapchain(
    swapchain_loader: &ash::khr::swapchain::Device,
    physical_device: vk::PhysicalDevice,
    surface_loader: &ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
    window: &Window,
    old_swapchain: vk::SwapchainKHR,
) -> Result<(vk::SwapchainKHR, Vec<vk::Image>, vk::Format, vk::Extent2D)> {
    // SAFETY: physical_device, surface, and surface_loader are valid.
    let capabilities = unsafe {
        surface_loader.get_physical_device_surface_capabilities(physical_device, surface)?
    };
    let formats = unsafe {
        surface_loader.get_physical_device_surface_formats(physical_device, surface)?
    };

    // Prefer BGRA8_SRGB + SRGB_NONLINEAR, fall back to first available
    let format = formats
        .iter()
        .find(|f| {
            f.format == vk::Format::B8G8R8A8_SRGB
                && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
        })
        .or_else(|| formats.first())
        .context("No surface formats available")?;

    // Extent: use current if defined, otherwise derive from window size
    let extent = if capabilities.current_extent.width != u32::MAX {
        capabilities.current_extent
    } else {
        let size = window.inner_size();
        vk::Extent2D {
            width: size.width.clamp(
                capabilities.min_image_extent.width,
                capabilities.max_image_extent.width,
            ),
            height: size.height.clamp(
                capabilities.min_image_extent.height,
                capabilities.max_image_extent.height,
            ),
        }
    };

    // Triple buffering: request min + 1, cap at max (0 = unlimited)
    let mut image_count = capabilities.min_image_count + 1;
    if capabilities.max_image_count > 0 && image_count > capabilities.max_image_count {
        image_count = capabilities.max_image_count;
    }

    let create_info = vk::SwapchainCreateInfoKHR::default()
        .surface(surface)
        .min_image_count(image_count)
        .image_format(format.format)
        .image_color_space(format.color_space)
        .image_extent(extent)
        .image_array_layers(1)
        .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
        .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
        .pre_transform(capabilities.current_transform)
        .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
        .present_mode(vk::PresentModeKHR::FIFO)
        .clipped(true)
        .old_swapchain(old_swapchain);

    // SAFETY: swapchain_loader wraps a valid device, create_info is well-formed.
    let swapchain = unsafe { swapchain_loader.create_swapchain(&create_info, None)? };
    let images = unsafe { swapchain_loader.get_swapchain_images(swapchain)? };

    log::info!(
        "Swapchain created: {}x{} ({} images, {:?})",
        extent.width,
        extent.height,
        images.len(),
        format.format,
    );

    Ok((swapchain, images, format.format, extent))
}

fn create_image_views(
    device: &ash::Device,
    images: &[vk::Image],
    format: vk::Format,
) -> Result<Vec<vk::ImageView>> {
    images
        .iter()
        .map(|&image| {
            let create_info = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format)
                .components(vk::ComponentMapping {
                    r: vk::ComponentSwizzle::IDENTITY,
                    g: vk::ComponentSwizzle::IDENTITY,
                    b: vk::ComponentSwizzle::IDENTITY,
                    a: vk::ComponentSwizzle::IDENTITY,
                })
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });

            // SAFETY: device and image are valid.
            unsafe { device.create_image_view(&create_info, None) }.map_err(Into::into)
        })
        .collect()
}

fn create_command_pool_and_buffers(
    device: &ash::Device,
    queue_family_index: u32,
) -> Result<(vk::CommandPool, Vec<vk::CommandBuffer>)> {
    let pool_info = vk::CommandPoolCreateInfo::default()
        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
        .queue_family_index(queue_family_index);

    // SAFETY: device is valid.
    let pool = unsafe { device.create_command_pool(&pool_info, None)? };

    let alloc_info = vk::CommandBufferAllocateInfo::default()
        .command_pool(pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(MAX_FRAMES_IN_FLIGHT as u32);

    // SAFETY: device and pool are valid.
    let buffers = unsafe { device.allocate_command_buffers(&alloc_info)? };

    log::info!(
        "Command pool created ({} command buffers)",
        buffers.len()
    );
    Ok((pool, buffers))
}

fn create_sync_objects(
    device: &ash::Device,
    swapchain_image_count: usize,
) -> Result<(Vec<vk::Semaphore>, Vec<vk::Semaphore>, Vec<vk::Fence>)> {
    let semaphore_info = vk::SemaphoreCreateInfo::default();
    // Start signaled so the first frame doesn't block on wait_for_fences.
    let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);

    // One acquire semaphore per swapchain image — avoids VUID-vkQueueSubmit-pSignalSemaphores-00067
    // when the presentation engine still holds a semaphore from a prior acquire.
    let mut image_available = Vec::with_capacity(swapchain_image_count);
    for _ in 0..swapchain_image_count {
        // SAFETY: device is valid, create info is well-formed.
        unsafe {
            image_available.push(device.create_semaphore(&semaphore_info, None)?);
        }
    }

    // One render-finished semaphore per swapchain image — the presentation engine
    // holds the semaphore until present completes, so we need one per image to avoid
    // signaling a semaphore that's still in use by a prior present.
    let mut render_finished = Vec::with_capacity(swapchain_image_count);
    for _ in 0..swapchain_image_count {
        // SAFETY: device is valid, create info is well-formed.
        unsafe {
            render_finished.push(device.create_semaphore(&semaphore_info, None)?);
        }
    }

    // Fences: one per frame-in-flight (protects command buffer reuse).
    let mut in_flight = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
    for _ in 0..MAX_FRAMES_IN_FLIGHT {
        // SAFETY: device is valid, create info is well-formed.
        unsafe {
            in_flight.push(device.create_fence(&fence_info, None)?);
        }
    }

    log::info!(
        "Sync objects created ({swapchain_image_count} semaphore pairs, {MAX_FRAMES_IN_FLIGHT} fences)"
    );
    Ok((image_available, render_finished, in_flight))
}

/// Return the best supported depth format for a depth-only attachment.
fn find_depth_format(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
) -> vk::Format {
    let candidates = [
        vk::Format::D32_SFLOAT,
        vk::Format::D32_SFLOAT_S8_UINT,
        vk::Format::D24_UNORM_S8_UINT,
    ];
    for format in candidates {
        // SAFETY: instance and physical_device are valid.
        let props =
            unsafe { instance.get_physical_device_format_properties(physical_device, format) };
        if props
            .optimal_tiling_features
            .contains(vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT)
        {
            return format;
        }
    }
    panic!("No supported depth format found on this GPU");
}

/// Depth aspect flags: D32_SFLOAT is depth-only; formats with stencil need both bits.
fn depth_aspect(format: vk::Format) -> vk::ImageAspectFlags {
    if format == vk::Format::D32_SFLOAT {
        vk::ImageAspectFlags::DEPTH
    } else {
        vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL
    }
}

/// Query the max MSAA sample count supported for both color and depth attachments.
fn max_msaa_samples(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
) -> vk::SampleCountFlags {
    // SAFETY: instance and physical_device are valid.
    let props = unsafe { instance.get_physical_device_properties(physical_device) };
    let color_depth = props.limits.framebuffer_color_sample_counts
        & props.limits.framebuffer_depth_sample_counts;
    for &candidate in &[
        vk::SampleCountFlags::TYPE_8,
        vk::SampleCountFlags::TYPE_4,
        vk::SampleCountFlags::TYPE_2,
    ] {
        if color_depth.contains(candidate) {
            return candidate;
        }
    }
    vk::SampleCountFlags::TYPE_1
}

// ---------------------------------------------------------------------------
// VulkanContext — public API
// ---------------------------------------------------------------------------

/// Write a single material descriptor set (3 COMBINED_IMAGE_SAMPLER bindings).
/// Shared by `VulkanContext::new` and `VulkanContext::reload_scene`.
fn write_material_set(
    device: &ash::Device,
    set: vk::DescriptorSet,
    sampler: vk::Sampler,
    rm: &super::gpu_resources::GpuResourceManager,
    albedo: super::gpu_resources::ImageHandle,
    normal: super::gpu_resources::ImageHandle,
    mr: super::gpu_resources::ImageHandle,
) {
    let image_infos = [
        vk::DescriptorImageInfo::default()
            .sampler(sampler)
            .image_view(rm.get_image_view(albedo))
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL),
        vk::DescriptorImageInfo::default()
            .sampler(sampler)
            .image_view(rm.get_image_view(normal))
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL),
        vk::DescriptorImageInfo::default()
            .sampler(sampler)
            .image_view(rm.get_image_view(mr))
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL),
    ];
    let writes: Vec<vk::WriteDescriptorSet> = (0u32..3)
        .map(|binding| {
            vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(binding)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&image_infos[binding as usize..binding as usize + 1])
        })
        .collect();
    // SAFETY: set, image views, and sampler are all valid.
    unsafe { device.update_descriptor_sets(&writes, &[]) };
}

impl VulkanContext {
    pub fn new(window: &Window, scene_data: &SceneData) -> Result<Self> {
        // SAFETY: loads libvulkan dynamically at runtime (MoltenVK on macOS, loader on Linux).
        let entry = unsafe { ash::Entry::load() }.context(
            "No se encontró libvulkan. \
             En macOS: instalar Vulkan SDK de LunarG. \
             En Linux: instalar mesa-vulkan-drivers o nvidia-driver.",
        )?;
        let instance = create_instance(&entry, window)?;
        let debug_utils = setup_debug_messenger(&entry, &instance);
        let (surface_loader, surface) = create_surface(&entry, &instance, window)?;
        let (physical_device, queue_family_index, supports_vulkan_13) =
            select_physical_device(&instance, &surface_loader, surface)?;
        let (device, graphics_queue) =
            create_logical_device(&instance, physical_device, queue_family_index, supports_vulkan_13)?;
        let swapchain_loader = ash::khr::swapchain::Device::new(&instance, &device);
        // KHR loader works on both 1.2+ext and 1.3 (promoted functions are aliased).
        let dynamic_rendering_loader =
            ash::khr::dynamic_rendering::Device::new(&instance, &device);
        let (swapchain, swapchain_images, swapchain_format, swapchain_extent) =
            create_swapchain(
                &swapchain_loader,
                physical_device,
                &surface_loader,
                surface,
                window,
                vk::SwapchainKHR::null(),
            )?;
        let swapchain_image_views =
            create_image_views(&device, &swapchain_images, swapchain_format)?;

        // GPU resource manager
        let mut resource_manager = GpuResourceManager::new(
            &instance,
            device.clone(),
            physical_device,
            graphics_queue,
            queue_family_index,
        )?;

        // Query MSAA support — choose 4x if available, else 2x, else off.
        let msaa_max = max_msaa_samples(&instance, physical_device);
        let msaa_samples = if msaa_max.contains(vk::SampleCountFlags::TYPE_4) {
            vk::SampleCountFlags::TYPE_4
        } else if msaa_max.contains(vk::SampleCountFlags::TYPE_2) {
            vk::SampleCountFlags::TYPE_2
        } else {
            vk::SampleCountFlags::TYPE_1
        };
        log::info!("MSAA: {:?} selected (max supported: {:?})", msaa_samples, msaa_max);

        // --- Shared sampler (linear, repeat, all mip levels) ---
        // max_lod=1000.0 ensures all generated mip levels are accessible.
        let sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::REPEAT)
            .address_mode_v(vk::SamplerAddressMode::REPEAT)
            .address_mode_w(vk::SamplerAddressMode::REPEAT)
            .anisotropy_enable(false)
            .min_lod(0.0)
            .max_lod(1000.0); // covers all mip levels for any texture size
        // SAFETY: device is valid, sampler_info is well-formed.
        let sampler = unsafe { device.create_sampler(&sampler_info, None) }
            .context("Failed to create sampler")?;

        // --- Default 1×1 fallback textures ---
        // white albedo (sRGB), flat normal [128,128,255,255], default MR [0,128,0,255] (roughness=0.5)
        let default_albedo = resource_manager.upload_texture(
            &[255, 255, 255, 255], 1, 1, vk::Format::R8G8B8A8_SRGB)?;
        let default_normal = resource_manager.upload_texture(
            &[128, 128, 255, 255], 1, 1, vk::Format::R8G8B8A8_UNORM)?;
        let default_mr = resource_manager.upload_texture(
            &[0, 128, 0, 255], 1, 1, vk::Format::R8G8B8A8_UNORM)?;

        // --- Upload scene textures ---
        let mut scene_textures: Vec<ImageHandle> =
            Vec::with_capacity(scene_data.textures.len());
        for tex in &scene_data.textures {
            let format = if tex.is_srgb {
                vk::Format::R8G8B8A8_SRGB
            } else {
                vk::Format::R8G8B8A8_UNORM
            };
            scene_textures.push(
                resource_manager.upload_texture(&tex.pixels, tex.width, tex.height, format)?,
            );
        }

        // --- Depth buffer ---
        let depth_format = find_depth_format(&instance, physical_device);
        let depth_image = resource_manager.create_attachment_image(
            swapchain_extent.width,
            swapchain_extent.height,
            depth_format,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            depth_aspect(depth_format),
            vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
            vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
            vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
        )?;
        log::info!("Depth buffer created ({:?})", depth_format);

        // --- Shadow map (fixed 2048×2048, D32_SFLOAT depth-only) ---
        // Starts in SHADER_READ_ONLY_OPTIMAL so the first frame's barrier transition is
        // consistent (SHADER_READ_ONLY → DEPTH_STENCIL_ATTACHMENT, then back each frame).
        let shadow_map = resource_manager.create_attachment_image(
            2048,
            2048,
            depth_format,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
            vk::ImageAspectFlags::DEPTH,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::AccessFlags::SHADER_READ,
        )?;

        // Shadow sampler: regular (no comparison) — MoltenVK does not support
        // mutableComparisonSamplers (VK_KHR_portability_subset limitation on macOS).
        // Depth comparison is done manually in the fragment shader.
        // CLAMP_TO_BORDER + OPAQUE_WHITE: areas outside the shadow frustum appear fully lit.
        let shadow_sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::NEAREST)
            .min_filter(vk::Filter::NEAREST)
            .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_BORDER)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_BORDER)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_BORDER)
            .border_color(vk::BorderColor::FLOAT_OPAQUE_WHITE)
            .min_lod(0.0)
            .max_lod(0.0);
        // SAFETY: device is valid, sampler_info is well-formed.
        let shadow_sampler = unsafe { device.create_sampler(&shadow_sampler_info, None) }
            .context("Failed to create shadow sampler")?;
        log::info!("Shadow map created (2048×2048, {:?})", depth_format);

        // --- IBL precomputation (CPU) + GPU upload ---
        let env = super::skybox::load_environment("assets/sky.hdr");
        let ibl = super::skybox::precompute_ibl(&env);

        // Sampler shared by all IBL images: linear, clamp-to-edge, mips 0..max_prefiltered.
        let ibl_sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .min_lod(0.0)
            .max_lod((super::skybox::PREFILTERED_MIP_LEVELS - 1) as f32);
        // SAFETY: device is valid.
        let ibl_sampler = unsafe { device.create_sampler(&ibl_sampler_info, None) }
            .context("Failed to create IBL sampler")?;

        let skybox_mip_data = vec![ibl.skybox_faces];
        let skybox_image = resource_manager.upload_cubemap_mips(
            &skybox_mip_data,
            super::skybox::SKYBOX_FACE_SIZE,
            vk::Format::R16G16B16A16_SFLOAT,
        )?;

        let irr_mip_data = vec![ibl.irr_faces];
        let irradiance_image = resource_manager.upload_cubemap_mips(
            &irr_mip_data,
            super::skybox::IRRADIANCE_FACE_SIZE,
            vk::Format::R16G16B16A16_SFLOAT,
        )?;

        let prefiltered_image = resource_manager.upload_cubemap_mips(
            &ibl.pre_faces,
            super::skybox::PREFILTERED_FACE_SIZE,
            vk::Format::R16G16B16A16_SFLOAT,
        )?;

        let brdf_lut_image = resource_manager.upload_image_raw(
            &ibl.brdf_lut,
            super::skybox::BRDF_LUT_SIZE,
            super::skybox::BRDF_LUT_SIZE,
            vk::Format::R16G16_SFLOAT,
        )?;
        log::info!("IBL images uploaded to GPU");

        // --- Descriptor set layouts ---
        let lighting_set_layout =
            super::pipeline::create_lighting_descriptor_set_layout(&device)?;
        let skybox_set_layout =
            super::pipeline::create_skybox_descriptor_set_layout(&device)?;
        let material_set_layout =
            super::pipeline::create_material_descriptor_set_layout(&device)?;

        let binding_descriptions = [Vertex::binding_description()];
        let attribute_descriptions = Vertex::attribute_descriptions();
        let (pipeline_layout, graphics_pipeline) =
            super::pipeline::create_graphics_pipeline(
                &device,
                super::pipeline::VERT_SPV,
                super::pipeline::FRAG_SPV,
                HDR_FORMAT,
                depth_format,
                &binding_descriptions,
                &attribute_descriptions,
                lighting_set_layout,
                material_set_layout,
                msaa_samples,
            )?;
        let (shadow_pipeline_layout, shadow_pipeline) =
            super::pipeline::create_shadow_pipeline(
                &device,
                super::pipeline::SHADOW_VERT_SPV,
                super::pipeline::SHADOW_FRAG_SPV,
                depth_format,
                &binding_descriptions,
                &attribute_descriptions,
            )?;
        let (skybox_pipeline_layout, skybox_pipeline) =
            super::pipeline::create_skybox_pipeline(
                &device,
                super::pipeline::SKYBOX_VERT_SPV,
                super::pipeline::SKYBOX_FRAG_SPV,
                HDR_FORMAT,
                depth_format,
                skybox_set_layout,
                msaa_samples,
            )?;

        // Wireframe debug pipeline (LINE_LIST, no depth test).
        let wf_binding = [super::vertex::WireframeVertex::binding_description()];
        let wf_attributes = super::vertex::WireframeVertex::attribute_descriptions();
        let (wireframe_pipeline_layout, wireframe_pipeline) =
            super::pipeline::create_wireframe_pipeline(&device, swapchain_format, &wf_binding, &wf_attributes)?;

        // Per-frame wireframe vertex buffers (CpuToGpu, 64 KiB each ≈ 5461 vertices).
        const WIREFRAME_BUFFER_SIZE: u64 = 64 * 1024;
        let mut wireframe_vertex_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            let buf = resource_manager.create_buffer(
                WIREFRAME_BUFFER_SIZE,
                vk::BufferUsageFlags::VERTEX_BUFFER,
                MemoryLocation::CpuToGpu,
            )?;
            wireframe_vertex_buffers.push(buf);
        }

        // Per-frame gizmo vertex buffers (8 KiB each — gizmo geometry is tiny).
        let mut gizmo_vertex_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            let buf = resource_manager.create_buffer(
                8 * 1024,
                vk::BufferUsageFlags::VERTEX_BUFFER,
                MemoryLocation::CpuToGpu,
            )?;
            gizmo_vertex_buffers.push(buf);
        }

        // UBO buffers: one per frame, HOST_VISIBLE so we can update every frame.
        let ubo_size = std::mem::size_of::<LightingUbo>() as u64;
        let mut ubo_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            let buf = resource_manager.create_buffer(
                ubo_size,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
                MemoryLocation::CpuToGpu,
            )?;
            ubo_buffers.push(buf);
        }

        // --- Lighting descriptor pool (set 0): UBO + 4 image samplers per frame ---
        // Bindings: 0=UBO, 1=shadow, 2=irradiance, 3=prefiltered, 4=BRDF LUT
        let lighting_pool_sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UNIFORM_BUFFER,
                descriptor_count: MAX_FRAMES_IN_FLIGHT as u32,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                descriptor_count: (MAX_FRAMES_IN_FLIGHT * 4) as u32,
            },
        ];
        let lighting_pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&lighting_pool_sizes)
            .max_sets(MAX_FRAMES_IN_FLIGHT as u32);
        // SAFETY: device is valid, pool_info is well-formed.
        let descriptor_pool =
            unsafe { device.create_descriptor_pool(&lighting_pool_info, None) }
                .context("Failed to create lighting descriptor pool")?;

        // Allocate one descriptor set per frame-in-flight.
        let lighting_layouts = vec![lighting_set_layout; MAX_FRAMES_IN_FLIGHT];
        let lighting_alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&lighting_layouts);
        // SAFETY: pool and layout are valid.
        let descriptor_sets =
            unsafe { device.allocate_descriptor_sets(&lighting_alloc_info) }
                .context("Failed to allocate lighting descriptor sets")?;

        // Point each descriptor set at its UBO + all image samplers.
        // Bindings: 0=UBO, 1=shadow, 2=irradiance, 3=prefiltered, 4=BRDF LUT.
        // IBL images are static (same view for all frames); only the UBO changes per frame.
        let shadow_view      = resource_manager.get_image_view(shadow_map);
        let irr_view         = resource_manager.get_image_view(irradiance_image);
        let pre_view         = resource_manager.get_image_view(prefiltered_image);
        let brdf_view        = resource_manager.get_image_view(brdf_lut_image);
        for (i, &set) in descriptor_sets.iter().enumerate() {
            let buffer = resource_manager.get_buffer(ubo_buffers[i]);
            let buffer_info = [vk::DescriptorBufferInfo { buffer, offset: 0, range: ubo_size }];
            let shadow_info = [vk::DescriptorImageInfo {
                sampler: shadow_sampler, image_view: shadow_view,
                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            }];
            let irr_info = [vk::DescriptorImageInfo {
                sampler: ibl_sampler, image_view: irr_view,
                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            }];
            let pre_info = [vk::DescriptorImageInfo {
                sampler: ibl_sampler, image_view: pre_view,
                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            }];
            let brdf_info = [vk::DescriptorImageInfo {
                sampler: ibl_sampler, image_view: brdf_view,
                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            }];
            let writes = [
                vk::WriteDescriptorSet::default()
                    .dst_set(set).dst_binding(0)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&buffer_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(set).dst_binding(1)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&shadow_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(set).dst_binding(2)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&irr_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(set).dst_binding(3)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&pre_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(set).dst_binding(4)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&brdf_info),
            ];
            // SAFETY: set, buffer, image views, and samplers are all valid.
            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }
        log::info!("Lighting descriptor sets created ({MAX_FRAMES_IN_FLIGHT} frames, 5 bindings each)");

        // --- Material descriptor pool (set 1): one set per material + 1 default ---
        // Each set has 3 COMBINED_IMAGE_SAMPLER bindings.
        let num_material_sets = scene_data.materials.len() + 1; // +1 for default
        let material_pool_sizes = [vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: (num_material_sets * 3) as u32,
        }];
        let material_pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&material_pool_sizes)
            .max_sets(num_material_sets as u32);
        // SAFETY: device is valid.
        let material_descriptor_pool =
            unsafe { device.create_descriptor_pool(&material_pool_info, None) }
                .context("Failed to create material descriptor pool")?;

        // Allocate all material sets (N materials + 1 default at the end).
        let material_layouts = vec![material_set_layout; num_material_sets];
        let material_alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(material_descriptor_pool)
            .set_layouts(&material_layouts);
        let material_sets =
            unsafe { device.allocate_descriptor_sets(&material_alloc_info) }
                .context("Failed to allocate material descriptor sets")?;

        // Helper: resolve a texture index to the right ImageHandle (or the default).
        let resolve_tex = |idx: Option<usize>, default: ImageHandle| -> ImageHandle {
            idx.and_then(|i| scene_textures.get(i).copied()).unwrap_or(default)
        };

        // Write descriptor sets for each glTF material (shared helper).
        for (i, mat) in scene_data.materials.iter().enumerate() {
            write_material_set(
                &device, material_sets[i], sampler, &resource_manager,
                resolve_tex(mat.albedo_tex, default_albedo),
                resolve_tex(mat.normal_tex, default_normal),
                resolve_tex(mat.metallic_roughness_tex, default_mr),
            );
        }

        // Write the default material set (last one) — all-default textures.
        let default_idx = num_material_sets - 1;
        write_material_set(
            &device, material_sets[default_idx], sampler, &resource_manager,
            default_albedo, default_normal, default_mr,
        );
        log::info!("Material descriptor sets created ({} materials + 1 default)", scene_data.materials.len());

        // --- Skybox descriptor pool + set (1 COMBINED_IMAGE_SAMPLER for the skybox cubemap) ---
        let skybox_pool_sizes = [vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: 1,
        }];
        let skybox_pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&skybox_pool_sizes)
            .max_sets(1);
        // SAFETY: device is valid.
        let skybox_descriptor_pool = unsafe { device.create_descriptor_pool(&skybox_pool_info, None) }
            .context("Failed to create skybox descriptor pool")?;

        let skybox_layouts = [skybox_set_layout];
        let skybox_alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(skybox_descriptor_pool)
            .set_layouts(&skybox_layouts);
        // SAFETY: pool and layout are valid.
        let skybox_sets = unsafe { device.allocate_descriptor_sets(&skybox_alloc_info) }
            .context("Failed to allocate skybox descriptor set")?;
        let skybox_descriptor_set = skybox_sets[0];

        let skybox_cubemap_info = [vk::DescriptorImageInfo {
            sampler: ibl_sampler,
            image_view: resource_manager.get_image_view(skybox_image),
            image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        }];
        let skybox_write = [vk::WriteDescriptorSet::default()
            .dst_set(skybox_descriptor_set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&skybox_cubemap_info)];
        // SAFETY: set, image view, and sampler are valid.
        unsafe { device.update_descriptor_sets(&skybox_write, &[]) };
        log::info!("Skybox descriptor set created");

        // --- Upload meshes (transforms and per-draw selection come from ECS) ---
        let mut gpu_meshes: Vec<GpuMesh> = Vec::with_capacity(scene_data.meshes.len());
        for data in &scene_data.meshes {
            let mesh = resource_manager.upload_mesh(&data.vertices, &data.indices)?;
            gpu_meshes.push(mesh);
        }
        let (command_pool, command_buffers) =
            create_command_pool_and_buffers(&device, queue_family_index)?;
        let (image_available_semaphores, render_finished_semaphores, in_flight_fences) =
            create_sync_objects(&device, swapchain_images.len())?;

        // --- egui renderer ---
        let egui_renderer = EguiRenderer::with_default_allocator(
            &instance,
            physical_device,
            device.clone(),
            DynamicRendering {
                color_attachment_format: swapchain_format,
                depth_attachment_format: None,
            },
            Options {
                in_flight_frames: MAX_FRAMES_IN_FLIGHT,
                ..Default::default()
            },
        )
        .context("Failed to create egui renderer")?;
        log::info!("egui renderer created");

        // --- MSAA images (none for TYPE_1) ---
        // Use HDR_FORMAT for the MSAA color image since the main pass renders to hdr_color.
        let (msaa_color, msaa_depth) = if msaa_samples != vk::SampleCountFlags::TYPE_1 {
            let color = resource_manager.create_msaa_image(
                swapchain_extent.width,
                swapchain_extent.height,
                HDR_FORMAT,
                msaa_samples,
                vk::ImageUsageFlags::COLOR_ATTACHMENT,
                vk::ImageAspectFlags::COLOR,
            )?;
            let depth = resource_manager.create_msaa_image(
                swapchain_extent.width,
                swapchain_extent.height,
                depth_format,
                msaa_samples,
                vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
                depth_aspect(depth_format),
            )?;
            log::info!(
                "MSAA images created ({}×{}, {:?})",
                swapchain_extent.width, swapchain_extent.height, msaa_samples
            );
            (Some(color), Some(depth))
        } else {
            (None, None)
        };

        // -----------------------------------------------------------------------
        // HDR color target + bloom chain
        // -----------------------------------------------------------------------

        // --- HDR color target (main pass renders here, bloom+composite read from it) ---
        let hdr_color = resource_manager.create_attachment_image(
            swapchain_extent.width, swapchain_extent.height,
            HDR_FORMAT,
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
            vk::ImageAspectFlags::COLOR,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::AccessFlags::SHADER_READ,
        )?;

        // --- Bloom sampler (linear, clamp to edge) ---
        let bloom_sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .max_lod(0.0);
        // SAFETY: device is valid, sampler_info is well-formed.
        let bloom_sampler = unsafe { device.create_sampler(&bloom_sampler_info, None) }
            .context("Failed to create bloom sampler")?;

        // --- Bloom chain images (BLOOM_LEVELS levels: half → 1/32 resolution) ---
        let mut bloom_images: Vec<ImageHandle> = Vec::with_capacity(BLOOM_LEVELS);
        let mut bloom_extents: Vec<vk::Extent2D> = Vec::with_capacity(BLOOM_LEVELS);
        let mut w = swapchain_extent.width.max(2) / 2;
        let mut h = swapchain_extent.height.max(2) / 2;
        for _ in 0..BLOOM_LEVELS {
            let img = resource_manager.create_attachment_image(
                w, h,
                HDR_FORMAT,
                vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
                vk::ImageAspectFlags::COLOR,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::AccessFlags::SHADER_READ,
            )?;
            bloom_images.push(img);
            bloom_extents.push(vk::Extent2D { width: w, height: h });
            w = (w / 2).max(1);
            h = (h / 2).max(1);
        }
        log::info!("HDR color target + {} bloom levels created", BLOOM_LEVELS);

        // --- Bloom descriptor set layout (1 sampler) ---
        let bloom_set_layout = super::pipeline::create_bloom_descriptor_set_layout(&device)?;
        // --- Composite descriptor set layout (2 samplers) ---
        let composite_set_layout = super::pipeline::create_composite_descriptor_set_layout(&device)?;

        // --- Bloom + composite descriptor pool ---
        // BLOOM_LEVELS ds-sets + (BLOOM_LEVELS-1) up-sets + 1 composite = 11 sets, max samplers.
        let bloom_pool_sizes = [vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: (BLOOM_LEVELS * 2 + 2) as u32,
        }];
        let bloom_pool_info = vk::DescriptorPoolCreateInfo::default()
            .max_sets((BLOOM_LEVELS * 2 + 1) as u32)
            .pool_sizes(&bloom_pool_sizes);
        // SAFETY: device is valid.
        let bloom_descriptor_pool =
            unsafe { device.create_descriptor_pool(&bloom_pool_info, None) }
                .context("Failed to create bloom descriptor pool")?;
        // composite_descriptor_pool shares the same pool — composite set is allocated from it too.
        let composite_descriptor_pool = bloom_descriptor_pool;

        // --- Allocate bloom downsample sets (BLOOM_LEVELS sets, bloom_set_layout) ---
        let down_layouts: Vec<vk::DescriptorSetLayout> = vec![bloom_set_layout; BLOOM_LEVELS];
        let down_alloc = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(bloom_descriptor_pool)
            .set_layouts(&down_layouts);
        // SAFETY: pool and layout are valid.
        let bloom_downsample_sets = unsafe { device.allocate_descriptor_sets(&down_alloc) }
            .context("Failed to allocate bloom downsample descriptor sets")?;

        // --- Allocate bloom upsample sets (BLOOM_LEVELS-1 sets) ---
        let up_layouts: Vec<vk::DescriptorSetLayout> = vec![bloom_set_layout; BLOOM_LEVELS - 1];
        let up_alloc = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(bloom_descriptor_pool)
            .set_layouts(&up_layouts);
        // SAFETY: pool and layout are valid.
        let bloom_upsample_sets = unsafe { device.allocate_descriptor_sets(&up_alloc) }
            .context("Failed to allocate bloom upsample descriptor sets")?;

        // --- Allocate composite descriptor set (1 set, composite_set_layout) ---
        let comp_layouts = [composite_set_layout];
        let comp_alloc = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(bloom_descriptor_pool)
            .set_layouts(&comp_layouts);
        // SAFETY: pool and layout are valid.
        let composite_sets = unsafe { device.allocate_descriptor_sets(&comp_alloc) }
            .context("Failed to allocate composite descriptor set")?;
        let composite_descriptor_set = composite_sets[0];

        // --- Write bloom downsample descriptor sets ---
        // ds[0] reads hdr_color, ds[i] reads bloom_images[i-1]
        for i in 0..BLOOM_LEVELS {
            let img_view = if i == 0 {
                resource_manager.get_image_view(hdr_color)
            } else {
                resource_manager.get_image_view(bloom_images[i - 1])
            };
            let info = [vk::DescriptorImageInfo {
                sampler: bloom_sampler,
                image_view: img_view,
                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            }];
            let write = [vk::WriteDescriptorSet::default()
                .dst_set(bloom_downsample_sets[i])
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&info)];
            // SAFETY: set, image view, and sampler are valid.
            unsafe { device.update_descriptor_sets(&write, &[]) };
        }

        // --- Write bloom upsample descriptor sets ---
        // us[i] reads bloom_images[i+1] (the smaller level)
        for i in 0..(BLOOM_LEVELS - 1) {
            let info = [vk::DescriptorImageInfo {
                sampler: bloom_sampler,
                image_view: resource_manager.get_image_view(bloom_images[i + 1]),
                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            }];
            let write = [vk::WriteDescriptorSet::default()
                .dst_set(bloom_upsample_sets[i])
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&info)];
            // SAFETY: set, image view, and sampler are valid.
            unsafe { device.update_descriptor_sets(&write, &[]) };
        }

        // --- Write composite descriptor set ---
        // binding 0 = hdr_color, binding 1 = bloom_images[0] (final bloom result)
        let hdr_img_info = [vk::DescriptorImageInfo {
            sampler: bloom_sampler,
            image_view: resource_manager.get_image_view(hdr_color),
            image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        }];
        let bloom_img_info = [vk::DescriptorImageInfo {
            sampler: bloom_sampler,
            image_view: resource_manager.get_image_view(bloom_images[0]),
            image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        }];
        let comp_writes = [
            vk::WriteDescriptorSet::default()
                .dst_set(composite_descriptor_set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&hdr_img_info),
            vk::WriteDescriptorSet::default()
                .dst_set(composite_descriptor_set)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&bloom_img_info),
        ];
        // SAFETY: set, image views, and sampler are valid.
        unsafe { device.update_descriptor_sets(&comp_writes, &[]) };
        log::info!("Bloom descriptor sets written");

        // --- Bloom pipelines ---
        let (bloom_downsample_pipeline_layout, bloom_downsample_pipeline) =
            super::pipeline::create_bloom_pipeline(
                &device, bloom_set_layout, HDR_FORMAT,
                super::pipeline::FULLSCREEN_VERT_SPV,
                super::pipeline::BLOOM_DOWN_FRAG_SPV,
            )?;
        let (bloom_upsample_pipeline_layout, bloom_upsample_pipeline) =
            super::pipeline::create_bloom_pipeline(
                &device, bloom_set_layout, HDR_FORMAT,
                super::pipeline::FULLSCREEN_VERT_SPV,
                super::pipeline::BLOOM_UP_FRAG_SPV,
            )?;
        let (composite_pipeline_layout, composite_pipeline) =
            super::pipeline::create_composite_pipeline(
                &device, composite_set_layout, swapchain_format,
            )?;

        Ok(Self {
            entry,
            instance,
            debug_utils,
            surface_loader,
            surface,
            physical_device,
            device,
            graphics_queue,
            queue_family_index,
            resource_manager: Some(resource_manager),
            gpu_meshes,
            material_sets,
            lighting_set_layout,
            descriptor_pool,
            descriptor_sets,
            ubo_buffers,
            sampler,
            material_set_layout,
            material_descriptor_pool,
            scene_textures,
            default_albedo,
            default_normal,
            default_mr,
            depth_image,
            depth_format,
            shadow_map,
            shadow_sampler,
            ibl_sampler,
            skybox_image,
            irradiance_image,
            prefiltered_image,
            brdf_lut_image,
            skybox_set_layout,
            skybox_descriptor_pool,
            skybox_descriptor_set,
            skybox_pipeline_layout,
            skybox_pipeline,
            wireframe_pipeline_layout,
            wireframe_pipeline,
            wireframe_vertex_buffers,
            gizmo_vertex_buffers,
            shadow_pipeline_layout,
            shadow_pipeline,
            pipeline_layout,
            graphics_pipeline,
            swapchain_loader,
            dynamic_rendering_loader,
            swapchain,
            swapchain_images,
            swapchain_image_views,
            swapchain_format,
            swapchain_extent,
            command_pool,
            command_buffers,
            image_available_semaphores,
            render_finished_semaphores,
            in_flight_fences,
            current_frame: 0,
            acquire_semaphore_index: 0,
            framebuffer_resized: false,
            msaa_samples,
            msaa_max,
            msaa_color,
            msaa_depth,
            hdr_color,
            bloom_sampler,
            bloom_images,
            bloom_extents,
            bloom_set_layout,
            bloom_descriptor_pool,
            bloom_downsample_sets,
            bloom_upsample_sets,
            composite_set_layout,
            composite_descriptor_pool,
            composite_descriptor_set,
            bloom_downsample_pipeline_layout,
            bloom_downsample_pipeline,
            bloom_upsample_pipeline_layout,
            bloom_upsample_pipeline,
            composite_pipeline_layout,
            composite_pipeline,
            egui_renderer: Some(egui_renderer),
            egui_textures_to_free: [Vec::new(), Vec::new()],
        })
    }

    pub fn draw_frame(
        &mut self,
        window: &Window,
        view_proj: glam::Mat4,
        lighting_ubo: LightingUbo,
        instances: &[(glam::Mat4, usize, usize)],
        wireframe_lines: &[glam::Vec3],
        show_wireframe: bool,
        gizmo_verts: &[glam::Vec3],
        gizmo_groups: &[(u32, u32, [f32; 4])],
        egui_primitives: &[egui::ClippedPrimitive],
        egui_textures_delta: egui::TexturesDelta,
        egui_pixels_per_point: f32,
        bloom_enabled: bool,
        bloom_intensity: f32,
        bloom_threshold: f32,
        tone_aces: bool,
    ) -> Result<()> {
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return Ok(()); // Window minimized — skip frame
        }

        // Wayland workaround: compositor may not report OUT_OF_DATE (see gotchas.md).
        if size.width != self.swapchain_extent.width
            || size.height != self.swapchain_extent.height
        {
            self.framebuffer_resized = true;
        }

        // Wait for this frame's previous work to finish.
        // SAFETY: device and fence are valid.
        unsafe {
            self.device
                .wait_for_fences(&[self.in_flight_fences[self.current_frame]], true, u64::MAX)?;
        }

        // Free egui textures from the previous use of this frame slot (GPU is done with them).
        if let Some(ref mut renderer) = self.egui_renderer {
            let to_free = std::mem::take(&mut self.egui_textures_to_free[self.current_frame]);
            if !to_free.is_empty() {
                renderer.free_textures(&to_free).ok();
            }
            // Upload new egui textures before recording.
            renderer
                .set_textures(self.graphics_queue, self.command_pool, egui_textures_delta.set.as_slice())
                .context("egui set_textures failed")?;
        }
        // Queue textures that need to be freed after this slot's GPU work completes.
        self.egui_textures_to_free[self.current_frame] = egui_textures_delta.free;

        // Acquire next swapchain image.
        // Use a rotating index over swapchain_image_count semaphores (not frame-in-flight)
        // to avoid VUID-vkQueueSubmit-pSignalSemaphores-00067.
        let acquire_sem = self.image_available_semaphores[self.acquire_semaphore_index];
        let acquire_result = unsafe {
            // SAFETY: swapchain_loader, swapchain, and semaphore are valid.
            self.swapchain_loader.acquire_next_image(
                self.swapchain,
                u64::MAX,
                acquire_sem,
                vk::Fence::null(),
            )
        };

        let image_index = match acquire_result {
            Ok((index, _suboptimal)) => index,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.recreate_swapchain(window)?;
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        };

        // Reset fence only after we know we will submit work.
        // SAFETY: device and fence are valid; the fence was waited on above.
        unsafe {
            self.device
                .reset_fences(&[self.in_flight_fences[self.current_frame]])?;
        }

        // Update lighting UBO for this frame (fence was waited on, so GPU is done with it).
        let rm_ref = self.resource_manager.as_ref().expect("resource manager alive");
        rm_ref.write_buffer(self.ubo_buffers[self.current_frame], bytemuck::bytes_of(&lighting_ubo));

        // Upload wireframe vertices for this frame (CpuToGpu, already waited on fence).
        const WIREFRAME_BUF_BYTES: usize = 64 * 1024;
        let wireframe_vertex_count = if show_wireframe && !wireframe_lines.is_empty() {
            let bytes = bytemuck::cast_slice::<glam::Vec3, u8>(wireframe_lines);
            let capped = &bytes[..bytes.len().min(WIREFRAME_BUF_BYTES)];
            rm_ref.write_buffer(self.wireframe_vertex_buffers[self.current_frame], capped);
            (capped.len() / std::mem::size_of::<glam::Vec3>()) as u32
        } else {
            0
        };

        // Upload gizmo vertices.
        const GIZMO_BUF_BYTES: usize = 8 * 1024;
        let has_gizmo = !gizmo_verts.is_empty();
        if has_gizmo {
            let bytes = bytemuck::cast_slice::<glam::Vec3, u8>(gizmo_verts);
            let capped = &bytes[..bytes.len().min(GIZMO_BUF_BYTES)];
            rm_ref.write_buffer(self.gizmo_vertex_buffers[self.current_frame], capped);
        }

        // Extract light_view_proj from the UBO (already computed by the caller).
        let light_view_proj = glam::Mat4::from_cols_array(&lighting_ubo.light_mvp);

        // Record command buffer.
        let cmd = self.command_buffers[self.current_frame];
        self.record_command_buffer(cmd, image_index as usize, view_proj, light_view_proj, instances, wireframe_vertex_count, show_wireframe, has_gizmo, gizmo_groups, egui_primitives, egui_pixels_per_point, bloom_enabled, bloom_intensity, bloom_threshold, tone_aces)?;

        // Submit.
        let wait_semaphores = [acquire_sem];
        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let signal_semaphores = [self.render_finished_semaphores[image_index as usize]];
        let command_buffers = [cmd];

        let submit_info = vk::SubmitInfo::default()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&wait_stages)
            .command_buffers(&command_buffers)
            .signal_semaphores(&signal_semaphores);

        // SAFETY: all handles are valid; fence was reset above.
        unsafe {
            self.device.queue_submit(
                self.graphics_queue,
                &[submit_info],
                self.in_flight_fences[self.current_frame],
            )?;
        }

        // Present.
        let swapchains = [self.swapchain];
        let image_indices = [image_index];
        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(&signal_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);

        // SAFETY: queue and present_info are valid.
        let present_result = unsafe {
            self.swapchain_loader
                .queue_present(self.graphics_queue, &present_info)
        };

        match present_result {
            Ok(true) | Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.framebuffer_resized = true;
            }
            Ok(false) => {}
            Err(e) => return Err(e.into()),
        }

        if self.framebuffer_resized {
            self.recreate_swapchain(window)?;
        }

        self.acquire_semaphore_index =
            (self.acquire_semaphore_index + 1) % self.image_available_semaphores.len();
        self.current_frame = (self.current_frame + 1) % MAX_FRAMES_IN_FLIGHT;
        Ok(())
    }

    fn record_command_buffer(
        &mut self,
        cmd: vk::CommandBuffer,
        image_index: usize,
        view_proj: glam::Mat4,
        light_view_proj: glam::Mat4,
        instances: &[(glam::Mat4, usize, usize)],
        wireframe_vertex_count: u32,
        show_wireframe: bool,
        has_gizmo: bool,
        gizmo_groups: &[(u32, u32, [f32; 4])],
        egui_primitives: &[egui::ClippedPrimitive],
        egui_pixels_per_point: f32,
        bloom_enabled: bool,
        bloom_intensity: f32,
        bloom_threshold: f32,
        tone_aces: bool,
    ) -> Result<()> {
        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        // SAFETY: cmd is valid and not in use (fence was waited on).
        unsafe {
            self.device.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())?;
            self.device.begin_command_buffer(cmd, &begin_info)?;
        }

        // -----------------------------------------------------------------------
        // Build the render graph for this frame.
        // -----------------------------------------------------------------------
        let rm = self.resource_manager.as_ref().expect("resource manager alive");

        let mut graph = RenderGraph::new();

        // Register image resources with their initial (per-frame) layouts.
        let r_swapchain = graph.add_resource(
            self.swapchain_images[image_index],
            vk::ImageAspectFlags::COLOR,
            vk::ImageLayout::UNDEFINED, // swapchain is UNDEFINED at frame start
        );
        let r_shadow = graph.add_resource(
            rm.get_image_raw(self.shadow_map),
            vk::ImageAspectFlags::DEPTH,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL, // initialised at creation
        );
        // HDR color target — discard each frame (UNDEFINED).
        let r_hdr = graph.add_resource(
            rm.get_image_raw(self.hdr_color),
            vk::ImageAspectFlags::COLOR,
            vk::ImageLayout::UNDEFINED,
        );
        // Bloom chain — discard each frame (UNDEFINED).
        let r_bloom: Vec<_> = self.bloom_images.iter().map(|&h| {
            graph.add_resource(
                rm.get_image_raw(h),
                vk::ImageAspectFlags::COLOR,
                vk::ImageLayout::UNDEFINED,
            )
        }).collect();
        // Optional MSAA resources.
        let r_msaa_color = self.msaa_color.map(|h| {
            graph.add_resource(
                rm.get_image_raw(h),
                vk::ImageAspectFlags::COLOR,
                vk::ImageLayout::UNDEFINED,
            )
        });
        let r_msaa_depth = self.msaa_depth.map(|h| {
            graph.add_resource(
                rm.get_image_raw(h),
                depth_aspect(self.depth_format),
                vk::ImageLayout::UNDEFINED,
            )
        });

        // --- Frame-init pseudo-pass: UNDEFINED → COLOR_ATTACHMENT / DEPTH_STENCIL ---
        {
            let mut init_accesses: Vec<(super::render_graph::ResourceId, ResourceAccess)> =
                vec![
                    (r_swapchain, ResourceAccess::color_init()),
                    (r_hdr, ResourceAccess::color_init()),
                ];
            if let Some(rc) = r_msaa_color {
                init_accesses.push((rc, ResourceAccess::color_init()));
            }
            if let Some(rd) = r_msaa_depth {
                // MSAA depth: UNDEFINED → DEPTH_STENCIL. Use a plain depth subresource
                // (the graph uses the aspect stored on the resource).
                init_accesses.push((rd, ResourceAccess::depth_init()));
            }
            graph.add_pass("FrameInit", &init_accesses);
        }

        // --- Shadow pass ---
        graph.add_pass("Shadow", &[(r_shadow, ResourceAccess::shadow_write())]);

        // --- Main + Skybox pass ---
        // Renders to hdr_color (or msaa_color if MSAA, which resolves to hdr_color).
        // After the pass, hdr_color transitions to SHADER_READ_ONLY for bloom/composite.
        {
            if let Some(rc) = r_msaa_color {
                // MSAA: msaa_color is the render target, hdr_color is the resolve target.
                // Both need COLOR_ATTACHMENT during the pass; hdr_color transitions to
                // SHADER_READ_ONLY after (msaa_color stays COLOR_ATTACHMENT, not sampled).
                graph.add_pass(
                    "Main+Skybox",
                    &[
                        (rc,    ResourceAccess::color_attachment()),
                        (r_hdr, ResourceAccess::color_attachment_to_read()),
                        (r_shadow, ResourceAccess::shader_read()),
                    ],
                );
            } else {
                graph.add_pass(
                    "Main+Skybox",
                    &[
                        (r_hdr, ResourceAccess::color_attachment_to_read()),
                        (r_shadow, ResourceAccess::shader_read()),
                    ],
                );
            }
        }

        // --- Bloom passes (conditional) ---
        if bloom_enabled {
            // Downsample: hdr_color → bloom[0] → bloom[1] → ... → bloom[BLOOM_LEVELS-1]
            for i in 0..BLOOM_LEVELS {
                // Each downsample writes to bloom[i] and transitions it to SHADER_READ_ONLY.
                // The source (hdr_color or bloom[i-1]) is already SHADER_READ_ONLY from the
                // previous pass's exit barrier, so no additional declaration needed here.
                let pass_name: &'static str = match i {
                    0 => "BloomDown0",
                    1 => "BloomDown1",
                    2 => "BloomDown2",
                    3 => "BloomDown3",
                    _ => "BloomDown4",
                };
                graph.add_pass(pass_name, &[(r_bloom[i], ResourceAccess::color_attachment_to_read())]);
            }
            // Upsample: bloom[BLOOM_LEVELS-2] ← bloom[BLOOM_LEVELS-1] (overwrite SHADER_READ_ONLY)
            for i in (0..(BLOOM_LEVELS - 1)).rev() {
                let pass_name: &'static str = match i {
                    0 => "BloomUp0",
                    1 => "BloomUp1",
                    2 => "BloomUp2",
                    _ => "BloomUp3",
                };
                graph.add_pass(pass_name, &[(r_bloom[i], ResourceAccess::bloom_overwrite())]);
            }
        }

        // --- Composite pass: tone-map hdr_color + bloom → swapchain ---
        // hdr_color and bloom[0] are already SHADER_READ_ONLY; swapchain is COLOR_ATTACHMENT.
        graph.add_pass("Composite", &[(r_swapchain, ResourceAccess::color_attachment())]);

        // --- Wireframe pass (conditional) ---
        if show_wireframe && wireframe_vertex_count > 0 {
            graph.add_pass("Wireframe", &[(r_swapchain, ResourceAccess::color_attachment())]);
        }

        // --- Gizmo pass (conditional) ---
        if has_gizmo && !gizmo_groups.is_empty() {
            graph.add_pass("Gizmo", &[(r_swapchain, ResourceAccess::color_attachment())]);
        }

        // --- Egui pass (conditional) ---
        if !egui_primitives.is_empty() {
            graph.add_pass("Egui", &[(r_swapchain, ResourceAccess::color_attachment())]);
        }

        // --- Present pseudo-pass: COLOR_ATTACHMENT → PRESENT_SRC ---
        graph.add_pass("Present", &[(r_swapchain, ResourceAccess::present())]);

        // Validate the graph (detect missing producers).
        graph.compile()?;

        // -----------------------------------------------------------------------
        // FrameInit — no draw commands, just layout transitions via begin/end.
        // -----------------------------------------------------------------------
        graph.begin_pass(&self.device, cmd);
        graph.end_pass(&self.device, cmd);

        // -----------------------------------------------------------------------
        // Shadow pass
        // -----------------------------------------------------------------------
        graph.begin_pass(&self.device, cmd);

        let shadow_depth_att = vk::RenderingAttachmentInfo::default()
            .image_view(rm.get_image_view(self.shadow_map))
            .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue { depth: 1.0, stencil: 0 },
            });

        let shadow_rendering_info = vk::RenderingInfo::default()
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: vk::Extent2D { width: 2048, height: 2048 },
            })
            .layer_count(1)
            .depth_attachment(&shadow_depth_att);

        // SAFETY: cmd is recording, shadow_rendering_info references live stack data.
        unsafe {
            self.dynamic_rendering_loader
                .cmd_begin_rendering(cmd, &shadow_rendering_info);

            // Standard (non-flipped) viewport — shadow UVs are computed from the same
            // light_view_proj that was used to render, so no Y flip needed here.
            let shadow_viewports = [vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: 2048.0,
                height: 2048.0,
                min_depth: 0.0,
                max_depth: 1.0,
            }];
            self.device.cmd_set_viewport(cmd, 0, &shadow_viewports);
            let shadow_scissors = [vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: vk::Extent2D { width: 2048, height: 2048 },
            }];
            self.device.cmd_set_scissor(cmd, 0, &shadow_scissors);

            self.device.cmd_bind_pipeline(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.shadow_pipeline,
            );

            for (model, mesh_idx, _mat_idx) in instances {
                let mesh = &self.gpu_meshes[*mesh_idx];
                // Push constant: full MVP from light's perspective = light_view_proj * model.
                let light_mvp: glam::Mat4 = light_view_proj * *model;
                // SAFETY: Mat4 is Pod, 64 bytes; layout has this range.
                self.device.cmd_push_constants(
                    cmd,
                    self.shadow_pipeline_layout,
                    vk::ShaderStageFlags::VERTEX,
                    0,
                    bytemuck::bytes_of(&light_mvp),
                );

                self.device.cmd_bind_vertex_buffers(
                    cmd,
                    0,
                    &[rm.get_buffer(mesh.vertex_buffer)],
                    &[0],
                );
                self.device.cmd_bind_index_buffer(
                    cmd,
                    rm.get_buffer(mesh.index_buffer),
                    0,
                    vk::IndexType::UINT32,
                );
                self.device.cmd_draw_indexed(cmd, mesh.index_count, 1, 0, 0, 0);
            }

            self.dynamic_rendering_loader.cmd_end_rendering(cmd);
        }

        // end_pass emits the exit barrier: DEPTH_STENCIL → SHADER_READ_ONLY.
        graph.end_pass(&self.device, cmd);

        // -----------------------------------------------------------------------
        // Main pass (PBR meshes + skybox)
        // -----------------------------------------------------------------------
        graph.begin_pass(&self.device, cmd);

        let clear_color = vk::ClearValue {
            color: vk::ClearColorValue { float32: [0.01, 0.01, 0.05, 1.0] },
        };
        let clear_depth = vk::ClearValue {
            depth_stencil: vk::ClearDepthStencilValue { depth: 1.0, stencil: 0 },
        };

        let color_attachment = if let Some(msaa_color) = self.msaa_color {
            // MSAA: render to multisampled image, resolve to hdr_color.
            vk::RenderingAttachmentInfo::default()
                .image_view(rm.get_image_view(msaa_color))
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .resolve_mode(vk::ResolveModeFlags::AVERAGE)
                .resolve_image_view(rm.get_image_view(self.hdr_color))
                .resolve_image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::DONT_CARE) // resolved copy is in hdr_color
                .clear_value(clear_color)
        } else {
            // No MSAA: render directly to hdr_color.
            vk::RenderingAttachmentInfo::default()
                .image_view(rm.get_image_view(self.hdr_color))
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(clear_color)
        };

        let depth_attachment = if let Some(msaa_depth) = self.msaa_depth {
            // MSAA depth: use the multisampled depth image (no resolve needed for depth).
            vk::RenderingAttachmentInfo::default()
                .image_view(rm.get_image_view(msaa_depth))
                .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::DONT_CARE)
                .clear_value(clear_depth)
        } else {
            // No MSAA: regular depth buffer.
            vk::RenderingAttachmentInfo::default()
                .image_view(rm.get_image_view(self.depth_image))
                .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::DONT_CARE)
                .clear_value(clear_depth)
        };

        let color_attachments = [color_attachment];
        let rendering_info = vk::RenderingInfo::default()
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: self.swapchain_extent,
            })
            .layer_count(1)
            .color_attachments(&color_attachments)
            .depth_attachment(&depth_attachment);

        // SAFETY: cmd is recording, rendering_info references live stack data.
        // Uses KHR loader — works on both Vulkan 1.2+ext (MoltenVK) and 1.3 (native).
        unsafe {
            self.dynamic_rendering_loader
                .cmd_begin_rendering(cmd, &rendering_info);

            // Dynamic viewport with negative height to flip Y (see gotchas.md)
            let viewports = [vk::Viewport {
                x: 0.0,
                y: self.swapchain_extent.height as f32,
                width: self.swapchain_extent.width as f32,
                height: -(self.swapchain_extent.height as f32),
                min_depth: 0.0,
                max_depth: 1.0,
            }];
            self.device.cmd_set_viewport(cmd, 0, &viewports);

            let scissors = [vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: self.swapchain_extent,
            }];
            self.device.cmd_set_scissor(cmd, 0, &scissors);

            self.device.cmd_bind_pipeline(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.graphics_pipeline,
            );

            // Bind lighting UBO for this frame — same for all meshes.
            self.device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout,
                0,
                &[self.descriptor_sets[self.current_frame]],
                &[],
            );

            for (model, mesh_idx, mat_set_idx) in instances {
                let mesh = &self.gpu_meshes[*mesh_idx];
                let material_set = self.material_sets[*mat_set_idx];

                // Bind per-material textures (set 1).
                self.device.cmd_bind_descriptor_sets(
                    cmd,
                    vk::PipelineBindPoint::GRAPHICS,
                    self.pipeline_layout,
                    1,
                    &[material_set],
                    &[],
                );

                let mvp = view_proj * *model;
                let pc = MeshPushConstants { mvp, model: *model };
                // SAFETY: MeshPushConstants is Pod, 128 bytes; layout has this range.
                self.device.cmd_push_constants(
                    cmd,
                    self.pipeline_layout,
                    vk::ShaderStageFlags::VERTEX,
                    0,
                    bytemuck::bytes_of(&pc),
                );

                self.device.cmd_bind_vertex_buffers(
                    cmd,
                    0,
                    &[rm.get_buffer(mesh.vertex_buffer)],
                    &[0],
                );
                self.device.cmd_bind_index_buffer(
                    cmd,
                    rm.get_buffer(mesh.index_buffer),
                    0,
                    vk::IndexType::UINT32,
                );
                // SAFETY: index_count matches what was uploaded; no index buffer overflow.
                self.device.cmd_draw_indexed(cmd, mesh.index_count, 1, 0, 0, 0);
            }

            // Skybox: fullscreen triangle at depth=1.0.
            // Passes LESS_OR_EQUAL test only where no geometry was drawn (depth buffer == 1.0).
            self.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.skybox_pipeline);
            self.device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.skybox_pipeline_layout,
                0,
                &[self.skybox_descriptor_set],
                &[],
            );
            let inv_view_proj = view_proj.inverse();
            // SAFETY: Mat4 is Pod, 64 bytes; layout has a 64-byte VERTEX push constant.
            self.device.cmd_push_constants(
                cmd,
                self.skybox_pipeline_layout,
                vk::ShaderStageFlags::VERTEX,
                0,
                bytemuck::bytes_of(&inv_view_proj),
            );
            // 3 vertices, no vertex buffer — positions generated from gl_VertexIndex.
            self.device.cmd_draw(cmd, 3, 1, 0, 0);

            self.dynamic_rendering_loader.cmd_end_rendering(cmd);
        }

        graph.end_pass(&self.device, cmd);

        // -----------------------------------------------------------------------
        // Bloom passes (conditional)
        // -----------------------------------------------------------------------
        if bloom_enabled {
            // Push constant layout for bloom shaders: [texel_w, texel_h, param, _pad]
            #[repr(C)]
            #[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
            struct BloomPc {
                texel_w: f32,
                texel_h: f32,
                param: f32, // threshold (downsample) or blend (upsample)
                _pad: f32,
            }

            // --- Downsample passes ---
            for i in 0..BLOOM_LEVELS {
                graph.begin_pass(&self.device, cmd);

                let ext = self.bloom_extents[i];
                let bloom_color_att = vk::RenderingAttachmentInfo::default()
                    .image_view(rm.get_image_view(self.bloom_images[i]))
                    .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .load_op(vk::AttachmentLoadOp::DONT_CARE)
                    .store_op(vk::AttachmentStoreOp::STORE);
                let bloom_rendering_info = vk::RenderingInfo::default()
                    .render_area(vk::Rect2D {
                        offset: vk::Offset2D { x: 0, y: 0 },
                        extent: ext,
                    })
                    .layer_count(1)
                    .color_attachments(std::slice::from_ref(&bloom_color_att));

                // Source texel size: if i==0, source is hdr_color; else bloom[i-1].
                let (src_w, src_h) = if i == 0 {
                    (self.swapchain_extent.width as f32, self.swapchain_extent.height as f32)
                } else {
                    (self.bloom_extents[i - 1].width as f32, self.bloom_extents[i - 1].height as f32)
                };
                let pc = BloomPc {
                    texel_w: 1.0 / src_w,
                    texel_h: 1.0 / src_h,
                    param: bloom_threshold,
                    _pad: 0.0,
                };

                // SAFETY: cmd is recording; all referenced handles are valid.
                unsafe {
                    self.dynamic_rendering_loader.cmd_begin_rendering(cmd, &bloom_rendering_info);

                    let viewports = [vk::Viewport {
                        x: 0.0, y: 0.0,
                        width: ext.width as f32, height: ext.height as f32,
                        min_depth: 0.0, max_depth: 1.0,
                    }];
                    self.device.cmd_set_viewport(cmd, 0, &viewports);
                    let scissors = [vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: ext }];
                    self.device.cmd_set_scissor(cmd, 0, &scissors);

                    self.device.cmd_bind_pipeline(
                        cmd, vk::PipelineBindPoint::GRAPHICS, self.bloom_downsample_pipeline,
                    );
                    self.device.cmd_bind_descriptor_sets(
                        cmd, vk::PipelineBindPoint::GRAPHICS,
                        self.bloom_downsample_pipeline_layout, 0,
                        &[self.bloom_downsample_sets[i]], &[],
                    );
                    // SAFETY: BloomPc is Pod, 16 bytes; layout has a 16-byte FRAGMENT push constant.
                    self.device.cmd_push_constants(
                        cmd, self.bloom_downsample_pipeline_layout,
                        vk::ShaderStageFlags::FRAGMENT, 0,
                        bytemuck::bytes_of(&pc),
                    );
                    // Fullscreen triangle — no vertex buffer.
                    self.device.cmd_draw(cmd, 3, 1, 0, 0);

                    self.dynamic_rendering_loader.cmd_end_rendering(cmd);
                }

                graph.end_pass(&self.device, cmd);
            }

            // --- Upsample passes (BLOOM_LEVELS-2 down to 0) ---
            for i in (0..(BLOOM_LEVELS - 1)).rev() {
                graph.begin_pass(&self.device, cmd);

                let ext = self.bloom_extents[i];
                let bloom_color_att = vk::RenderingAttachmentInfo::default()
                    .image_view(rm.get_image_view(self.bloom_images[i]))
                    .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .load_op(vk::AttachmentLoadOp::DONT_CARE)
                    .store_op(vk::AttachmentStoreOp::STORE);
                let bloom_rendering_info = vk::RenderingInfo::default()
                    .render_area(vk::Rect2D {
                        offset: vk::Offset2D { x: 0, y: 0 },
                        extent: ext,
                    })
                    .layer_count(1)
                    .color_attachments(std::slice::from_ref(&bloom_color_att));

                // Source is bloom[i+1] (smaller level).
                let src_ext = self.bloom_extents[i + 1];
                let pc = BloomPc {
                    texel_w: 1.0 / src_ext.width as f32,
                    texel_h: 1.0 / src_ext.height as f32,
                    param: bloom_intensity, // blend factor for upsample
                    _pad: 0.0,
                };

                // SAFETY: cmd is recording; all referenced handles are valid.
                unsafe {
                    self.dynamic_rendering_loader.cmd_begin_rendering(cmd, &bloom_rendering_info);

                    let viewports = [vk::Viewport {
                        x: 0.0, y: 0.0,
                        width: ext.width as f32, height: ext.height as f32,
                        min_depth: 0.0, max_depth: 1.0,
                    }];
                    self.device.cmd_set_viewport(cmd, 0, &viewports);
                    let scissors = [vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: ext }];
                    self.device.cmd_set_scissor(cmd, 0, &scissors);

                    self.device.cmd_bind_pipeline(
                        cmd, vk::PipelineBindPoint::GRAPHICS, self.bloom_upsample_pipeline,
                    );
                    self.device.cmd_bind_descriptor_sets(
                        cmd, vk::PipelineBindPoint::GRAPHICS,
                        self.bloom_upsample_pipeline_layout, 0,
                        &[self.bloom_upsample_sets[i]], &[],
                    );
                    // SAFETY: BloomPc is Pod, 16 bytes; layout has a 16-byte FRAGMENT push constant.
                    self.device.cmd_push_constants(
                        cmd, self.bloom_upsample_pipeline_layout,
                        vk::ShaderStageFlags::FRAGMENT, 0,
                        bytemuck::bytes_of(&pc),
                    );
                    self.device.cmd_draw(cmd, 3, 1, 0, 0);

                    self.dynamic_rendering_loader.cmd_end_rendering(cmd);
                }

                graph.end_pass(&self.device, cmd);
            }
        }

        // -----------------------------------------------------------------------
        // Composite pass: tone-map hdr_color + bloom → swapchain
        // -----------------------------------------------------------------------
        graph.begin_pass(&self.device, cmd);
        {
            let comp_color_att = vk::RenderingAttachmentInfo::default()
                .image_view(self.swapchain_image_views[image_index])
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::DONT_CARE) // composite covers full screen
                .store_op(vk::AttachmentStoreOp::STORE);
            let comp_rendering_info = vk::RenderingInfo::default()
                .render_area(vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: self.swapchain_extent,
                })
                .layer_count(1)
                .color_attachments(std::slice::from_ref(&comp_color_att));

            #[repr(C)]
            #[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
            struct CompositePc {
                bloom_intensity: f32,
                tone_mode: f32,      // 0 = Reinhard, 1 = ACES
                bloom_enabled: f32,
                _pad: f32,
            }
            let comp_pc = CompositePc {
                bloom_intensity,
                tone_mode: if tone_aces { 1.0 } else { 0.0 },
                bloom_enabled: if bloom_enabled { 1.0 } else { 0.0 },
                _pad: 0.0,
            };

            // SAFETY: cmd is recording; all referenced handles are valid.
            unsafe {
                self.dynamic_rendering_loader.cmd_begin_rendering(cmd, &comp_rendering_info);

                // No Y-flip: hdr_color was rendered with Y-flip so it's already correctly oriented.
                let viewports = [vk::Viewport {
                    x: 0.0, y: 0.0,
                    width: self.swapchain_extent.width as f32,
                    height: self.swapchain_extent.height as f32,
                    min_depth: 0.0, max_depth: 1.0,
                }];
                self.device.cmd_set_viewport(cmd, 0, &viewports);
                let scissors = [vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: self.swapchain_extent,
                }];
                self.device.cmd_set_scissor(cmd, 0, &scissors);

                self.device.cmd_bind_pipeline(
                    cmd, vk::PipelineBindPoint::GRAPHICS, self.composite_pipeline,
                );
                self.device.cmd_bind_descriptor_sets(
                    cmd, vk::PipelineBindPoint::GRAPHICS,
                    self.composite_pipeline_layout, 0,
                    &[self.composite_descriptor_set], &[],
                );
                // SAFETY: CompositePc is Pod, 16 bytes; layout has a 16-byte FRAGMENT push constant.
                self.device.cmd_push_constants(
                    cmd, self.composite_pipeline_layout,
                    vk::ShaderStageFlags::FRAGMENT, 0,
                    bytemuck::bytes_of(&comp_pc),
                );
                // Fullscreen triangle — no vertex buffer.
                self.device.cmd_draw(cmd, 3, 1, 0, 0);

                self.dynamic_rendering_loader.cmd_end_rendering(cmd);
            }
        }
        graph.end_pass(&self.device, cmd);

        // -----------------------------------------------------------------------
        // Wireframe debug pass (conditional)
        // -----------------------------------------------------------------------
        if show_wireframe && wireframe_vertex_count > 0 {
            graph.begin_pass(&self.device, cmd);

            let wf_color_att = vk::RenderingAttachmentInfo::default()
                .image_view(self.swapchain_image_views[image_index])
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::LOAD)
                .store_op(vk::AttachmentStoreOp::STORE);

            let wf_rendering_info = vk::RenderingInfo::default()
                .render_area(vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: self.swapchain_extent,
                })
                .layer_count(1)
                .color_attachments(std::slice::from_ref(&wf_color_att));

            let wf_vbuf = rm.get_buffer(self.wireframe_vertex_buffers[self.current_frame]);

            #[repr(C)]
            #[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
            struct WireframePc {
                view_proj: glam::Mat4,
                color: [f32; 4],
            }
            let pc = WireframePc {
                view_proj,
                color: [0.0, 1.0, 0.0, 1.0], // bright green
            };

            // SAFETY: cmd is recording; all referenced handles are valid.
            unsafe {
                self.dynamic_rendering_loader.cmd_begin_rendering(cmd, &wf_rendering_info);

                let viewports = [vk::Viewport {
                    x: 0.0,
                    y: self.swapchain_extent.height as f32,
                    width: self.swapchain_extent.width as f32,
                    height: -(self.swapchain_extent.height as f32),
                    min_depth: 0.0,
                    max_depth: 1.0,
                }];
                self.device.cmd_set_viewport(cmd, 0, &viewports);
                let scissors = [vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: self.swapchain_extent,
                }];
                self.device.cmd_set_scissor(cmd, 0, &scissors);

                self.device.cmd_bind_pipeline(
                    cmd,
                    vk::PipelineBindPoint::GRAPHICS,
                    self.wireframe_pipeline,
                );
                // SAFETY: WireframePc is Pod, 80 bytes; layout has an 80-byte VERTEX range.
                self.device.cmd_push_constants(
                    cmd,
                    self.wireframe_pipeline_layout,
                    vk::ShaderStageFlags::VERTEX,
                    0,
                    bytemuck::bytes_of(&pc),
                );
                self.device.cmd_bind_vertex_buffers(cmd, 0, &[wf_vbuf], &[0]);
                self.device.cmd_draw(cmd, wireframe_vertex_count, 1, 0, 0);

                self.dynamic_rendering_loader.cmd_end_rendering(cmd);
            }

            graph.end_pass(&self.device, cmd);
        }

        // -----------------------------------------------------------------------
        // Gizmo pass (conditional — selected entity or active gizmo)
        // -----------------------------------------------------------------------
        if has_gizmo && !gizmo_groups.is_empty() {
            graph.begin_pass(&self.device, cmd);

            let gizmo_color_att = vk::RenderingAttachmentInfo::default()
                .image_view(self.swapchain_image_views[image_index])
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::LOAD)
                .store_op(vk::AttachmentStoreOp::STORE);

            let gizmo_rendering_info = vk::RenderingInfo::default()
                .render_area(vk::Rect2D { offset: vk::Offset2D { x:0, y:0 }, extent: self.swapchain_extent })
                .layer_count(1)
                .color_attachments(std::slice::from_ref(&gizmo_color_att));

            let gizmo_vbuf = rm.get_buffer(self.gizmo_vertex_buffers[self.current_frame]);

            #[repr(C)]
            #[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
            struct GizmoPc { view_proj: glam::Mat4, color: [f32; 4] }

            // SAFETY: cmd is recording; all handles valid.
            unsafe {
                self.dynamic_rendering_loader.cmd_begin_rendering(cmd, &gizmo_rendering_info);
                let viewports = [vk::Viewport {
                    x: 0.0, y: self.swapchain_extent.height as f32,
                    width: self.swapchain_extent.width as f32,
                    height: -(self.swapchain_extent.height as f32),
                    min_depth: 0.0, max_depth: 1.0,
                }];
                self.device.cmd_set_viewport(cmd, 0, &viewports);
                let scissors = [vk::Rect2D { offset: vk::Offset2D { x:0, y:0 }, extent: self.swapchain_extent }];
                self.device.cmd_set_scissor(cmd, 0, &scissors);
                self.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.wireframe_pipeline);
                self.device.cmd_bind_vertex_buffers(cmd, 0, &[gizmo_vbuf], &[0]);

                for &(first_vertex, vertex_count, color) in gizmo_groups {
                    if vertex_count == 0 { continue; }
                    let pc = GizmoPc { view_proj, color };
                    self.device.cmd_push_constants(
                        cmd, self.wireframe_pipeline_layout,
                        vk::ShaderStageFlags::VERTEX, 0, bytemuck::bytes_of(&pc),
                    );
                    self.device.cmd_draw(cmd, vertex_count, 1, first_vertex, 0);
                }
                self.dynamic_rendering_loader.cmd_end_rendering(cmd);
            }
            graph.end_pass(&self.device, cmd);
        }

        // -----------------------------------------------------------------------
        // Egui pass (conditional)
        // -----------------------------------------------------------------------
        if !egui_primitives.is_empty() {
            graph.begin_pass(&self.device, cmd);

            let egui_color_att = vk::RenderingAttachmentInfo::default()
                .image_view(self.swapchain_image_views[image_index])
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::LOAD)   // keep the rendered scene
                .store_op(vk::AttachmentStoreOp::STORE);

            let egui_rendering_info = vk::RenderingInfo::default()
                .render_area(vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: self.swapchain_extent,
                })
                .layer_count(1)
                .color_attachments(std::slice::from_ref(&egui_color_att));

            // SAFETY: cmd is recording, all referenced objects are valid.
            unsafe {
                self.dynamic_rendering_loader.cmd_begin_rendering(cmd, &egui_rendering_info);
            }

            if let Some(ref mut renderer) = self.egui_renderer {
                renderer
                    .cmd_draw(cmd, self.swapchain_extent, egui_pixels_per_point, egui_primitives)
                    .context("egui cmd_draw failed")?;
            }

            // SAFETY: matching cmd_begin_rendering above.
            unsafe {
                self.dynamic_rendering_loader.cmd_end_rendering(cmd);
            }

            graph.end_pass(&self.device, cmd);
        }

        // -----------------------------------------------------------------------
        // Present pseudo-pass: COLOR_ATTACHMENT → PRESENT_SRC_KHR
        // -----------------------------------------------------------------------
        graph.begin_pass(&self.device, cmd);
        graph.end_pass(&self.device, cmd);

        // SAFETY: cmd is recording with a matching begin.
        unsafe {
            self.device.end_command_buffer(cmd)?;
        }

        Ok(())
    }

    /// Hot-reload: replace a pipeline without recreating its layout.
    /// Calls `vkDeviceWaitIdle` which causes a brief stutter (see Phase 12 notes).
    pub fn recreate_pipeline(
        &mut self,
        target: crate::asset::ShaderTarget,
        vert_spv: &[u8],
        frag_spv: &[u8],
    ) -> Result<()> {
        // SAFETY: wait for all in-flight GPU work so we can safely destroy the pipeline.
        unsafe { self.device.device_wait_idle()? };

        let binding_descriptions = [super::vertex::Vertex::binding_description()];
        let attribute_descriptions = super::vertex::Vertex::attribute_descriptions();

        match target {
            crate::asset::ShaderTarget::Main => {
                // SAFETY: device is idle, pipeline is not in use.
                unsafe { self.device.destroy_pipeline(self.graphics_pipeline, None) };
                self.graphics_pipeline = super::pipeline::build_graphics_pipeline(
                    &self.device,
                    vert_spv,
                    frag_spv,
                    self.pipeline_layout,
                    HDR_FORMAT, // main pass renders to hdr_color
                    self.depth_format,
                    &binding_descriptions,
                    &attribute_descriptions,
                    self.msaa_samples,
                )?;
                log::info!("✓ Reloaded: main pipeline");
            }
            crate::asset::ShaderTarget::Shadow => {
                // SAFETY: device is idle.
                unsafe { self.device.destroy_pipeline(self.shadow_pipeline, None) };
                self.shadow_pipeline = super::pipeline::build_shadow_pipeline(
                    &self.device,
                    vert_spv,
                    frag_spv,
                    self.shadow_pipeline_layout,
                    self.depth_format,
                    &binding_descriptions,
                    &attribute_descriptions,
                )?;
                log::info!("✓ Reloaded: shadow pipeline");
            }
            crate::asset::ShaderTarget::Skybox => {
                // SAFETY: device is idle.
                unsafe { self.device.destroy_pipeline(self.skybox_pipeline, None) };
                self.skybox_pipeline = super::pipeline::build_skybox_pipeline(
                    &self.device,
                    vert_spv,
                    frag_spv,
                    self.skybox_pipeline_layout,
                    HDR_FORMAT, // skybox renders to hdr_color
                    self.depth_format,
                    self.msaa_samples,
                )?;
                log::info!("✓ Reloaded: skybox pipeline");
            }
        }

        Ok(())
    }

    pub fn msaa_samples(&self) -> vk::SampleCountFlags { self.msaa_samples }
    pub fn msaa_max(&self) -> vk::SampleCountFlags { self.msaa_max }

    /// Replace all scene-specific GPU resources (meshes, textures, material descriptor sets)
    /// with the contents of `scene_data`. The swap is atomic from the CPU perspective:
    /// `vkDeviceWaitIdle` ensures no in-flight commands reference the old resources.
    ///
    /// After this call, `MeshRenderer.mesh_index` and `material_set_index` from the new
    /// ECS world reference the updated global arrays.
    pub fn reload_scene(&mut self, scene_data: &crate::asset::SceneData) -> Result<()> {
        // SAFETY: wait for all in-flight GPU work before touching any resources.
        unsafe { self.device.device_wait_idle().context("device_wait_idle failed")? };

        // --- Free old scene meshes and textures ---
        {
            let rm = self.resource_manager.as_mut().context("resource manager unavailable")?;
            for mesh in self.gpu_meshes.drain(..) {
                rm.destroy_mesh(mesh);
            }
            for &tex in &self.scene_textures {
                rm.destroy_image(tex);
            }
        }
        self.scene_textures.clear();

        // Destroy old material descriptor pool — implicitly frees all sets allocated from it.
        // SAFETY: device is idle, pool is no longer referenced.
        unsafe { self.device.destroy_descriptor_pool(self.material_descriptor_pool, None); }

        // --- Upload new textures ---
        {
            let rm = self.resource_manager.as_mut().context("resource manager unavailable")?;
            for tex in &scene_data.textures {
                let format = if tex.is_srgb {
                    vk::Format::R8G8B8A8_SRGB
                } else {
                    vk::Format::R8G8B8A8_UNORM
                };
                self.scene_textures.push(
                    rm.upload_texture(&tex.pixels, tex.width, tex.height, format)?,
                );
            }
        }

        // --- Create new material descriptor pool ---
        let num_material_sets = scene_data.materials.len() + 1; // +1 default
        let pool_sizes = [vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: (num_material_sets * 3) as u32,
        }];
        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&pool_sizes)
            .max_sets(num_material_sets as u32);
        // SAFETY: device is idle, pool_info is well-formed.
        self.material_descriptor_pool =
            unsafe { self.device.create_descriptor_pool(&pool_info, None) }
                .context("Failed to create material descriptor pool")?;

        // --- Allocate new material descriptor sets ---
        let layouts = vec![self.material_set_layout; num_material_sets];
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(self.material_descriptor_pool)
            .set_layouts(&layouts);
        self.material_sets =
            unsafe { self.device.allocate_descriptor_sets(&alloc_info) }
                .context("Failed to allocate material descriptor sets")?;

        // --- Write material descriptor sets ---
        {
            let rm = self.resource_manager.as_ref().context("resource manager unavailable")?;
            let scene_textures = &self.scene_textures;
            let (default_albedo, default_normal, default_mr) =
                (self.default_albedo, self.default_normal, self.default_mr);

            for (i, mat) in scene_data.materials.iter().enumerate() {
                let albedo = mat.albedo_tex
                    .and_then(|j| scene_textures.get(j).copied())
                    .unwrap_or(default_albedo);
                let normal = mat.normal_tex
                    .and_then(|j| scene_textures.get(j).copied())
                    .unwrap_or(default_normal);
                let mr = mat.metallic_roughness_tex
                    .and_then(|j| scene_textures.get(j).copied())
                    .unwrap_or(default_mr);
                write_material_set(
                    &self.device, self.material_sets[i], self.sampler, rm,
                    albedo, normal, mr,
                );
            }

            // Default material set (last entry — all-default textures).
            let def = num_material_sets - 1;
            write_material_set(
                &self.device, self.material_sets[def], self.sampler, rm,
                default_albedo, default_normal, default_mr,
            );
        }

        // --- Upload new meshes ---
        {
            let rm = self.resource_manager.as_mut().context("resource manager unavailable")?;
            for data in &scene_data.meshes {
                self.gpu_meshes.push(rm.upload_mesh(&data.vertices, &data.indices)?);
            }
        }

        log::info!(
            "Scene reloaded: {} meshes, {} materials, {} textures",
            self.gpu_meshes.len(),
            scene_data.materials.len(),
            self.scene_textures.len(),
        );
        Ok(())
    }

    /// Change the MSAA sample count. Recreates MSAA images and rebuilds affected pipelines.
    /// Calls `vkDeviceWaitIdle` — brief stutter, acceptable for a settings change.
    pub fn set_msaa_samples(&mut self, new_samples: vk::SampleCountFlags) -> Result<()> {
        if new_samples == self.msaa_samples {
            return Ok(());
        }

        // SAFETY: wait for all in-flight GPU work.
        unsafe { self.device.device_wait_idle()? };

        // Destroy old MSAA images.
        if let Some(ref mut rm) = self.resource_manager {
            if let Some(h) = self.msaa_color.take() { rm.destroy_image(h); }
            if let Some(h) = self.msaa_depth.take() { rm.destroy_image(h); }
        }

        self.msaa_samples = new_samples;

        // Create new MSAA images (skip if TYPE_1 = no MSAA).
        if new_samples != vk::SampleCountFlags::TYPE_1 {
            if let Some(ref mut rm) = self.resource_manager {
                let color = rm.create_msaa_image(
                    self.swapchain_extent.width,
                    self.swapchain_extent.height,
                    HDR_FORMAT, // MSAA color resolves to hdr_color
                    new_samples,
                    vk::ImageUsageFlags::COLOR_ATTACHMENT,
                    vk::ImageAspectFlags::COLOR,
                )?;
                let depth = rm.create_msaa_image(
                    self.swapchain_extent.width,
                    self.swapchain_extent.height,
                    self.depth_format,
                    new_samples,
                    vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
                    depth_aspect(self.depth_format),
                )?;
                self.msaa_color = Some(color);
                self.msaa_depth = Some(depth);
            }
        }

        // Rebuild graphics + skybox pipelines with the new sample count.
        let binding_descriptions = [super::vertex::Vertex::binding_description()];
        let attribute_descriptions = super::vertex::Vertex::attribute_descriptions();

        // SAFETY: device is idle, pipelines are not in use.
        unsafe { self.device.destroy_pipeline(self.graphics_pipeline, None) };
        self.graphics_pipeline = super::pipeline::build_graphics_pipeline(
            &self.device,
            super::pipeline::VERT_SPV,
            super::pipeline::FRAG_SPV,
            self.pipeline_layout,
            HDR_FORMAT, // main pass renders to hdr_color
            self.depth_format,
            &binding_descriptions,
            &attribute_descriptions,
            new_samples,
        )?;

        unsafe { self.device.destroy_pipeline(self.skybox_pipeline, None) };
        self.skybox_pipeline = super::pipeline::build_skybox_pipeline(
            &self.device,
            super::pipeline::SKYBOX_VERT_SPV,
            super::pipeline::SKYBOX_FRAG_SPV,
            self.skybox_pipeline_layout,
            HDR_FORMAT, // skybox renders to hdr_color
            self.depth_format,
            new_samples,
        )?;

        log::info!("MSAA changed to {:?}", new_samples);
        Ok(())
    }

    fn recreate_swapchain(&mut self, window: &Window) -> Result<()> {
        // SAFETY: device is valid — wait for all GPU work before touching swapchain.
        unsafe {
            self.device.device_wait_idle()?;
        }

        // Destroy old semaphores (count may change with new swapchain).
        for &sem in self.image_available_semaphores.iter().chain(&self.render_finished_semaphores) {
            // SAFETY: device is idle, semaphore is valid.
            unsafe {
                self.device.destroy_semaphore(sem, None);
            }
        }

        // Destroy old image views.
        for &view in &self.swapchain_image_views {
            // SAFETY: device is idle, view is valid.
            unsafe {
                self.device.destroy_image_view(view, None);
            }
        }

        let old_swapchain = self.swapchain;

        let (swapchain, images, format, extent) = create_swapchain(
            &self.swapchain_loader,
            self.physical_device,
            &self.surface_loader,
            self.surface,
            window,
            old_swapchain,
        )?;

        // SAFETY: device is idle, old swapchain was passed to the new one.
        unsafe {
            self.swapchain_loader
                .destroy_swapchain(old_swapchain, None);
        }

        self.swapchain = swapchain;
        self.swapchain_images = images;
        self.swapchain_format = format;
        self.swapchain_extent = extent;
        self.swapchain_image_views =
            create_image_views(&self.device, &self.swapchain_images, self.swapchain_format)?;

        // Recreate depth buffer + MSAA images + HDR color + bloom at the new size.
        if let Some(ref mut rm) = self.resource_manager {
            rm.destroy_image(self.depth_image);
            self.depth_image = rm.create_attachment_image(
                extent.width,
                extent.height,
                self.depth_format,
                vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
                depth_aspect(self.depth_format),
                vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                    | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                    | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
            )?;

            // Recreate MSAA images if active.
            if let Some(h) = self.msaa_color.take() { rm.destroy_image(h); }
            if let Some(h) = self.msaa_depth.take() { rm.destroy_image(h); }
            if self.msaa_samples != vk::SampleCountFlags::TYPE_1 {
                let color = rm.create_msaa_image(
                    extent.width, extent.height,
                    HDR_FORMAT, self.msaa_samples, // MSAA color uses HDR_FORMAT
                    vk::ImageUsageFlags::COLOR_ATTACHMENT,
                    vk::ImageAspectFlags::COLOR,
                )?;
                let depth = rm.create_msaa_image(
                    extent.width, extent.height,
                    self.depth_format, self.msaa_samples,
                    vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
                    depth_aspect(self.depth_format),
                )?;
                self.msaa_color = Some(color);
                self.msaa_depth = Some(depth);
            }

            // Recreate HDR color target.
            rm.destroy_image(self.hdr_color);
            self.hdr_color = rm.create_attachment_image(
                extent.width, extent.height,
                HDR_FORMAT,
                vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
                vk::ImageAspectFlags::COLOR,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::AccessFlags::SHADER_READ,
            )?;

            // Recreate bloom chain.
            for h in self.bloom_images.drain(..) { rm.destroy_image(h); }
            let mut new_bloom_extents = Vec::with_capacity(BLOOM_LEVELS);
            let mut w = extent.width.max(2) / 2;
            let mut h = extent.height.max(2) / 2;
            for _ in 0..BLOOM_LEVELS {
                let img = rm.create_attachment_image(
                    w, h, HDR_FORMAT,
                    vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
                    vk::ImageAspectFlags::COLOR,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::AccessFlags::SHADER_READ,
                )?;
                self.bloom_images.push(img);
                new_bloom_extents.push(vk::Extent2D { width: w, height: h });
                w = (w / 2).max(1);
                h = (h / 2).max(1);
            }
            self.bloom_extents = new_bloom_extents;

            // Re-write all bloom descriptor sets to point at the new image views.
            // ds[0] reads hdr_color, ds[i] reads bloom_images[i-1]
            for i in 0..BLOOM_LEVELS {
                let img_view = if i == 0 {
                    rm.get_image_view(self.hdr_color)
                } else {
                    rm.get_image_view(self.bloom_images[i - 1])
                };
                let info = [vk::DescriptorImageInfo {
                    sampler: self.bloom_sampler,
                    image_view: img_view,
                    image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                }];
                let write = [vk::WriteDescriptorSet::default()
                    .dst_set(self.bloom_downsample_sets[i])
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&info)];
                // SAFETY: set, image view, and sampler are valid.
                unsafe { self.device.update_descriptor_sets(&write, &[]) };
            }
            // us[i] reads bloom_images[i+1]
            for i in 0..(BLOOM_LEVELS - 1) {
                let info = [vk::DescriptorImageInfo {
                    sampler: self.bloom_sampler,
                    image_view: rm.get_image_view(self.bloom_images[i + 1]),
                    image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                }];
                let write = [vk::WriteDescriptorSet::default()
                    .dst_set(self.bloom_upsample_sets[i])
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&info)];
                // SAFETY: set, image view, and sampler are valid.
                unsafe { self.device.update_descriptor_sets(&write, &[]) };
            }
            // Composite: binding 0 = hdr_color, binding 1 = bloom_images[0]
            let hdr_img_info = [vk::DescriptorImageInfo {
                sampler: self.bloom_sampler,
                image_view: rm.get_image_view(self.hdr_color),
                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            }];
            let bloom_img_info = [vk::DescriptorImageInfo {
                sampler: self.bloom_sampler,
                image_view: rm.get_image_view(self.bloom_images[0]),
                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            }];
            let comp_writes = [
                vk::WriteDescriptorSet::default()
                    .dst_set(self.composite_descriptor_set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&hdr_img_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(self.composite_descriptor_set)
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&bloom_img_info),
            ];
            // SAFETY: set, image views, and sampler are valid.
            unsafe { self.device.update_descriptor_sets(&comp_writes, &[]) };
        }

        // Recreate semaphores for new image count.
        let semaphore_info = vk::SemaphoreCreateInfo::default();
        let image_count = self.swapchain_images.len();
        let mut new_acquire = Vec::with_capacity(image_count);
        let mut new_render_finished = Vec::with_capacity(image_count);
        for _ in 0..image_count {
            // SAFETY: device is valid, create info is well-formed.
            unsafe {
                new_acquire.push(self.device.create_semaphore(&semaphore_info, None)?);
                new_render_finished.push(self.device.create_semaphore(&semaphore_info, None)?);
            }
        }
        self.image_available_semaphores = new_acquire;
        self.render_finished_semaphores = new_render_finished;
        self.acquire_semaphore_index = 0;

        self.framebuffer_resized = false;

        log::info!(
            "Swapchain recreated: {}x{}",
            extent.width,
            extent.height
        );
        Ok(())
    }

    /// Explicit cleanup in reverse creation order.
    /// Do NOT rely on Rust `Drop` for Vulkan resources (see gotchas.md).
    pub fn destroy(&mut self) {
        unsafe {
            // SAFETY: device is valid — wait for all in-flight work first.
            self.device
                .device_wait_idle()
                .expect("Failed to wait for device idle");

            // Fences (per frame-in-flight)
            for i in 0..MAX_FRAMES_IN_FLIGHT {
                self.device.destroy_fence(self.in_flight_fences[i], None);
            }
            // Semaphores (per swapchain image)
            for &sem in self.image_available_semaphores.iter().chain(&self.render_finished_semaphores) {
                self.device.destroy_semaphore(sem, None);
            }

            // Command pool (implicitly frees its command buffers)
            self.device
                .destroy_command_pool(self.command_pool, None);

            // Image views
            for &view in &self.swapchain_image_views {
                self.device.destroy_image_view(view, None);
            }

            // Free all GPU allocations before dropping the allocator.
            // Order: UBO buffers → mesh buffers → textures → destroy_all (drops allocator).
            // gpu-allocator panics in debug if allocations survive the drop.
            if let Some(ref mut rm) = self.resource_manager {
                for &buf in &self.wireframe_vertex_buffers {
                    rm.destroy_buffer(buf);
                }
                for &buf in &self.gizmo_vertex_buffers {
                    rm.destroy_buffer(buf);
                }
                for &buf in &self.ubo_buffers {
                    rm.destroy_buffer(buf);
                }
                for mesh in self.gpu_meshes.drain(..) {
                    rm.destroy_mesh(mesh);
                }
                for &tex in &self.scene_textures {
                    rm.destroy_image(tex);
                }
                rm.destroy_image(self.default_albedo);
                rm.destroy_image(self.default_normal);
                rm.destroy_image(self.default_mr);
                rm.destroy_image(self.depth_image);
                if let Some(h) = self.msaa_color { rm.destroy_image(h); }
                if let Some(h) = self.msaa_depth { rm.destroy_image(h); }
                rm.destroy_image(self.hdr_color);
                for h in self.bloom_images.drain(..) { rm.destroy_image(h); }
                rm.destroy_image(self.shadow_map);
                rm.destroy_image(self.skybox_image);
                rm.destroy_image(self.irradiance_image);
                rm.destroy_image(self.prefiltered_image);
                rm.destroy_image(self.brdf_lut_image);
                rm.destroy_all();
            }
            self.resource_manager = None;

            // Material descriptor pool (frees its sets when destroyed).
            self.device
                .destroy_descriptor_pool(self.material_descriptor_pool, None);
            self.device
                .destroy_descriptor_set_layout(self.material_set_layout, None);

            // Bloom + composite descriptor pools + layouts + sampler.
            // bloom_descriptor_pool and composite_descriptor_pool share the same handle.
            self.device.destroy_descriptor_pool(self.bloom_descriptor_pool, None);
            // composite_descriptor_pool == bloom_descriptor_pool; do not destroy twice.
            self.device.destroy_descriptor_set_layout(self.composite_set_layout, None);
            self.device.destroy_descriptor_set_layout(self.bloom_set_layout, None);
            self.device.destroy_sampler(self.bloom_sampler, None);

            // Samplers
            self.device.destroy_sampler(self.shadow_sampler, None);
            self.device.destroy_sampler(self.ibl_sampler, None);
            self.device.destroy_sampler(self.sampler, None);

            // Skybox descriptor pool + layout (frees descriptor set when pool is destroyed).
            self.device.destroy_descriptor_pool(self.skybox_descriptor_pool, None);
            self.device.destroy_descriptor_set_layout(self.skybox_set_layout, None);

            // Lighting descriptor pool (frees its sets when destroyed).
            self.device.destroy_descriptor_pool(self.descriptor_pool, None);
            self.device
                .destroy_descriptor_set_layout(self.lighting_set_layout, None);

            // egui renderer (must be before device destruction)
            drop(self.egui_renderer.take());

            // Pipelines + layouts (before swapchain, reverse creation order)
            self.device.destroy_pipeline(self.composite_pipeline, None);
            self.device.destroy_pipeline_layout(self.composite_pipeline_layout, None);
            self.device.destroy_pipeline(self.bloom_upsample_pipeline, None);
            self.device.destroy_pipeline_layout(self.bloom_upsample_pipeline_layout, None);
            self.device.destroy_pipeline(self.bloom_downsample_pipeline, None);
            self.device.destroy_pipeline_layout(self.bloom_downsample_pipeline_layout, None);
            self.device.destroy_pipeline(self.wireframe_pipeline, None);
            self.device.destroy_pipeline_layout(self.wireframe_pipeline_layout, None);
            self.device.destroy_pipeline(self.skybox_pipeline, None);
            self.device.destroy_pipeline_layout(self.skybox_pipeline_layout, None);
            self.device
                .destroy_pipeline(self.graphics_pipeline, None);
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device
                .destroy_pipeline(self.shadow_pipeline, None);
            self.device
                .destroy_pipeline_layout(self.shadow_pipeline_layout, None);

            // Swapchain
            self.swapchain_loader
                .destroy_swapchain(self.swapchain, None);

            // Logical device
            self.device.destroy_device(None);

            // Surface
            self.surface_loader.destroy_surface(self.surface, None);

            // Debug messenger
            if let Some((ref loader, messenger)) = self.debug_utils {
                loader.destroy_debug_utils_messenger(messenger, None);
            }

            // Instance
            self.instance.destroy_instance(None);
        }

        log::info!("All Vulkan resources destroyed");
    }
}
