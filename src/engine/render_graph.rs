/// Lightweight per-frame render graph.
///
/// Tracks image layouts across declared passes and automatically inserts
/// `vkCmdPipelineBarrier` calls between them.  No heap allocation of closures —
/// the caller drives execution with `begin_pass` / `end_pass`.
///
/// Usage pattern in `record_command_buffer`:
/// ```
/// let mut graph = RenderGraph::new();
/// let rid = graph.add_resource(image, aspect, initial_layout);
/// graph.add_pass("Shadow", &[(rid, ResourceAccess::shadow_write())]);
/// graph.add_pass("Main",   &[(rid, ResourceAccess::shadow_read())]);
/// graph.compile()?;
///
/// graph.begin_pass(&device, cmd);   // emits barriers
/// /* shadow draw commands */
/// graph.end_pass();
///
/// graph.begin_pass(&device, cmd);   // emits barriers
/// /* main draw commands */
/// graph.end_pass();
/// ```
use anyhow::{bail, Result};
use ash::vk;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Opaque handle for a resource registered with the render graph.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ResourceId(usize);

/// Describes how a single pass accesses one image resource.
///
/// Two barriers may be generated:
/// - **Enter**: current layout → `required_layout`, emitted by `begin_pass`.
/// - **Exit**:  `required_layout` → `final_layout`, emitted by `end_pass`
///   (only when `final_layout != required_layout`).
#[derive(Clone, Copy)]
pub struct ResourceAccess {
    /// Layout the resource must be in when the pass begins drawing.
    pub required_layout: vk::ImageLayout,
    /// Layout the resource is in when the pass finishes drawing.
    /// Set equal to `required_layout` when the pass does not change the layout.
    pub final_layout: vk::ImageLayout,

    // ---- Enter barrier (current_layout → required_layout) ----------------
    pub enter_src_stage: vk::PipelineStageFlags,
    pub enter_dst_stage: vk::PipelineStageFlags,
    pub enter_src_access: vk::AccessFlags,
    pub enter_dst_access: vk::AccessFlags,

    // ---- Exit barrier (required_layout → final_layout) -------------------
    // Used only when final_layout != required_layout.
    pub exit_src_stage: vk::PipelineStageFlags,
    pub exit_dst_stage: vk::PipelineStageFlags,
    pub exit_src_access: vk::AccessFlags,
    pub exit_dst_access: vk::AccessFlags,
}

// ---------------------------------------------------------------------------
// Preset constructors — cover all transitions used in this engine
// ---------------------------------------------------------------------------

impl ResourceAccess {
    /// A color image transitioning from UNDEFINED to COLOR_ATTACHMENT_OPTIMAL.
    /// Used for the swapchain and MSAA color image at frame start.
    pub fn color_init() -> Self {
        Self {
            required_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            final_layout:    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            enter_src_stage:  vk::PipelineStageFlags::TOP_OF_PIPE,
            enter_dst_stage:  vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            enter_src_access: vk::AccessFlags::empty(),
            enter_dst_access: vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            // No exit barrier needed (layout stays the same).
            exit_src_stage:  vk::PipelineStageFlags::empty(),
            exit_dst_stage:  vk::PipelineStageFlags::empty(),
            exit_src_access: vk::AccessFlags::empty(),
            exit_dst_access: vk::AccessFlags::empty(),
        }
    }

    /// A depth image transitioning from UNDEFINED to DEPTH_STENCIL_ATTACHMENT_OPTIMAL.
    /// Used for the MSAA depth image at frame start.
    pub fn depth_init() -> Self {
        Self {
            required_layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
            final_layout:    vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
            enter_src_stage:  vk::PipelineStageFlags::TOP_OF_PIPE,
            enter_dst_stage:  vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
            enter_src_access: vk::AccessFlags::empty(),
            enter_dst_access: vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
            exit_src_stage:  vk::PipelineStageFlags::empty(),
            exit_dst_stage:  vk::PipelineStageFlags::empty(),
            exit_src_access: vk::AccessFlags::empty(),
            exit_dst_access: vk::AccessFlags::empty(),
        }
    }

