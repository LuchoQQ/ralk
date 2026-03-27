/// GPU performance profiler using Vulkan timestamp and pipeline statistics queries.
///
/// Design:
/// - Double-buffered query pools: frame N writes to pools[N%2], reads pools[(N-1)%2].
///   Since we wait_for_fences(N%2) before recording, the pools slot is always safe to read.
/// - Timestamps: 2 writes per pass (TOP_OF_PIPE begin, BOTTOM_OF_PIPE end).
/// - Pipeline stats: 1 beginQuery/endQuery pair encompassing all draw passes.
/// - If timestamps not supported (timestampValidBits == 0), profiler is a no-op.
use anyhow::Result;
use ash::vk;

pub(super) const MAX_FRAMES: usize = 2;
const MAX_TS_QUERIES: u32 = 64; // up to 32 passes × 2 timestamps

/// Per-pass GPU time.
#[derive(Clone, Default)]
pub struct PassTiming {
    pub name: String,
    pub ms: f32,
}

/// Aggregated pipeline statistics for a full frame.
#[derive(Clone, Default)]
pub struct PipelineStats {
    /// Total vertex shader invocations across all draw calls.
    pub vertex_invocations: u64,
    /// Total fragments that passed rasterization (pre-depth test).
    pub fragment_invocations: u64,
    /// Primitives that passed clipping.
    pub clipping_primitives: u64,
}

/// Manages timestamp and pipeline-statistics query pools.
pub struct GpuProfiler {
    /// Non-zero → GPU supports timestamps on this queue family.
    pub supports_timestamps: bool,
    /// Nanoseconds per timestamp tick (`VkPhysicalDeviceLimits::timestampPeriod`).
    pub timestamp_period_ns: f32,
    /// Whether `pipelineStatisticsQuery` was enabled at device creation.
    pub supports_pipeline_stats: bool,
    /// Number of valid bits in a timestamp value (informational).
    pub timestamp_valid_bits: u32,

    /// Double-buffered timestamp query pools. None if timestamps not supported.
    ts_pools: Option<[vk::QueryPool; MAX_FRAMES]>,
    /// Per-frame: next free query slot index.
    write_idx: [u32; MAX_FRAMES],
    /// Per-frame: pass names recorded (correlates to timestamp pairs in the pool).
    names: [Vec<&'static str>; MAX_FRAMES],

    /// Double-buffered pipeline stats pools. None if feature not available.
    stat_pools: Option<[vk::QueryPool; MAX_FRAMES]>,
    /// Whether the stats query is currently active (beginQuery called, endQuery not yet).
    stat_active: [bool; MAX_FRAMES],

    // --- Published results (from the last completed frame) ---
    /// Per-pass timings, ordered as they were recorded.
    pub results: Vec<PassTiming>,
    /// Sum of all pass timings (total GPU frame time).
    pub total_ms: f32,
    /// Pipeline statistics from the last completed frame.
    pub pipeline_stats: PipelineStats,
}

impl GpuProfiler {
    /// Create pools and detect capabilities.
    ///
    /// `pipeline_stats_enabled` must be `true` only if the logical device was created
    /// with `VkPhysicalDeviceFeatures::pipelineStatisticsQuery = VK_TRUE`.
    pub fn new(
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: &ash::Instance,
        queue_family_index: u32,
        pipeline_stats_enabled: bool,
    ) -> Result<Self> {
        // ------------------------------------------------------------------ //
        // Query hardware capabilities
        // ------------------------------------------------------------------ //
        let queue_props = unsafe {
            instance.get_physical_device_queue_family_properties(physical_device)
        };
        let timestamp_valid_bits = queue_props
            .get(queue_family_index as usize)
            .map(|p| p.timestamp_valid_bits)
            .unwrap_or(0);
        let supports_timestamps = timestamp_valid_bits > 0;

        let dev_props =
            unsafe { instance.get_physical_device_properties(physical_device) };
        let timestamp_period_ns = dev_props.limits.timestamp_period;

        log::info!(
            "GpuProfiler: timestamps={supports_timestamps} \
             valid_bits={timestamp_valid_bits} period={timestamp_period_ns:.2}ns \
             pipeline_stats={pipeline_stats_enabled}"
        );

        // ------------------------------------------------------------------ //
        // Timestamp pools
        // ------------------------------------------------------------------ //
        let ts_pools = if supports_timestamps {
            let ci = vk::QueryPoolCreateInfo::default()
                .query_type(vk::QueryType::TIMESTAMP)
                .query_count(MAX_TS_QUERIES);
            // SAFETY: device is valid; create_info is well-formed.
            Some([
                unsafe { device.create_query_pool(&ci, None)? },
                unsafe { device.create_query_pool(&ci, None)? },
            ])
        } else {
            None
        };

        // ------------------------------------------------------------------ //
        // Pipeline statistics pools
        // ------------------------------------------------------------------ //
        // We capture three statistics: vertex invocations, clipping primitives,
        // fragment invocations.  These are stored in flag-bit order in the result.
        let stat_pools = if pipeline_stats_enabled {
            let stat_flags = vk::QueryPipelineStatisticFlags::VERTEX_SHADER_INVOCATIONS
                | vk::QueryPipelineStatisticFlags::CLIPPING_PRIMITIVES
                | vk::QueryPipelineStatisticFlags::FRAGMENT_SHADER_INVOCATIONS;
            let ci = vk::QueryPoolCreateInfo::default()
                .query_type(vk::QueryType::PIPELINE_STATISTICS)
                .query_count(1)
                .pipeline_statistics(stat_flags);
            // SAFETY: device is valid; create_info is well-formed.
            Some([
                unsafe { device.create_query_pool(&ci, None)? },
                unsafe { device.create_query_pool(&ci, None)? },
            ])
        } else {
            None
        };

        Ok(Self {
            supports_timestamps,
            timestamp_period_ns,
            supports_pipeline_stats: pipeline_stats_enabled,
            timestamp_valid_bits,
            ts_pools,
            write_idx: [0; MAX_FRAMES],
            names: [Vec::new(), Vec::new()],
            stat_pools,
            stat_active: [false; MAX_FRAMES],
            results: Vec::new(),
            total_ms: 0.0,
            pipeline_stats: PipelineStats::default(),
        })
    }

