use std::ffi::{c_char, CStr};

use anyhow::{Context, Result};
use ash::vk;
use bytemuck;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::Window;

use gpu_allocator::MemoryLocation;

use super::gpu_resources::{BufferHandle, GpuMesh, GpuResourceManager, ImageHandle};
use super::vertex::Vertex;
use crate::asset::SceneData;
use crate::scene::{compute_light_mvp, LightingState, LightingUbo};

/// Push constant layout (128 bytes = Vulkan minimum).
/// bytes  0..64 = MVP matrix, bytes 64..128 = model matrix.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct MeshPushConstants {
    mvp:   glam::Mat4,
    model: glam::Mat4,
}

const MAX_FRAMES_IN_FLIGHT: usize = 2;

/// A mesh instance: GPU geometry + world transform + pre-resolved material descriptor set.
struct SceneInstance {
    mesh: GpuMesh,
    model: glam::Mat4,
    material_set: vk::DescriptorSet,
}

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
    instances: Vec<SceneInstance>,
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

// ---------------------------------------------------------------------------
// VulkanContext — public API
// ---------------------------------------------------------------------------

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

        // --- Shared sampler (linear, repeat, single mip) ---
        let sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::REPEAT)
            .address_mode_v(vk::SamplerAddressMode::REPEAT)
            .address_mode_w(vk::SamplerAddressMode::REPEAT)
            .anisotropy_enable(false)
            .min_lod(0.0)
            .max_lod(0.0);
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

        // --- Descriptor set layouts ---
        let lighting_set_layout =
            super::pipeline::create_lighting_descriptor_set_layout(&device)?;
        let material_set_layout =
            super::pipeline::create_material_descriptor_set_layout(&device)?;

        let binding_descriptions = [Vertex::binding_description()];
        let attribute_descriptions = Vertex::attribute_descriptions();
        let (pipeline_layout, graphics_pipeline) =
            super::pipeline::create_graphics_pipeline(
                &device,
                swapchain_format,
                depth_format,
                &binding_descriptions,
                &attribute_descriptions,
                lighting_set_layout,
                material_set_layout,
            )?;
        let (shadow_pipeline_layout, shadow_pipeline) =
            super::pipeline::create_shadow_pipeline(
                &device,
                depth_format,
                &binding_descriptions,
                &attribute_descriptions,
            )?;

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

        // --- Lighting descriptor pool (set 0): UBO (binding 0) + shadow sampler (binding 1) ---
        let lighting_pool_sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UNIFORM_BUFFER,
                descriptor_count: MAX_FRAMES_IN_FLIGHT as u32,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                descriptor_count: MAX_FRAMES_IN_FLIGHT as u32,
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

        // Point each descriptor set at its UBO buffer (binding 0) and shadow map (binding 1).
        let shadow_view = resource_manager.get_image_view(shadow_map);
        for (i, &set) in descriptor_sets.iter().enumerate() {
            let buffer = resource_manager.get_buffer(ubo_buffers[i]);
            let buffer_info = [vk::DescriptorBufferInfo {
                buffer,
                offset: 0,
                range: ubo_size,
            }];
            let shadow_image_info = [vk::DescriptorImageInfo {
                sampler: shadow_sampler,
                image_view: shadow_view,
                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            }];
            let writes = [
                vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&buffer_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&shadow_image_info),
            ];
            // SAFETY: set, buffer, image view, and sampler are valid.
            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }
        log::info!("Lighting descriptor sets created ({MAX_FRAMES_IN_FLIGHT} frames)");

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

        // Write descriptor sets for each glTF material.
        for (i, mat) in scene_data.materials.iter().enumerate() {
            let albedo_view = resource_manager
                .get_image_view(resolve_tex(mat.albedo_tex, default_albedo));
            let normal_view = resource_manager
                .get_image_view(resolve_tex(mat.normal_tex, default_normal));
            let mr_view = resource_manager
                .get_image_view(resolve_tex(mat.metallic_roughness_tex, default_mr));

            let image_infos = [
                vk::DescriptorImageInfo::default()
                    .sampler(sampler)
                    .image_view(albedo_view)
                    .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL),
                vk::DescriptorImageInfo::default()
                    .sampler(sampler)
                    .image_view(normal_view)
                    .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL),
                vk::DescriptorImageInfo::default()
                    .sampler(sampler)
                    .image_view(mr_view)
                    .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL),
            ];
            let writes: Vec<vk::WriteDescriptorSet> = (0u32..3)
                .map(|binding| {
                    vk::WriteDescriptorSet::default()
                        .dst_set(material_sets[i])
                        .dst_binding(binding)
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .image_info(&image_infos[binding as usize..binding as usize + 1])
                })
                .collect();
            // SAFETY: set, image views, and sampler are valid.
            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }

        // Write the default material set (last one) — all-default textures.
        let default_idx = num_material_sets - 1;
        {
            let image_infos = [
                vk::DescriptorImageInfo::default()
                    .sampler(sampler)
                    .image_view(resource_manager.get_image_view(default_albedo))
                    .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL),
                vk::DescriptorImageInfo::default()
                    .sampler(sampler)
                    .image_view(resource_manager.get_image_view(default_normal))
                    .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL),
                vk::DescriptorImageInfo::default()
                    .sampler(sampler)
                    .image_view(resource_manager.get_image_view(default_mr))
                    .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL),
            ];
            let writes: Vec<vk::WriteDescriptorSet> = (0u32..3)
                .map(|binding| {
                    vk::WriteDescriptorSet::default()
                        .dst_set(material_sets[default_idx])
                        .dst_binding(binding)
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .image_info(&image_infos[binding as usize..binding as usize + 1])
                })
                .collect();
            // SAFETY: default set, image views, and sampler are valid.
            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }
        log::info!("Material descriptor sets created ({} materials + 1 default)", scene_data.materials.len());

        // --- Upload meshes, resolve material descriptor sets per instance ---
        let default_material_set = material_sets[default_idx];
        let mut instances: Vec<SceneInstance> = Vec::with_capacity(scene_data.meshes.len());
        for data in &scene_data.meshes {
            let mesh = resource_manager.upload_mesh(&data.vertices, &data.indices)?;
            let material_set = data.material_index
                .and_then(|i| material_sets.get(i).copied())
                .unwrap_or(default_material_set);
            instances.push(SceneInstance { mesh, model: data.transform, material_set });
        }
        let (command_pool, command_buffers) =
            create_command_pool_and_buffers(&device, queue_family_index)?;
        let (image_available_semaphores, render_finished_semaphores, in_flight_fences) =
            create_sync_objects(&device, swapchain_images.len())?;

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
            instances,
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
        })
    }

    pub fn draw_frame(
        &mut self,
        window: &Window,
        view_proj: glam::Mat4,
        camera_pos: glam::Vec3,
        lights: &LightingState,
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
        let ubo = LightingUbo::from_state(lights, camera_pos);
        self.resource_manager
            .as_ref()
            .expect("resource manager alive")
            .write_buffer(self.ubo_buffers[self.current_frame], bytemuck::bytes_of(&ubo));

        // Record command buffer.
        let cmd = self.command_buffers[self.current_frame];
        let light_view_proj = compute_light_mvp(&lights.directional);
        self.record_command_buffer(cmd, image_index as usize, view_proj, light_view_proj)?;

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
        &self,
        cmd: vk::CommandBuffer,
        image_index: usize,
        view_proj: glam::Mat4,
        light_view_proj: glam::Mat4,
    ) -> Result<()> {
        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        // SAFETY: cmd is valid and not in use (fence was waited on).
        unsafe {
            self.device.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())?;
            self.device.begin_command_buffer(cmd, &begin_info)?;
        }

        let subresource_range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        };

        // Transition: UNDEFINED → COLOR_ATTACHMENT_OPTIMAL
        let barrier_to_color = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::empty())
            .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(self.swapchain_images[image_index])
            .subresource_range(subresource_range);

        // SAFETY: cmd is recording, barrier data is valid.
        unsafe {
            self.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier_to_color],
            );
        }

        // -----------------------------------------------------------------------
        // Shadow pass: render scene depth from the directional light's perspective.
        // -----------------------------------------------------------------------
        let shadow_subresource = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::DEPTH,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        };
        let rm = self.resource_manager.as_ref().expect("resource manager alive");

        // Barrier: SHADER_READ_ONLY → DEPTH_STENCIL_ATTACHMENT_OPTIMAL
        let shadow_to_write = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_READ)
            .dst_access_mask(
                vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                    | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
            )
            .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .new_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(rm.get_image_raw(self.shadow_map))
            .subresource_range(shadow_subresource);

        // SAFETY: cmd is recording, barrier data is valid.
        unsafe {
            self.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                    | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[shadow_to_write],
            );
        }

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

            for instance in &self.instances {
                // Push constant: full MVP from light's perspective = light_view_proj * model.
                let light_mvp: glam::Mat4 = light_view_proj * instance.model;
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
                    &[rm.get_buffer(instance.mesh.vertex_buffer)],
                    &[0],
                );
                self.device.cmd_bind_index_buffer(
                    cmd,
                    rm.get_buffer(instance.mesh.index_buffer),
                    0,
                    vk::IndexType::UINT32,
                );
                self.device
                    .cmd_draw_indexed(cmd, instance.mesh.index_count, 1, 0, 0, 0);
            }

            self.dynamic_rendering_loader.cmd_end_rendering(cmd);
        }

        // Barrier: DEPTH_STENCIL_ATTACHMENT_OPTIMAL → SHADER_READ_ONLY_OPTIMAL
        let shadow_to_read = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .old_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(rm.get_image_raw(self.shadow_map))
            .subresource_range(shadow_subresource);

        // SAFETY: cmd is recording, barrier data is valid.
        unsafe {
            self.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                    | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[shadow_to_read],
            );
        }

        // Dynamic rendering with color + depth.
        let color_attachment = vk::RenderingAttachmentInfo::default()
            .image_view(self.swapchain_image_views[image_index])
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.01, 0.01, 0.05, 1.0],
                },
            });

        let depth_view = rm.get_image_view(self.depth_image);
        let depth_attachment = vk::RenderingAttachmentInfo::default()
            .image_view(depth_view)
            .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::DONT_CARE) // no need to read depth after render
            .clear_value(vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue { depth: 1.0, stencil: 0 },
            });

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

            for instance in &self.instances {
                // Bind per-material textures (set 1).
                self.device.cmd_bind_descriptor_sets(
                    cmd,
                    vk::PipelineBindPoint::GRAPHICS,
                    self.pipeline_layout,
                    1,
                    &[instance.material_set],
                    &[],
                );

                let mvp = view_proj * instance.model;
                let pc = MeshPushConstants { mvp, model: instance.model };
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
                    &[rm.get_buffer(instance.mesh.vertex_buffer)],
                    &[0],
                );
                self.device.cmd_bind_index_buffer(
                    cmd,
                    rm.get_buffer(instance.mesh.index_buffer),
                    0,
                    vk::IndexType::UINT32,
                );
                // SAFETY: index_count matches what was uploaded; no index buffer overflow.
                self.device
                    .cmd_draw_indexed(cmd, instance.mesh.index_count, 1, 0, 0, 0);
            }

            self.dynamic_rendering_loader.cmd_end_rendering(cmd);
        }

        // Transition: COLOR_ATTACHMENT_OPTIMAL → PRESENT_SRC_KHR
        let barrier_to_present = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
            .dst_access_mask(vk::AccessFlags::empty())
            .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(self.swapchain_images[image_index])
            .subresource_range(subresource_range);

        // SAFETY: cmd is recording, barrier data is valid.
        unsafe {
            self.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier_to_present],
            );
        }

        // SAFETY: cmd is recording with a matching begin.
        unsafe {
            self.device.end_command_buffer(cmd)?;
        }

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

        // Recreate depth buffer at the new size.
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
                for &buf in &self.ubo_buffers {
                    rm.destroy_buffer(buf);
                }
                for instance in self.instances.drain(..) {
                    rm.destroy_mesh(instance.mesh);
                }
                for &tex in &self.scene_textures {
                    rm.destroy_image(tex);
                }
                rm.destroy_image(self.default_albedo);
                rm.destroy_image(self.default_normal);
                rm.destroy_image(self.default_mr);
                rm.destroy_image(self.depth_image);
                rm.destroy_image(self.shadow_map);
                rm.destroy_all();
            }
            self.resource_manager = None;

            // Material descriptor pool (frees its sets when destroyed).
            self.device
                .destroy_descriptor_pool(self.material_descriptor_pool, None);
            self.device
                .destroy_descriptor_set_layout(self.material_set_layout, None);

            // Samplers
            self.device.destroy_sampler(self.shadow_sampler, None);
            self.device.destroy_sampler(self.sampler, None);

            // Lighting descriptor pool (frees its sets when destroyed).
            self.device.destroy_descriptor_pool(self.descriptor_pool, None);
            self.device
                .destroy_descriptor_set_layout(self.lighting_set_layout, None);

            // Pipelines + layouts (before swapchain, reverse creation order)
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
