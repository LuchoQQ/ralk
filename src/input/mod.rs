use glam::Vec2;

/// Per-frame input state. Keys are boolean (held/not). Mouse delta resets each frame.
pub struct InputState {
    pub forward: bool,
    pub backward: bool,
    pub left: bool,
    pub right: bool,
    pub sprint: bool,
    /// Emergency brake (Space — vehicles only, not used when on foot).
    pub brake: bool,
    /// Jump request — set on key-press, consumed each physics tick.
    pub jump: bool,
    /// Raw mouse delta accumulated since last `clear_frame_deltas()`.
    pub mouse_delta: Vec2,
    /// True when egui is consuming pointer/keyboard events — camera should not respond.
    pub ui_captured: bool,
    /// Left stick (X = strafe, Y = forward/back). Range [-1, 1], dead-zone applied.
    pub gamepad_move: Vec2,
    /// Right stick (X = yaw, Y = pitch). Range [-1, 1], dead-zone applied.
    pub gamepad_look: Vec2,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            forward: false,
            backward: false,
            left: false,
            right: false,
            sprint: false,
            brake: false,
            jump: false,
            mouse_delta: Vec2::ZERO,
            ui_captured: false,
            gamepad_move: Vec2::ZERO,
            gamepad_look: Vec2::ZERO,
        }
    }

    /// Called by the UI layer to block camera input when egui is active.
    pub fn set_captured(&mut self, captured: bool) {
        self.ui_captured = captured;
    }

    /// Reset per-frame accumulators. Call once at the start of each frame.
    pub fn clear_frame_deltas(&mut self) {
        self.mouse_delta = Vec2::ZERO;
    }
}
