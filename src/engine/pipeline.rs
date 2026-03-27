use anyhow::{Context, Result};
use ash::vk;

// Built-in SPIR-V binaries embedded at compile time.
// Used for initial pipeline creation in VulkanContext::new().
pub(super) const VERT_SPV:        &[u8] = include_bytes!("../../shaders/triangle.vert.spv");
pub(super) const FRAG_SPV:        &[u8] = include_bytes!("../../shaders/triangle.frag.spv");
pub(super) const SHADOW_VERT_SPV: &[u8] = include_bytes!("../../shaders/shadow.vert.spv");
pub(super) const SHADOW_FRAG_SPV: &[u8] = include_bytes!("../../shaders/shadow.frag.spv");
pub(super) const SKYBOX_VERT_SPV: &[u8] = include_bytes!("../../shaders/skybox.vert.spv");
pub(super) const SKYBOX_FRAG_SPV: &[u8] = include_bytes!("../../shaders/skybox.frag.spv");
pub(super) const WIREFRAME_VERT_SPV: &[u8] = include_bytes!("../../shaders/wireframe.vert.spv");
pub(super) const WIREFRAME_FRAG_SPV: &[u8] = include_bytes!("../../shaders/wireframe.frag.spv");
pub(super) const FULLSCREEN_VERT_SPV:  &[u8] = include_bytes!("../../shaders/fullscreen.vert.spv");
pub(super) const BLOOM_DOWN_FRAG_SPV:  &[u8] = include_bytes!("../../shaders/bloom_downsample.frag.spv");
pub(super) const BLOOM_UP_FRAG_SPV:    &[u8] = include_bytes!("../../shaders/bloom_upsample.frag.spv");
pub(super) const COMPOSITE_FRAG_SPV:   &[u8] = include_bytes!("../../shaders/composite.frag.spv");
pub(super) const SSAO_FRAG_SPV:        &[u8] = include_bytes!("../../shaders/ssao.frag.spv");
pub(super) const SSAO_BLUR_FRAG_SPV:   &[u8] = include_bytes!("../../shaders/ssao_blur.frag.spv");
pub(super) const CULL_COMP_SPV:        &[u8] = include_bytes!("../../shaders/cull.comp.spv");
pub(super) const PARTICLE_VERT_SPV:    &[u8] = include_bytes!("../../shaders/particle.vert.spv");
pub(super) const PARTICLE_FRAG_SPV:    &[u8] = include_bytes!("../../shaders/particle.frag.spv");

fn create_shader_module(device: &ash::Device, spv: &[u8]) -> Result<vk::ShaderModule> {
    assert!(spv.len() % 4 == 0, "SPIR-V binary size must be a multiple of 4");
    let code: Vec<u32> = spv
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();
    let create_info = vk::ShaderModuleCreateInfo::default().code(&code);
    // SAFETY: device is valid, code is valid SPIR-V.
    unsafe { device.create_shader_module(&create_info, None) }
        .context("Failed to create shader module")
}

/// Set 0: lighting UBO (binding 0, VERTEX+FRAGMENT), shadow map (binding 1),
/// irradiance cubemap (binding 2), prefiltered env (binding 3), BRDF LUT (binding 4),
/// instance SSBO (binding 5, VERTEX — model matrices for GPU-driven indirect draw).
pub fn create_lighting_descriptor_set_layout(
    device: &ash::Device,
) -> Result<vk::DescriptorSetLayout> {
    let bindings = [
        // binding 0: LightingUbo — VERTEX stage for view_proj, FRAGMENT for lighting
        vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
        vk::DescriptorSetLayoutBinding::default()
            .binding(1)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        vk::DescriptorSetLayoutBinding::default()
            .binding(2)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        vk::DescriptorSetLayoutBinding::default()
            .binding(3)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        vk::DescriptorSetLayoutBinding::default()
            .binding(4)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        // binding 5: instance SSBO — vertex shader reads model matrices via gl_BaseInstance
        vk::DescriptorSetLayoutBinding::default()
            .binding(5)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX),
    ];
    let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
    // SAFETY: device is valid, binding info is well-formed.
    unsafe { device.create_descriptor_set_layout(&info, None) }
        .context("Failed to create lighting descriptor set layout")
}

/// Descriptor set layout for the GPU cull compute pass.
/// Set 0: LightingUbo (binding 0, COMPUTE), instance SSBO in (binding 1),
///        mesh-info SSBO (binding 2), draw-command SSBO out (binding 3).
pub fn create_cull_descriptor_set_layout(
    device: &ash::Device,
) -> Result<vk::DescriptorSetLayout> {
    let bindings = [
        vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::COMPUTE),
        vk::DescriptorSetLayoutBinding::default()
            .binding(1)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::COMPUTE),
        vk::DescriptorSetLayoutBinding::default()
            .binding(2)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::COMPUTE),
        vk::DescriptorSetLayoutBinding::default()
            .binding(3)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::COMPUTE),
    ];
    let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
    // SAFETY: device is valid.
    unsafe { device.create_descriptor_set_layout(&info, None) }
        .context("Failed to create cull descriptor set layout")
}

/// Skybox set layout: binding 0 = samplerCube (fragment stage).
pub fn create_skybox_descriptor_set_layout(
    device: &ash::Device,
) -> Result<vk::DescriptorSetLayout> {
    let bindings = [
        vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
    ];
    let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
    // SAFETY: device is valid.
    unsafe { device.create_descriptor_set_layout(&info, None) }
        .context("Failed to create skybox descriptor set layout")
}

