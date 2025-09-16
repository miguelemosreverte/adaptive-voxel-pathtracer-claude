use std::collections::VecDeque;

pub struct PerformanceController {
    target_framerate: f32,
    current_voxel_size: f32,
    frame_time_history: VecDeque<f32>,
    adjustment_rate: f32,
    history_size: usize,
    last_adjustment_direction: i8,  // -1 for decrease, 0 for none, 1 for increase
    stable_frames: u32,  // Count frames at stable performance
}

impl PerformanceController {
    pub fn new(target_framerate: f32) -> Self {
        Self {
            target_framerate,
            current_voxel_size: 0.02,  // Start with high performance for 60 FPS target
            frame_time_history: VecDeque::with_capacity(10),  // Smaller window for faster response
            adjustment_rate: 0.1,
            history_size: 10,  // Smaller history for quicker reaction
            last_adjustment_direction: 0,
            stable_frames: 0,
        }
    }

    pub fn update(&mut self, frame_time: f32) -> Option<f32> {
        self.frame_time_history.push_back(frame_time);

        if self.frame_time_history.len() > self.history_size {
            self.frame_time_history.pop_front();
        }

        // Need a few frames to make a decision
        if self.frame_time_history.len() < 3 {
            return None;
        }

        let current_fps = 1.0 / frame_time;
        let avg_frame_time = self.average_frame_time();
        let avg_fps = 1.0 / avg_frame_time;

        // CRITICAL: If current FPS drops below 60, react IMMEDIATELY
        if current_fps < 58.0 {
            // Emergency increase - big jump to get back above 60 FPS
            let panic_multiplier = 60.0 / current_fps.max(10.0);  // How much we need to improve
            self.current_voxel_size = (self.current_voxel_size * panic_multiplier.min(2.0)).min(0.05);

            log::info!("⚠️ EMERGENCY: FPS {:.1} < 60! Step size -> {:.4}", current_fps, self.current_voxel_size);
            self.last_adjustment_direction = 1;
            self.stable_frames = 0;
            return Some(self.current_voxel_size);
        }

        // Check for oscillation - if we just adjusted in opposite direction, dampen
        let mut adjustment_factor = 1.0;
        if self.stable_frames < 10 {
            adjustment_factor = 0.5;  // Smaller adjustments when unstable
        }

        if avg_fps < 60.0 {
            // Below target: increase step size for better performance
            let scale = 1.0 + (0.3 * adjustment_factor);  // Less aggressive when dampened

            // Prevent oscillation
            if self.last_adjustment_direction == -1 {
                // We just decreased, now increasing - use smaller step
                self.current_voxel_size = (self.current_voxel_size * (1.0 + 0.1 * adjustment_factor)).min(0.05);
            } else {
                self.current_voxel_size = (self.current_voxel_size * scale).min(0.05);
            }

            log::debug!("Performance low: FPS {:.1} -> step size {:.4}", avg_fps, self.current_voxel_size);
            self.last_adjustment_direction = 1;
            self.stable_frames = 0;
            Some(self.current_voxel_size)

        } else if avg_fps > 70.0 && self.stable_frames > 15 {
            // Only improve quality if we've been stable for a while
            let scale = 1.0 - (0.1 * adjustment_factor);
            self.current_voxel_size = (self.current_voxel_size * scale).max(0.005);

            log::debug!("Performance good: FPS {:.1} -> step size {:.4}", avg_fps, self.current_voxel_size);
            self.last_adjustment_direction = -1;
            self.stable_frames = 0;
            Some(self.current_voxel_size)

        } else {
            // In the sweet spot (60-70 FPS)
            self.stable_frames += 1;
            None
        }
    }

    fn average_frame_time(&self) -> f32 {
        if self.frame_time_history.is_empty() {
            return 0.016;
        }

        let sum: f32 = self.frame_time_history.iter().sum();
        sum / self.frame_time_history.len() as f32
    }

    pub fn get_current_voxel_size(&self) -> f32 {
        self.current_voxel_size
    }
}