    // ---------------------------------------------------------------------- //
    // Per-frame recording API
    // ---------------------------------------------------------------------- //

    /// Reset query pools for this frame slot. Call at the top of record_command_buffer,
    /// before any graph passes.
    pub fn begin_frame(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame_idx: usize,
    ) {
        self.write_idx[frame_idx] = 0;
        self.names[frame_idx].clear();
        self.stat_active[frame_idx] = false;

        if let Some(pools) = &self.ts_pools {
            // SAFETY: cmd is recording; pool is valid.
            unsafe {
                device.cmd_reset_query_pool(cmd, pools[frame_idx], 0, MAX_TS_QUERIES);
            }
        }
        if let Some(pools) = &self.stat_pools {
            // SAFETY: cmd is recording; pool is valid.
            unsafe {
                device.cmd_reset_query_pool(cmd, pools[frame_idx], 0, 1);
            }
        }
    }

    /// Write a BEGIN timestamp (TOP_OF_PIPE).
    /// Call after `graph.begin_pass()` and before the first draw command.
    pub fn begin_pass(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame_idx: usize,
        name: &'static str,
    ) {
        let Some(pools) = &self.ts_pools else { return };
        let idx = self.write_idx[frame_idx];
        if idx + 1 >= MAX_TS_QUERIES {
            return; // overflow guard — should never happen with MAX_TS_QUERIES = 64
        }
        // SAFETY: cmd is recording, pool is valid, idx is in-range.
        unsafe {
            device.cmd_write_timestamp(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                pools[frame_idx],
                idx,
            );
        }
        self.names[frame_idx].push(name);
        self.write_idx[frame_idx] = idx + 1;
    }

    /// Write an END timestamp (BOTTOM_OF_PIPE).
    /// Call after the last draw command and before `graph.end_pass()`.
    pub fn end_pass(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame_idx: usize,
    ) {
        let Some(pools) = &self.ts_pools else { return };
        let idx = self.write_idx[frame_idx];
        if idx >= MAX_TS_QUERIES {
            return;
        }
        // SAFETY: cmd is recording, pool is valid, idx is in-range.
        unsafe {
            device.cmd_write_timestamp(
                cmd,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                pools[frame_idx],
                idx,
            );
        }
        self.write_idx[frame_idx] = idx + 1;
    }