/// Set 1: three COMBINED_IMAGE_SAMPLER bindings — albedo, normal map, metallic-roughness.
pub fn create_material_descriptor_set_layout(
    device: &ash::Device,
) -> Result<vk::DescriptorSetLayout> {
    let bindings = [
        vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        vk::DescriptorSetLayoutBinding::default()
            .binding(1)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        vk::DescriptorSetLayoutBinding::default()
            .binding(2)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
    ];
    let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
    // SAFETY: device is valid.
    unsafe { device.create_descriptor_set_layout(&info, None) }
        .context("Failed to create material descriptor set layout")
}

// ---------------------------------------------------------------------------
// Internal helpers — build just the vk::Pipeline given an existing layout.
// These are called both by the full create_xxx functions (initial setup)
// and by VulkanContext::recreate_pipeline() (hot-reload).
// ---------------------------------------------------------------------------

pub(super) fn build_graphics_pipeline(
    device: &ash::Device,
    vert_spv: &[u8],
    frag_spv: &[u8],
    pipeline_layout: vk::PipelineLayout,
    swapchain_format: vk::Format,
    depth_format: vk::Format,
    binding_descriptions: &[vk::VertexInputBindingDescription],
    attribute_descriptions: &[vk::VertexInputAttributeDescription],
    rasterization_samples: vk::SampleCountFlags,
) -> Result<vk::Pipeline> {
    let vert_module = create_shader_module(device, vert_spv)?;
    let frag_module = create_shader_module(device, frag_spv)?;

    let entry_point = c"main";
    let shader_stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert_module)
            .name(entry_point),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag_module)
            .name(entry_point),
    ];

    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(binding_descriptions)
        .vertex_attribute_descriptions(attribute_descriptions);

    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
        .primitive_restart_enable(false);

    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);

    let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
        .depth_clamp_enable(false)
        .rasterizer_discard_enable(false)
        .polygon_mode(vk::PolygonMode::FILL)
        .line_width(1.0)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .depth_bias_enable(false);

    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .sample_shading_enable(false)
        .rasterization_samples(rasterization_samples);

    let color_blend_attachment = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(false);
    let color_blend_attachments = [color_blend_attachment];
    let color_blend = vk::PipelineColorBlendStateCreateInfo::default()
        .logic_op_enable(false)
        .attachments(&color_blend_attachments);

    let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(true)
        .depth_write_enable(true)
        .depth_compare_op(vk::CompareOp::LESS)
        .depth_bounds_test_enable(false)
        .stencil_test_enable(false);

    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

    let color_formats = [swapchain_format];
    let mut rendering_info = vk::PipelineRenderingCreateInfo::default()
        .color_attachment_formats(&color_formats)
        .depth_attachment_format(depth_format);

    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&shader_stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterization)
        .multisample_state(&multisample)
        .color_blend_state(&color_blend)
        .depth_stencil_state(&depth_stencil)
        .dynamic_state(&dynamic_state)
        .layout(pipeline_layout)
        .push_next(&mut rendering_info);

    // SAFETY: device is valid, all referenced state lives on the stack.
    let pipelines = unsafe {
        device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
    }
    .map_err(|(_pipelines, err)| err)
    .context("Failed to create graphics pipeline")?;

    // Shader modules baked into pipeline — safe to destroy.
    // SAFETY: device is valid, modules are no longer referenced.
    unsafe {
        device.destroy_shader_module(vert_module, None);
        device.destroy_shader_module(frag_module, None);
    }

    Ok(pipelines[0])
}

pub(super) fn build_shadow_pipeline(
    device: &ash::Device,
    vert_spv: &[u8],
    frag_spv: &[u8],
    pipeline_layout: vk::PipelineLayout,
    depth_format: vk::Format,
    binding_descriptions: &[vk::VertexInputBindingDescription],
    attribute_descriptions: &[vk::VertexInputAttributeDescription],
) -> Result<vk::Pipeline> {
    let vert_module = create_shader_module(device, vert_spv)?;
    let frag_module = create_shader_module(device, frag_spv)?;

    let entry_point = c"main";
    let shader_stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert_module)
            .name(entry_point),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag_module)
            .name(entry_point),
    ];

    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(binding_descriptions)
        .vertex_attribute_descriptions(attribute_descriptions);

    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
        .primitive_restart_enable(false);

    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);

    let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
        .depth_clamp_enable(false)
        .rasterizer_discard_enable(false)
        .polygon_mode(vk::PolygonMode::FILL)
        .line_width(1.0)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .depth_bias_enable(true)
        .depth_bias_constant_factor(4.0)
        .depth_bias_slope_factor(1.5)
        .depth_bias_clamp(0.0);

    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .sample_shading_enable(false)
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);

    // No color attachments — depth-only pass.
    let color_blend = vk::PipelineColorBlendStateCreateInfo::default().logic_op_enable(false);

    let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(true)
        .depth_write_enable(true)
        .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL)
        .depth_bounds_test_enable(false)
        .stencil_test_enable(false);

    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

    let mut rendering_info =
        vk::PipelineRenderingCreateInfo::default().depth_attachment_format(depth_format);

    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&shader_stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterization)
        .multisample_state(&multisample)
        .color_blend_state(&color_blend)
        .depth_stencil_state(&depth_stencil)
        .dynamic_state(&dynamic_state)
        .layout(pipeline_layout)
        .push_next(&mut rendering_info);

    // SAFETY: device is valid.
    let pipelines = unsafe {
        device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
    }
    .map_err(|(_pipelines, err)| err)
    .context("Failed to create shadow pipeline")?;

    // SAFETY: device is valid, modules are no longer referenced.
    unsafe {
        device.destroy_shader_module(vert_module, None);
        device.destroy_shader_module(frag_module, None);
    }

    Ok(pipelines[0])
}