    /// Shadow map written as depth attachment, then transitioned to SHADER_READ_ONLY
    /// automatically after the pass (for sampling in the main pass).
    pub fn shadow_write() -> Self {
        Self {
            required_layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
            final_layout:    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            enter_src_stage:  vk::PipelineStageFlags::FRAGMENT_SHADER,
            enter_dst_stage:  vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
            enter_src_access: vk::AccessFlags::SHADER_READ,
            enter_dst_access: vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
            // Exit: transition back to SHADER_READ_ONLY so the main pass can sample it.
            exit_src_stage:  vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
            exit_dst_stage:  vk::PipelineStageFlags::FRAGMENT_SHADER,
            exit_src_access: vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
            exit_dst_access: vk::AccessFlags::SHADER_READ,
        }
    }

    /// Resource sampled in the fragment shader, stays in SHADER_READ_ONLY.
    pub fn shader_read() -> Self {
        Self {
            required_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            final_layout:    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            enter_src_stage:  vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
            enter_dst_stage:  vk::PipelineStageFlags::FRAGMENT_SHADER,
            enter_src_access: vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
            enter_dst_access: vk::AccessFlags::SHADER_READ,
            exit_src_stage:  vk::PipelineStageFlags::empty(),
            exit_dst_stage:  vk::PipelineStageFlags::empty(),
            exit_src_access: vk::AccessFlags::empty(),
            exit_dst_access: vk::AccessFlags::empty(),
        }
    }

    /// A color image used as COLOR_ATTACHMENT with LOAD_OP_LOAD (stays COLOR_ATTACHMENT).
    /// Used for wireframe, egui, and any pass that composites onto an existing image.
    pub fn color_attachment() -> Self {
        Self {
            required_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            final_layout:    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            enter_src_stage:  vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            enter_dst_stage:  vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            enter_src_access: vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            enter_dst_access: vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            exit_src_stage:  vk::PipelineStageFlags::empty(),
            exit_dst_stage:  vk::PipelineStageFlags::empty(),
            exit_src_access: vk::AccessFlags::empty(),
            exit_dst_access: vk::AccessFlags::empty(),
        }
    }

    /// Write to a color attachment, then transition to SHADER_READ_ONLY after the pass.
    /// Used for: hdr_color in Main+Skybox, bloom levels during downsample.
    /// Enter from UNDEFINED (discard) or COLOR_ATTACHMENT_OPTIMAL.
    pub fn color_attachment_to_read() -> Self {
        Self {
            required_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            final_layout:    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            enter_src_stage:  vk::PipelineStageFlags::TOP_OF_PIPE,
            enter_dst_stage:  vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            enter_src_access: vk::AccessFlags::empty(),
            enter_dst_access: vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            exit_src_stage:  vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            exit_dst_stage:  vk::PipelineStageFlags::FRAGMENT_SHADER,
            exit_src_access: vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            exit_dst_access: vk::AccessFlags::SHADER_READ,
        }
    }

    /// Overwrite a bloom level that is currently SHADER_READ_ONLY.
    /// Used for: bloom levels during upsample (they were SHADER_READ_ONLY after downsample).
    pub fn bloom_overwrite() -> Self {
        Self {
            required_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            final_layout:    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            enter_src_stage:  vk::PipelineStageFlags::FRAGMENT_SHADER,
            enter_dst_stage:  vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            enter_src_access: vk::AccessFlags::SHADER_READ,
            enter_dst_access: vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            exit_src_stage:  vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            exit_dst_stage:  vk::PipelineStageFlags::FRAGMENT_SHADER,
            exit_src_access: vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            exit_dst_access: vk::AccessFlags::SHADER_READ,
        }
    }

    /// Sample a color image that is already in SHADER_READ_ONLY. No barrier emitted
    /// when the image is already in that layout.
    pub fn color_shader_read() -> Self {
        Self {
            required_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            final_layout:    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            enter_src_stage:  vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            enter_dst_stage:  vk::PipelineStageFlags::FRAGMENT_SHADER,
            enter_src_access: vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            enter_dst_access: vk::AccessFlags::SHADER_READ,
            exit_src_stage:  vk::PipelineStageFlags::empty(),
            exit_dst_stage:  vk::PipelineStageFlags::empty(),
            exit_src_access: vk::AccessFlags::empty(),
            exit_dst_access: vk::AccessFlags::empty(),
        }
    }

