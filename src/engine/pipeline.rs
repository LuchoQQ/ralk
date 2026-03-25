use anyhow::{Context, Result};
use ash::vk;

const VERT_SPV: &[u8] = include_bytes!("../../shaders/triangle.vert.spv");
const FRAG_SPV: &[u8] = include_bytes!("../../shaders/triangle.frag.spv");
const SHADOW_VERT_SPV: &[u8] = include_bytes!("../../shaders/shadow.vert.spv");
const SHADOW_FRAG_SPV: &[u8] = include_bytes!("../../shaders/shadow.frag.spv");

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

/// Set 0: lighting UBO (binding 0) + shadow map sampler (binding 1), fragment stage.
pub fn create_lighting_descriptor_set_layout(
    device: &ash::Device,
) -> Result<vk::DescriptorSetLayout> {
    let bindings = [
        vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        vk::DescriptorSetLayoutBinding::default()
            .binding(1)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
    ];
    let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
    // SAFETY: device is valid, binding info is well-formed.
    unsafe { device.create_descriptor_set_layout(&info, None) }
        .context("Failed to create lighting descriptor set layout")
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

/// Create the graphics pipeline for the mesh PBR pass.
///
/// Push constants (vertex stage):
///   - bytes   0..64: MVP matrix (mat4)
///   - bytes  64..128: model matrix (mat4)
///
/// Returns `(PipelineLayout, Pipeline)`. Caller owns and must destroy both.
pub fn create_graphics_pipeline(
    device: &ash::Device,
    swapchain_format: vk::Format,
    depth_format: vk::Format,
    binding_descriptions: &[vk::VertexInputBindingDescription],
    attribute_descriptions: &[vk::VertexInputAttributeDescription],
    lighting_set_layout: vk::DescriptorSetLayout,   // set 0
    material_set_layout: vk::DescriptorSetLayout,   // set 1
) -> Result<(vk::PipelineLayout, vk::Pipeline)> {
    let vert_module = create_shader_module(device, VERT_SPV)?;
    let frag_module = create_shader_module(device, FRAG_SPV)?;

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

    let color_blend_attachment = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(false);
    let color_blend_attachments = [color_blend_attachment];
    let color_blend = vk::PipelineColorBlendStateCreateInfo::default()
        .logic_op_enable(false)
        .attachments(&color_blend_attachments);

    // Depth test: LESS, write enabled. No stencil.
    let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(true)
        .depth_write_enable(true)
        .depth_compare_op(vk::CompareOp::LESS)
        .depth_bounds_test_enable(false)
        .stencil_test_enable(false);

    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

    // Push constants: 128 bytes in vertex stage — MVP (0..64) + model (64..128).
    let push_constant_range = vk::PushConstantRange::default()
        .stage_flags(vk::ShaderStageFlags::VERTEX)
        .offset(0)
        .size(128);
    let push_constant_ranges = [push_constant_range];

    // Set 0: lighting UBO — Set 1: material textures (albedo, normal, metallic-roughness)
    let set_layouts = [lighting_set_layout, material_set_layout];
    let layout_info = vk::PipelineLayoutCreateInfo::default()
        .set_layouts(&set_layouts)
        .push_constant_ranges(&push_constant_ranges);
    // SAFETY: device is valid.
    let pipeline_layout = unsafe { device.create_pipeline_layout(&layout_info, None) }
        .context("Failed to create pipeline layout")?;

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

    let pipeline_infos = [pipeline_info];

    // SAFETY: device is valid, all referenced state lives on the stack.
    let pipelines = unsafe {
        device.create_graphics_pipelines(vk::PipelineCache::null(), &pipeline_infos, None)
    }
    .map_err(|(_pipelines, err)| err)
    .context("Failed to create graphics pipeline")?;

    // Shader modules are baked into the pipeline — safe to destroy now.
    // SAFETY: device is valid, modules are no longer referenced.
    unsafe {
        device.destroy_shader_module(vert_module, None);
        device.destroy_shader_module(frag_module, None);
    }

    log::info!("Graphics pipeline created (Cook-Torrance PBR, push constants 128 bytes)");
    Ok((pipeline_layout, pipelines[0]))
}

/// Depth-only pipeline for the shadow map pass.
///
/// Push constants (vertex stage): 64 bytes — `light_view_proj * model` (mat4).
/// No descriptor sets — everything goes through push constants.
///
/// Returns `(PipelineLayout, Pipeline)`. Caller owns and must destroy both.
pub fn create_shadow_pipeline(
    device: &ash::Device,
    depth_format: vk::Format,
    binding_descriptions: &[vk::VertexInputBindingDescription],
    attribute_descriptions: &[vk::VertexInputAttributeDescription],
) -> Result<(vk::PipelineLayout, vk::Pipeline)> {
    let vert_module = create_shader_module(device, SHADOW_VERT_SPV)?;
    let frag_module = create_shader_module(device, SHADOW_FRAG_SPV)?;

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

    // Depth bias reduces shadow acne (self-shadowing). NONE cull avoids light leaking
    // on thin geometry. Adjust bias values if artifacts appear on specific models.
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
    let color_blend = vk::PipelineColorBlendStateCreateInfo::default()
        .logic_op_enable(false);

    let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(true)
        .depth_write_enable(true)
        .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL)
        .depth_bounds_test_enable(false)
        .stencil_test_enable(false);

    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

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

    // Depth-only rendering — no color attachment formats.
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

    let pipeline_infos = [pipeline_info];

    // SAFETY: device is valid, all referenced state lives on the stack.
    let pipelines = unsafe {
        device.create_graphics_pipelines(vk::PipelineCache::null(), &pipeline_infos, None)
    }
    .map_err(|(_pipelines, err)| err)
    .context("Failed to create shadow pipeline")?;

    // SAFETY: device is valid, modules are no longer referenced.
    unsafe {
        device.destroy_shader_module(vert_module, None);
        device.destroy_shader_module(frag_module, None);
    }

    log::info!("Shadow pipeline created (depth-only, 2048×2048)");
    Ok((pipeline_layout, pipelines[0]))
}