pub(super) fn build_skybox_pipeline(
    device: &ash::Device,
    vert_spv: &[u8],
    frag_spv: &[u8],
    pipeline_layout: vk::PipelineLayout,
    swapchain_format: vk::Format,
    depth_format: vk::Format,
    rasterization_samples: vk::SampleCountFlags,
) -> Result<vk::Pipeline> {
    let vert_module = create_shader_module(device, vert_spv)?;
    let frag_module = create_shader_module(device, frag_spv)?;

    let entry_point = c"main";
    let shader_stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert_module)
            .name(entry_point),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag_module)
            .name(entry_point),
    ];

    // No vertex input — positions generated from gl_VertexIndex.
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();

    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
        .primitive_restart_enable(false);

    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);

    let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
        .depth_clamp_enable(false)
        .rasterizer_discard_enable(false)
        .polygon_mode(vk::PolygonMode::FILL)
        .line_width(1.0)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .depth_bias_enable(false);

    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .sample_shading_enable(false)
        .rasterization_samples(rasterization_samples);

    let color_blend_attachment = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(false);
    let color_blend_attachments = [color_blend_attachment];
    let color_blend = vk::PipelineColorBlendStateCreateInfo::default()
        .logic_op_enable(false)
        .attachments(&color_blend_attachments);

    // Depth LESS_OR_EQUAL (skybox at z=1.0 passes where nothing was drawn), write OFF.
    let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(true)
        .depth_write_enable(false)
        .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL)
        .depth_bounds_test_enable(false)
        .stencil_test_enable(false);

    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

    let color_formats = [swapchain_format];
    let mut rendering_info = vk::PipelineRenderingCreateInfo::default()
        .color_attachment_formats(&color_formats)
        .depth_attachment_format(depth_format);

    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&shader_stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterization)
        .multisample_state(&multisample)
        .color_blend_state(&color_blend)
        .depth_stencil_state(&depth_stencil)
        .dynamic_state(&dynamic_state)
        .layout(pipeline_layout)
        .push_next(&mut rendering_info);

    // SAFETY: device is valid.
    let pipelines = unsafe {
        device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
    }
    .map_err(|(_pipelines, err)| err)
    .context("Failed to create skybox pipeline")?;

    // SAFETY: device is valid, modules are no longer referenced.
    unsafe {
        device.destroy_shader_module(vert_module, None);
        device.destroy_shader_module(frag_module, None);
    }

    Ok(pipelines[0])
}

// ---------------------------------------------------------------------------
// Full create_xxx functions — create layout + pipeline together (used in new()).
// ---------------------------------------------------------------------------

/// Create the graphics pipeline for the mesh PBR pass.
///
/// Push constants (vertex stage):
///   - bytes   0..64: MVP matrix (mat4)
///   - bytes  64..128: model matrix (mat4)
///
/// Returns `(PipelineLayout, Pipeline)`. Caller owns and must destroy both.
pub fn create_graphics_pipeline(
    device: &ash::Device,
    vert_spv: &[u8],
    frag_spv: &[u8],
    swapchain_format: vk::Format,
    depth_format: vk::Format,
    binding_descriptions: &[vk::VertexInputBindingDescription],
    attribute_descriptions: &[vk::VertexInputAttributeDescription],
    lighting_set_layout: vk::DescriptorSetLayout,   // set 0
    material_set_layout: vk::DescriptorSetLayout,   // set 1
    rasterization_samples: vk::SampleCountFlags,
) -> Result<(vk::PipelineLayout, vk::Pipeline)> {
    // Fase 23: no push constants — model matrices come from the instance SSBO
    // (set 0 binding 5); view_proj comes from LightingUbo (set 0 binding 0).
    let set_layouts = [lighting_set_layout, material_set_layout];
    let layout_info = vk::PipelineLayoutCreateInfo::default()
        .set_layouts(&set_layouts);
    // SAFETY: device is valid.
    let pipeline_layout = unsafe { device.create_pipeline_layout(&layout_info, None) }
        .context("Failed to create pipeline layout")?;

    let pipeline = build_graphics_pipeline(
        device,
        vert_spv,
        frag_spv,
        pipeline_layout,
        swapchain_format,
        depth_format,
        binding_descriptions,
        attribute_descriptions,
        rasterization_samples,
    )?;

    log::info!("Graphics pipeline created (Cook-Torrance PBR, GPU-driven indirect)");
    Ok((pipeline_layout, pipeline))
}

