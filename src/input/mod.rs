use glam::Vec2;

/// Per-frame input state. Keys are boolean (held/not). Mouse delta resets each frame.
pub struct InputState {
    pub forward: bool,
    pub backward: bool,
    pub left: bool,
    pub right: bool,
    pub sprint: bool,
    /// Raw mouse delta accumulated since last `clear_frame_deltas()`.
    pub mouse_delta: Vec2,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            forward: false,
            backward: false,
            left: false,
            right: false,
            sprint: false,
            mouse_delta: Vec2::ZERO,
        }
    }

    /// Reset per-frame accumulators. Call once at the start of each frame.
    pub fn clear_frame_deltas(&mut self) {
        self.mouse_delta = Vec2::ZERO;
    }
}