    /// Swapchain image transitioning from COLOR_ATTACHMENT to PRESENT_SRC.
    /// Used for the final present pseudo-pass at frame end.
    pub fn present() -> Self {
        Self {
            required_layout: vk::ImageLayout::PRESENT_SRC_KHR,
            final_layout:    vk::ImageLayout::PRESENT_SRC_KHR,
            enter_src_stage:  vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            enter_dst_stage:  vk::PipelineStageFlags::BOTTOM_OF_PIPE,
            enter_src_access: vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            enter_dst_access: vk::AccessFlags::empty(),
            exit_src_stage:  vk::PipelineStageFlags::empty(),
            exit_dst_stage:  vk::PipelineStageFlags::empty(),
            exit_src_access: vk::AccessFlags::empty(),
            exit_dst_access: vk::AccessFlags::empty(),
        }
    }
}

// ---------------------------------------------------------------------------
// Internal state
// ---------------------------------------------------------------------------

struct TrackedImage {
    raw: vk::Image,
    aspect: vk::ImageAspectFlags,
    layout: vk::ImageLayout,
}

struct PassNode {
    name: &'static str,
    /// (resource index, access descriptor)
    accesses: Vec<(usize, ResourceAccess)>,
}

// ---------------------------------------------------------------------------
// RenderGraph
// ---------------------------------------------------------------------------

pub struct RenderGraph {
    images: Vec<TrackedImage>,
    passes: Vec<PassNode>,
    /// Index of the pass that will be executed on the next begin_pass() call.
    cursor: usize,
}

impl RenderGraph {
    pub fn new() -> Self {
        Self { images: Vec::new(), passes: Vec::new(), cursor: 0 }
    }

    // -----------------------------------------------------------------------
    // Resource registration
    // -----------------------------------------------------------------------

    /// Register an image.  Returns a `ResourceId` for use in `add_pass`.
    /// `initial_layout` is the layout this image is in before any pass runs.
    pub fn add_resource(
        &mut self,
        raw: vk::Image,
        aspect: vk::ImageAspectFlags,
        initial_layout: vk::ImageLayout,
    ) -> ResourceId {
        let id = ResourceId(self.images.len());
        self.images.push(TrackedImage { raw, aspect, layout: initial_layout });
        id
    }

    // -----------------------------------------------------------------------
    // Pass declaration
    // -----------------------------------------------------------------------

    /// Declare a render pass.  Passes execute in declaration order.
    /// `accesses` lists every resource the pass reads or writes, with the
    /// corresponding `ResourceAccess` describing the expected layouts and
    /// barrier parameters.
    pub fn add_pass(&mut self, name: &'static str, accesses: &[(ResourceId, ResourceAccess)]) {
        let resolved: Vec<(usize, ResourceAccess)> = accesses
            .iter()
            .map(|(id, acc)| {
                debug_assert!(id.0 < self.images.len(), "ResourceId out of range for pass '{name}'");
                (id.0, *acc)
            })
            .collect();
        self.passes.push(PassNode { name, accesses: resolved });
    }

    // -----------------------------------------------------------------------
    // Compile — validate the graph
    // -----------------------------------------------------------------------