/// Compute pipeline for GPU frustum culling (cull.comp).
/// Returns `(PipelineLayout, Pipeline)`. Caller owns and must destroy both.
pub fn create_cull_pipeline(
    device: &ash::Device,
    comp_spv: &[u8],
    cull_set_layout: vk::DescriptorSetLayout,
) -> Result<(vk::PipelineLayout, vk::Pipeline)> {
    // Push constant: {instance_count: u32, lod_distance_step: f32} — 8 bytes.
    let pc_range = vk::PushConstantRange::default()
        .stage_flags(vk::ShaderStageFlags::COMPUTE)
        .offset(0)
        .size(8);
    let set_layouts = [cull_set_layout];
    let layout_info = vk::PipelineLayoutCreateInfo::default()
        .set_layouts(&set_layouts)
        .push_constant_ranges(std::slice::from_ref(&pc_range));
    // SAFETY: device is valid.
    let layout = unsafe { device.create_pipeline_layout(&layout_info, None) }
        .context("Failed to create cull pipeline layout")?;

    let module = create_shader_module(device, comp_spv)?;
    let entry = c"main";
    let stage = vk::PipelineShaderStageCreateInfo::default()
        .stage(vk::ShaderStageFlags::COMPUTE)
        .module(module)
        .name(entry);
    let pipeline_info = vk::ComputePipelineCreateInfo::default()
        .layout(layout)
        .stage(stage);
    // SAFETY: device and layout are valid; shader module is well-formed SPIR-V.
    let pipeline = unsafe {
        device.create_compute_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
    }
    .map_err(|(_, e)| e)
    .context("Failed to create cull compute pipeline")?
    .remove(0);

    // SAFETY: module is no longer needed after pipeline creation.
    unsafe { device.destroy_shader_module(module, None) };

    log::info!("Cull compute pipeline created (GPU frustum culling, 64 threads/workgroup)");
    Ok((layout, pipeline))
}

/// Depth-only pipeline for the shadow map pass.
///
/// Push constants (vertex stage): 64 bytes — `light_view_proj * model` (mat4).
/// No descriptor sets — everything goes through push constants.
///
/// Returns `(PipelineLayout, Pipeline)`. Caller owns and must destroy both.
pub fn create_shadow_pipeline(
    device: &ash::Device,
    vert_spv: &[u8],
    frag_spv: &[u8],
    depth_format: vk::Format,
    binding_descriptions: &[vk::VertexInputBindingDescription],
    attribute_descriptions: &[vk::VertexInputAttributeDescription],
) -> Result<(vk::PipelineLayout, vk::Pipeline)> {
    // Push constant: 64 bytes (lightMVP = light_view_proj * model), vertex stage.
    let push_constant_range = vk::PushConstantRange::default()
        .stage_flags(vk::ShaderStageFlags::VERTEX)
        .offset(0)
        .size(64);
    let push_constant_ranges = [push_constant_range];

    // No descriptor set layouts — shadow pass only needs the MVP push constant.
    let layout_info = vk::PipelineLayoutCreateInfo::default()
        .push_constant_ranges(&push_constant_ranges);
    // SAFETY: device is valid.
    let pipeline_layout = unsafe { device.create_pipeline_layout(&layout_info, None) }
        .context("Failed to create shadow pipeline layout")?;

    let pipeline = build_shadow_pipeline(
        device,
        vert_spv,
        frag_spv,
        pipeline_layout,
        depth_format,
        binding_descriptions,
        attribute_descriptions,
    )?;

    log::info!("Shadow pipeline created (depth-only, 2048×2048)");
    Ok((pipeline_layout, pipeline))
}

/// Wireframe debug pipeline: LINE_LIST, no depth test, position-only vertices.
///
/// Push constants (vertex stage): 80 bytes — view_proj (mat4, 0..64) + color (vec4, 64..80).
/// No descriptor sets.
///
/// Returns `(PipelineLayout, Pipeline)`. Caller owns and must destroy both.
pub fn create_wireframe_pipeline(
    device: &ash::Device,
    swapchain_format: vk::Format,
    binding_descriptions: &[vk::VertexInputBindingDescription],
    attribute_descriptions: &[vk::VertexInputAttributeDescription],
) -> Result<(vk::PipelineLayout, vk::Pipeline)> {
    // Push constants: view_proj (64 bytes) + color (16 bytes) = 80 bytes, vertex stage.
    let push_constant_range = vk::PushConstantRange::default()
        .stage_flags(vk::ShaderStageFlags::VERTEX)
        .offset(0)
        .size(80);
    let push_constant_ranges = [push_constant_range];

    let layout_info = vk::PipelineLayoutCreateInfo::default()
        .push_constant_ranges(&push_constant_ranges);
    // SAFETY: device is valid.
    let pipeline_layout = unsafe { device.create_pipeline_layout(&layout_info, None) }
        .context("Failed to create wireframe pipeline layout")?;

    let vert_module = create_shader_module(device, WIREFRAME_VERT_SPV)?;
    let frag_module = create_shader_module(device, WIREFRAME_FRAG_SPV)?;

    let entry_point = c"main";
    let shader_stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert_module)
            .name(entry_point),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag_module)
            .name(entry_point),
    ];

    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(binding_descriptions)
        .vertex_attribute_descriptions(attribute_descriptions);

    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::LINE_LIST)
        .primitive_restart_enable(false);

    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);

    let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
        .depth_clamp_enable(false)
        .rasterizer_discard_enable(false)
        .polygon_mode(vk::PolygonMode::FILL)
        .line_width(1.0)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .depth_bias_enable(false);

    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .sample_shading_enable(false)
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);

    let color_blend_attachment = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(false);
    let color_blend_attachments = [color_blend_attachment];
    let color_blend = vk::PipelineColorBlendStateCreateInfo::default()
        .logic_op_enable(false)
        .attachments(&color_blend_attachments);

    // No depth test — wireframes always visible on top.
    let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(false)
        .depth_write_enable(false)
        .depth_compare_op(vk::CompareOp::ALWAYS)
        .depth_bounds_test_enable(false)
        .stencil_test_enable(false);

    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

    // No depth attachment — wireframe renders after the main pass resolved to swapchain.
    let color_formats = [swapchain_format];
    let mut rendering_info = vk::PipelineRenderingCreateInfo::default()
        .color_attachment_formats(&color_formats);

    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&shader_stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterization)
        .multisample_state(&multisample)
        .color_blend_state(&color_blend)
        .depth_stencil_state(&depth_stencil)
        .dynamic_state(&dynamic_state)
        .layout(pipeline_layout)
        .push_next(&mut rendering_info);

    // SAFETY: device is valid.
    let pipelines = unsafe {
        device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
    }
    .map_err(|(_pipelines, err)| err)
    .context("Failed to create wireframe pipeline")?;

    // SAFETY: modules baked into pipeline — safe to destroy.
    unsafe {
        device.destroy_shader_module(vert_module, None);
        device.destroy_shader_module(frag_module, None);
    }

    log::info!("Wireframe pipeline created (LINE_LIST, no depth test, 80-byte push constants)");
    Ok((pipeline_layout, pipelines[0]))
}