    /// Begin pipeline statistics query. Call before the first draw commands in the frame.
    pub fn begin_stats(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame_idx: usize,
    ) {
        let Some(pools) = &self.stat_pools else { return };
        // SAFETY: cmd recording, pool valid.
        unsafe {
            device.cmd_begin_query(
                cmd,
                pools[frame_idx],
                0,
                vk::QueryControlFlags::empty(),
            );
        }
        self.stat_active[frame_idx] = true;
    }

    /// End pipeline statistics query. Call after the last draw commands in the frame.
    pub fn end_stats(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame_idx: usize,
    ) {
        if !self.stat_active[frame_idx] {
            return;
        }
        let Some(pools) = &self.stat_pools else { return };
        // SAFETY: cmd recording, pool valid.
        unsafe {
            device.cmd_end_query(cmd, pools[frame_idx], 0);
        }
        self.stat_active[frame_idx] = false;
    }

    // ---------------------------------------------------------------------- //
    // Result readback (CPU)
    // ---------------------------------------------------------------------- //

    /// Read results from `frame_idx` (which must have finished on the GPU — called
    /// after `vkWaitForFences`).  Updates `self.results`, `self.total_ms`, and
    /// `self.pipeline_stats`.
    pub fn read_results(&mut self, device: &ash::Device, frame_idx: usize) {
        // ------------------------------------------------------------------ //
        // Timestamps
        // ------------------------------------------------------------------ //
        let pass_count = self.names[frame_idx].len();
        if let Some(pools) = &self.ts_pools {
            if pass_count > 0 {
                // data.len() = pass_count * 2 queries; ash derives query_count from data.len()
                // and stride from size_of::<u64>() = 8. Each timestamp is one u64.
                let mut timestamps = vec![0u64; pass_count * 2];

                // SAFETY: pool is valid; fence was waited on → results are ready.
                let ok = unsafe {
                    device.get_query_pool_results(
                        pools[frame_idx],
                        0,
                        &mut timestamps,
                        vk::QueryResultFlags::TYPE_64 | vk::QueryResultFlags::WAIT,
                    )
                };

                if ok.is_ok() {
                    self.results.clear();
                    self.total_ms = 0.0;
                    for (i, &name) in self.names[frame_idx].iter().enumerate() {
                        let begin_ts = timestamps[i * 2];
                        let end_ts = timestamps[i * 2 + 1];
                        if end_ts > begin_ts {
                            let ns = (end_ts - begin_ts) as f32 * self.timestamp_period_ns;
                            let ms = ns / 1_000_000.0;
                            self.results.push(PassTiming { name: name.to_string(), ms });
                            self.total_ms += ms;
                        }
                    }
                }
            }
        }

        // ------------------------------------------------------------------ //
        // Pipeline statistics
        // ------------------------------------------------------------------ //
        if let Some(pools) = &self.stat_pools {
            // 1 query with 3 statistics, each 8 bytes: use [u64; 3] as element type.
            // ash derives: query_count = data.len() = 1, stride = size_of::<[u64;3]>() = 24.
            // Bit order in result: VERTEX_INVOCATIONS (bit2), CLIPPING_PRIMITIVES (bit6),
            //                     FRAGMENT_INVOCATIONS (bit7).
            let mut stats = [[0u64; 3]; 1];
            // SAFETY: pool valid, fence waited.
            let ok = unsafe {
                device.get_query_pool_results(
                    pools[frame_idx],
                    0,
                    &mut stats,
                    vk::QueryResultFlags::TYPE_64 | vk::QueryResultFlags::WAIT,
                )
            };
            if ok.is_ok() {
                self.pipeline_stats = PipelineStats {
                    vertex_invocations:   stats[0][0],
                    clipping_primitives:  stats[0][1],
                    fragment_invocations: stats[0][2],
                };
            }
        }
    }

    // ---------------------------------------------------------------------- //
    // Cleanup
    // ---------------------------------------------------------------------- //

    /// Destroy all Vulkan query pools.  Must be called before the device is destroyed.
    pub fn destroy(&self, device: &ash::Device) {
        // SAFETY: device is valid, pools are valid.
        unsafe {
            if let Some(pools) = &self.ts_pools {
                for &pool in pools {
                    device.destroy_query_pool(pool, None);
                }
            }
            if let Some(pools) = &self.stat_pools {
                for &pool in pools {
                    device.destroy_query_pool(pool, None);
                }
            }
        }
    }
}