    /// Validate the graph.  Simulates layout evolution and checks that every
    /// resource required in `SHADER_READ_ONLY_OPTIMAL` was either initialised
    /// in that layout or written (with `final_layout = SHADER_READ_ONLY`) by a
    /// prior pass.
    ///
    /// Returns an error if a pass would sample a resource that nobody produces.
    pub fn compile(&self) -> Result<()> {
        // Simulate layout evolution.
        let mut layouts: Vec<vk::ImageLayout> =
            self.images.iter().map(|img| img.layout).collect();

        for pass in &self.passes {
            for (idx, access) in &pass.accesses {
                if *idx >= layouts.len() {
                    bail!(
                        "Pass '{}': ResourceId({idx}) is out of range (only {} resources registered)",
                        pass.name,
                        layouts.len()
                    );
                }

                // Detect reads of resources that have never been produced.
                if access.required_layout == vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
                    && layouts[*idx] == vk::ImageLayout::UNDEFINED
                {
                    bail!(
                        "Pass '{}': resource {idx} requires SHADER_READ_ONLY_OPTIMAL \
                         but it is still UNDEFINED — no prior pass produces it",
                        pass.name
                    );
                }

                // Advance simulated layout.
                layouts[*idx] = access.final_layout;
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Execution — begin_pass / end_pass
    // -----------------------------------------------------------------------

    /// Emit barriers required before the current pass, then advance the cursor.
    /// Call this immediately before recording draw commands for the pass.
    ///
    /// # Panics
    /// Panics if called more times than there are declared passes.
    pub fn begin_pass(&mut self, device: &ash::Device, cmd: vk::CommandBuffer) {
        let pass = &self.passes[self.cursor];

        let mut barriers: Vec<vk::ImageMemoryBarrier> = Vec::new();
        let mut src_stage = vk::PipelineStageFlags::empty();
        let mut dst_stage = vk::PipelineStageFlags::empty();

        for (idx, access) in &pass.accesses {
            let image = &self.images[*idx];
            if image.layout == access.required_layout {
                // Already in the right layout — no barrier needed.
                continue;
            }

            barriers.push(
                vk::ImageMemoryBarrier::default()
                    .src_access_mask(access.enter_src_access)
                    .dst_access_mask(access.enter_dst_access)
                    .old_layout(image.layout)
                    .new_layout(access.required_layout)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .image(image.raw)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: image.aspect,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    }),
            );
            src_stage |= access.enter_src_stage;
            dst_stage |= access.enter_dst_stage;
        }

        if !barriers.is_empty() {
            // SAFETY: cmd is recording; image barriers reference valid VkImage handles.
            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    src_stage,
                    dst_stage,
                    vk::DependencyFlags::empty(),
                    &[], &[], &barriers,
                );
            }
        }
    }

    /// Emit any exit barriers for the current pass, update tracked layouts,
    /// then advance to the next pass.  Call this after recording draw commands.
    pub fn end_pass(&mut self, device: &ash::Device, cmd: vk::CommandBuffer) {
        let pass = &self.passes[self.cursor];

        let mut barriers: Vec<vk::ImageMemoryBarrier> = Vec::new();
        let mut src_stage = vk::PipelineStageFlags::empty();
        let mut dst_stage = vk::PipelineStageFlags::empty();

        for (idx, access) in &pass.accesses {
            if access.required_layout != access.final_layout {
                let image = &self.images[*idx];
                barriers.push(
                    vk::ImageMemoryBarrier::default()
                        .src_access_mask(access.exit_src_access)
                        .dst_access_mask(access.exit_dst_access)
                        .old_layout(access.required_layout)
                        .new_layout(access.final_layout)
                        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .image(image.raw)
                        .subresource_range(vk::ImageSubresourceRange {
                            aspect_mask: image.aspect,
                            base_mip_level: 0,
                            level_count: 1,
                            base_array_layer: 0,
                            layer_count: 1,
                        }),
                );
                src_stage |= access.exit_src_stage;
                dst_stage |= access.exit_dst_stage;
            }
        }

        if !barriers.is_empty() {
            // SAFETY: cmd is recording; image barriers reference valid VkImage handles.
            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    src_stage,
                    dst_stage,
                    vk::DependencyFlags::empty(),
                    &[], &[], &barriers,
                );
            }
        }

        // Update tracked layouts to final state.
        for (idx, access) in &pass.accesses {
            self.images[*idx].layout = access.final_layout;
        }

        self.cursor += 1;
    }

    /// Name of the pass currently at the cursor (for debug assertions).
    pub fn current_pass_name(&self) -> &'static str {
        self.passes.get(self.cursor).map(|p| p.name).unwrap_or("<done>")
    }
}