/// Bloom descriptor set layout: binding 0 = COMBINED_IMAGE_SAMPLER (fragment stage).
/// Used for both downsample and upsample passes (each reads one source image).
pub fn create_bloom_descriptor_set_layout(
    device: &ash::Device,
) -> Result<vk::DescriptorSetLayout> {
    let bindings = [
        vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
    ];
    let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
    // SAFETY: device is valid, binding info is well-formed.
    unsafe { device.create_descriptor_set_layout(&info, None) }
        .context("Failed to create bloom descriptor set layout")
}

/// Composite descriptor set layout:
///   binding 0 = HDR color, binding 1 = bloom result, binding 2 = SSAO AO texture.
/// All COMBINED_IMAGE_SAMPLER in the fragment stage.
pub fn create_composite_descriptor_set_layout(
    device: &ash::Device,
) -> Result<vk::DescriptorSetLayout> {
    let bindings = [
        vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        vk::DescriptorSetLayoutBinding::default()
            .binding(1)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        vk::DescriptorSetLayoutBinding::default()
            .binding(2)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
    ];
    let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
    // SAFETY: device is valid, binding info is well-formed.
    unsafe { device.create_descriptor_set_layout(&info, None) }
        .context("Failed to create composite descriptor set layout")
}

/// SSAO descriptor set layout (set 0):
///   binding 0 = depth buffer (sampler2D)
///   binding 1 = noise texture (sampler2D)
///   binding 2 = SsaoUbo (UNIFORM_BUFFER, std140)
pub fn create_ssao_descriptor_set_layout(
    device: &ash::Device,
) -> Result<vk::DescriptorSetLayout> {
    let bindings = [
        vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        vk::DescriptorSetLayoutBinding::default()
            .binding(1)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        vk::DescriptorSetLayoutBinding::default()
            .binding(2)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
    ];
    let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
    // SAFETY: device is valid, binding info is well-formed.
    unsafe { device.create_descriptor_set_layout(&info, None) }
        .context("Failed to create SSAO descriptor set layout")
}

/// SSAO blur descriptor set layout (set 0): binding 0 = ssao_raw (sampler2D).
pub fn create_ssao_blur_descriptor_set_layout(
    device: &ash::Device,
) -> Result<vk::DescriptorSetLayout> {
    let bindings = [
        vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
    ];
    let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
    // SAFETY: device is valid, binding info is well-formed.
    unsafe { device.create_descriptor_set_layout(&info, None) }
        .context("Failed to create SSAO blur descriptor set layout")
}

/// SSAO fullscreen pipeline. No push constants. Renders R8_UNORM AO to ssao_raw_image.
/// Returns `(PipelineLayout, Pipeline)`.
pub fn create_ssao_pipeline(
    device: &ash::Device,
    set_layout: vk::DescriptorSetLayout,
) -> Result<(vk::PipelineLayout, vk::Pipeline)> {
    let set_layouts = [set_layout];
    let layout_info = vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts);
    // SAFETY: device is valid.
    let pipeline_layout = unsafe { device.create_pipeline_layout(&layout_info, None) }
        .context("Failed to create SSAO pipeline layout")?;

    let pipeline = build_fullscreen_frag_pipeline(
        device,
        FULLSCREEN_VERT_SPV,
        SSAO_FRAG_SPV,
        pipeline_layout,
        vk::Format::R8_UNORM,
    )?;

    log::info!("SSAO pipeline created");
    Ok((pipeline_layout, pipeline))
}

/// SSAO blur pipeline. No push constants. Renders R8_UNORM to ssao_blurred_image.
/// Returns `(PipelineLayout, Pipeline)`.
pub fn create_ssao_blur_pipeline(
    device: &ash::Device,
    set_layout: vk::DescriptorSetLayout,
) -> Result<(vk::PipelineLayout, vk::Pipeline)> {
    let set_layouts = [set_layout];
    let layout_info = vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts);
    // SAFETY: device is valid.
    let pipeline_layout = unsafe { device.create_pipeline_layout(&layout_info, None) }
        .context("Failed to create SSAO blur pipeline layout")?;

    let pipeline = build_fullscreen_frag_pipeline(
        device,
        FULLSCREEN_VERT_SPV,
        SSAO_BLUR_FRAG_SPV,
        pipeline_layout,
        vk::Format::R8_UNORM,
    )?;

    log::info!("SSAO blur pipeline created");
    Ok((pipeline_layout, pipeline))
}

/// Shared helper: fullscreen triangle pipeline with a single color attachment.
/// No vertex input, no depth test, MSAA TYPE_1, no push constants.
fn build_fullscreen_frag_pipeline(
    device: &ash::Device,
    vert_spv: &[u8],
    frag_spv: &[u8],
    pipeline_layout: vk::PipelineLayout,
    color_format: vk::Format,
) -> Result<vk::Pipeline> {
    let vert_module = create_shader_module(device, vert_spv)?;
    let frag_module = create_shader_module(device, frag_spv)?;

    let entry_point = c"main";
    let shader_stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert_module)
            .name(entry_point),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag_module)
            .name(entry_point),
    ];

    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
        .primitive_restart_enable(false);
    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
        .depth_clamp_enable(false)
        .rasterizer_discard_enable(false)
        .polygon_mode(vk::PolygonMode::FILL)
        .line_width(1.0)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .depth_bias_enable(false);
    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .sample_shading_enable(false)
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let color_blend_attachment = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(false);
    let color_blend_attachments = [color_blend_attachment];
    let color_blend = vk::PipelineColorBlendStateCreateInfo::default()
        .logic_op_enable(false)
        .attachments(&color_blend_attachments);
    let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(false)
        .depth_write_enable(false)
        .depth_compare_op(vk::CompareOp::ALWAYS)
        .depth_bounds_test_enable(false)
        .stencil_test_enable(false);
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state = vk::PipelineDynamicStateCreateInfo::default()
        .dynamic_states(&dynamic_states);
    let color_formats = [color_format];
    let mut rendering_info = vk::PipelineRenderingCreateInfo::default()
        .color_attachment_formats(&color_formats);

    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&shader_stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterization)
        .multisample_state(&multisample)
        .color_blend_state(&color_blend)
        .depth_stencil_state(&depth_stencil)
        .dynamic_state(&dynamic_state)
        .layout(pipeline_layout)
        .push_next(&mut rendering_info);

    // SAFETY: device is valid.
    let pipelines = unsafe {
        device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
    }
    .map_err(|(_p, e)| e)
    .context("Failed to create fullscreen pipeline")?;

    // SAFETY: shader modules baked into pipeline — safe to destroy.
    unsafe {
        device.destroy_shader_module(vert_module, None);
        device.destroy_shader_module(frag_module, None);
    }

    Ok(pipelines[0])
}

/// Bloom downsample/upsample pipeline.
///
/// Push constants (fragment stage): 16 bytes — texel_w (f32), texel_h (f32),
/// threshold/blend (f32), _pad (f32).
/// Descriptor set 0: one COMBINED_IMAGE_SAMPLER (bloom_set_layout).
/// No vertex input — fullscreen triangle from gl_VertexIndex.
/// No depth test/write. MSAA TYPE_1.
///
/// Returns `(PipelineLayout, Pipeline)`. Caller owns and must destroy both.
pub fn create_bloom_pipeline(
    device: &ash::Device,
    set_layout: vk::DescriptorSetLayout,
    color_format: vk::Format,
    vert_spv: &[u8],
    frag_spv: &[u8],
) -> Result<(vk::PipelineLayout, vk::Pipeline)> {
    // Push constants: 16 bytes in fragment stage.
    let push_constant_range = vk::PushConstantRange::default()
        .stage_flags(vk::ShaderStageFlags::FRAGMENT)
        .offset(0)
        .size(16);
    let push_constant_ranges = [push_constant_range];

    let set_layouts = [set_layout];
    let layout_info = vk::PipelineLayoutCreateInfo::default()
        .set_layouts(&set_layouts)
        .push_constant_ranges(&push_constant_ranges);
    // SAFETY: device is valid.
    let pipeline_layout = unsafe { device.create_pipeline_layout(&layout_info, None) }
        .context("Failed to create bloom pipeline layout")?;

    let vert_module = create_shader_module(device, vert_spv)?;
    let frag_module = create_shader_module(device, frag_spv)?;

    let entry_point = c"main";
    let shader_stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert_module)
            .name(entry_point),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag_module)
            .name(entry_point),
    ];

    // No vertex input — positions generated from gl_VertexIndex.
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();

    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
        .primitive_restart_enable(false);

    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);

    let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
        .depth_clamp_enable(false)
        .rasterizer_discard_enable(false)
        .polygon_mode(vk::PolygonMode::FILL)
        .line_width(1.0)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .depth_bias_enable(false);

    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .sample_shading_enable(false)
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);

    let color_blend_attachment = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(false);
    let color_blend_attachments = [color_blend_attachment];
    let color_blend = vk::PipelineColorBlendStateCreateInfo::default()
        .logic_op_enable(false)
        .attachments(&color_blend_attachments);

    // No depth test or write — fullscreen post-process.
    let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(false)
        .depth_write_enable(false)
        .depth_compare_op(vk::CompareOp::ALWAYS)
        .depth_bounds_test_enable(false)
        .stencil_test_enable(false);

    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

    // No depth attachment — color-only pass.
    let color_formats = [color_format];
    let mut rendering_info = vk::PipelineRenderingCreateInfo::default()
        .color_attachment_formats(&color_formats);

    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&shader_stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterization)
        .multisample_state(&multisample)
        .color_blend_state(&color_blend)
        .depth_stencil_state(&depth_stencil)
        .dynamic_state(&dynamic_state)
        .layout(pipeline_layout)
        .push_next(&mut rendering_info);

    // SAFETY: device is valid, all referenced state lives on the stack.
    let pipelines = unsafe {
        device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
    }
    .map_err(|(_pipelines, err)| err)
    .context("Failed to create bloom pipeline")?;

    // SAFETY: shader modules baked into pipeline — safe to destroy.
    unsafe {
        device.destroy_shader_module(vert_module, None);
        device.destroy_shader_module(frag_module, None);
    }

    log::info!("Bloom pipeline created (fullscreen triangle, HDR format {:?})", color_format);
    Ok((pipeline_layout, pipelines[0]))
}

/// Composite pipeline: tone-maps HDR + blends bloom → LDR swapchain.
///
/// Push constants (fragment stage): 16 bytes — bloom_intensity (f32), tone_mode (f32),
/// bloom_enabled (f32), _pad (f32).
/// Descriptor set 0: two COMBINED_IMAGE_SAMPLER (composite_set_layout).
/// No vertex input, no depth. Writes to swapchain_format.
///
/// Returns `(PipelineLayout, Pipeline)`. Caller owns and must destroy both.
pub fn create_composite_pipeline(
    device: &ash::Device,
    set_layout: vk::DescriptorSetLayout,
    swapchain_format: vk::Format,
) -> Result<(vk::PipelineLayout, vk::Pipeline)> {
    // Push constants: 16 bytes in fragment stage.
    let push_constant_range = vk::PushConstantRange::default()
        .stage_flags(vk::ShaderStageFlags::FRAGMENT)
        .offset(0)
        .size(16);
    let push_constant_ranges = [push_constant_range];

    let set_layouts = [set_layout];
    let layout_info = vk::PipelineLayoutCreateInfo::default()
        .set_layouts(&set_layouts)
        .push_constant_ranges(&push_constant_ranges);
    // SAFETY: device is valid.
    let pipeline_layout = unsafe { device.create_pipeline_layout(&layout_info, None) }
        .context("Failed to create composite pipeline layout")?;

    let vert_module = create_shader_module(device, FULLSCREEN_VERT_SPV)?;
    let frag_module = create_shader_module(device, COMPOSITE_FRAG_SPV)?;

    let entry_point = c"main";
    let shader_stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert_module)
            .name(entry_point),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag_module)
            .name(entry_point),
    ];

    // No vertex input — positions generated from gl_VertexIndex.
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();

    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
        .primitive_restart_enable(false);

    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);

    let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
        .depth_clamp_enable(false)
        .rasterizer_discard_enable(false)
        .polygon_mode(vk::PolygonMode::FILL)
        .line_width(1.0)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .depth_bias_enable(false);

    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .sample_shading_enable(false)
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);

    let color_blend_attachment = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(false);
    let color_blend_attachments = [color_blend_attachment];
    let color_blend = vk::PipelineColorBlendStateCreateInfo::default()
        .logic_op_enable(false)
        .attachments(&color_blend_attachments);

    // No depth test — fullscreen composite.
    let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(false)
        .depth_write_enable(false)
        .depth_compare_op(vk::CompareOp::ALWAYS)
        .depth_bounds_test_enable(false)
        .stencil_test_enable(false);

    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

    // No depth attachment — writes directly to swapchain.
    let color_formats = [swapchain_format];
    let mut rendering_info = vk::PipelineRenderingCreateInfo::default()
        .color_attachment_formats(&color_formats);

    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&shader_stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterization)
        .multisample_state(&multisample)
        .color_blend_state(&color_blend)
        .depth_stencil_state(&depth_stencil)
        .dynamic_state(&dynamic_state)
        .layout(pipeline_layout)
        .push_next(&mut rendering_info);

    // SAFETY: device is valid, all referenced state lives on the stack.
    let pipelines = unsafe {
        device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
    }
    .map_err(|(_pipelines, err)| err)
    .context("Failed to create composite pipeline")?;

    // SAFETY: shader modules baked into pipeline — safe to destroy.
    unsafe {
        device.destroy_shader_module(vert_module, None);
        device.destroy_shader_module(frag_module, None);
    }

    log::info!("Composite pipeline created (tone-map + bloom blend, {:?})", swapchain_format);
    Ok((pipeline_layout, pipelines[0]))
}

/// Fullscreen skybox pipeline: draws a 3-vertex triangle covering the screen at depth=1.0.
///
/// Push constants:
///   - VERTEX  stage: offset 0,  size 64 — `invViewProj` (mat4)
///   - FRAGMENT stage: offset 64, size 16 — `skyTint` (vec4: rgb tint × a brightness)
/// Descriptor set 0: samplerCube skyboxMap (binding 0).
/// Depth test: LESS_OR_EQUAL, depth write: OFF.
///
/// Returns `(PipelineLayout, Pipeline)`. Caller owns and must destroy both.
pub fn create_skybox_pipeline(
    device: &ash::Device,
    vert_spv: &[u8],
    frag_spv: &[u8],
    swapchain_format: vk::Format,
    depth_format: vk::Format,
    skybox_set_layout: vk::DescriptorSetLayout,
    rasterization_samples: vk::SampleCountFlags,
) -> Result<(vk::PipelineLayout, vk::Pipeline)> {
    // Two push constant ranges: vertex (invViewProj) + fragment (skyTint).
    let push_constant_ranges = [
        vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX)
            .offset(0)
            .size(64),
        vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .offset(64)
            .size(16),
    ];

    let set_layouts = [skybox_set_layout];
    let layout_info = vk::PipelineLayoutCreateInfo::default()
        .set_layouts(&set_layouts)
        .push_constant_ranges(&push_constant_ranges);
    // SAFETY: device is valid.
    let pipeline_layout = unsafe { device.create_pipeline_layout(&layout_info, None) }
        .context("Failed to create skybox pipeline layout")?;

    let pipeline = build_skybox_pipeline(
        device,
        vert_spv,
        frag_spv,
        pipeline_layout,
        swapchain_format,
        depth_format,
        rasterization_samples,
    )?;

    log::info!("Skybox pipeline created (fullscreen triangle, depth LESS_OR_EQUAL, no depth write)");
    Ok((pipeline_layout, pipeline))
}

/// Particle billboard pipeline (Fase 40).
///
/// Push constants (vertex stage): 64 bytes — `view_proj` (mat4).
/// No descriptor sets — vertices carry all per-particle data.
/// Additive blending: src=ONE, dst=ONE for fire/glow particles.
/// Depth test ON, depth write OFF — particles sort behind opaque but not each other.
///
/// Returns `(PipelineLayout, Pipeline)`. Caller owns and must destroy both.
pub fn create_particle_pipeline(
    device: &ash::Device,
    swapchain_format: vk::Format,
    depth_format: vk::Format,
    binding_descriptions: &[vk::VertexInputBindingDescription],
    attribute_descriptions: &[vk::VertexInputAttributeDescription],
) -> Result<(vk::PipelineLayout, vk::Pipeline)> {
    // Push constant: view_proj mat4, 64 bytes, vertex stage.
    let push_constant_range = vk::PushConstantRange::default()
        .stage_flags(vk::ShaderStageFlags::VERTEX)
        .offset(0)
        .size(64);
    let push_constant_ranges = [push_constant_range];

    let layout_info = vk::PipelineLayoutCreateInfo::default()
        .push_constant_ranges(&push_constant_ranges);
    // SAFETY: device is valid.
    let pipeline_layout = unsafe { device.create_pipeline_layout(&layout_info, None) }
        .context("Failed to create particle pipeline layout")?;

    let vert_module = create_shader_module(device, PARTICLE_VERT_SPV)?;
    let frag_module = create_shader_module(device, PARTICLE_FRAG_SPV)?;

    let entry_point = c"main";
    let shader_stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert_module)
            .name(entry_point),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag_module)
            .name(entry_point),
    ];

    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(binding_descriptions)
        .vertex_attribute_descriptions(attribute_descriptions);

    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
        .primitive_restart_enable(false);

    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);

    let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
        .depth_clamp_enable(false)
        .rasterizer_discard_enable(false)
        .polygon_mode(vk::PolygonMode::FILL)
        .line_width(1.0)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .depth_bias_enable(false);

    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .sample_shading_enable(false)
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);

    // Additive blending: outColor = src * 1 + dst * 1
    let color_blend_attachment = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::ONE)
        .dst_color_blend_factor(vk::BlendFactor::ONE)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ONE)
        .alpha_blend_op(vk::BlendOp::ADD);
    let color_blend_attachments = [color_blend_attachment];
    let color_blend = vk::PipelineColorBlendStateCreateInfo::default()
        .logic_op_enable(false)
        .attachments(&color_blend_attachments);

    // Depth test ON (particles occluded by geometry), depth write OFF (particles don't block each other).
    let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(true)
        .depth_write_enable(false)
        .depth_compare_op(vk::CompareOp::LESS)
        .depth_bounds_test_enable(false)
        .stencil_test_enable(false);

    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

    let color_formats = [swapchain_format];
    let mut rendering_info = vk::PipelineRenderingCreateInfo::default()
        .color_attachment_formats(&color_formats)
        .depth_attachment_format(depth_format);

    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&shader_stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterization)
        .multisample_state(&multisample)
        .color_blend_state(&color_blend)
        .depth_stencil_state(&depth_stencil)
        .dynamic_state(&dynamic_state)
        .layout(pipeline_layout)
        .push_next(&mut rendering_info);

    // SAFETY: device is valid, all referenced state lives on the stack.
    let pipelines = unsafe {
        device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
    }
    .map_err(|(_p, e)| e)
    .context("Failed to create particle pipeline")?;

    // SAFETY: shader modules baked into pipeline — safe to destroy.
    unsafe {
        device.destroy_shader_module(vert_module, None);
        device.destroy_shader_module(frag_module, None);
    }

    log::info!("Particle pipeline created (additive blend, depth test ON, depth write OFF)");
    Ok((pipeline_layout, pipelines[0]))
}
