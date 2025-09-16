use std::collections::VecDeque;

pub struct PerformanceController {
    target_framerate: f32,
    current_voxel_size: f32,
    frame_time_history: VecDeque<f32>,
    adjustment_rate: f32,
    history_size: usize,
}

impl PerformanceController {
    pub fn new(target_framerate: f32) -> Self {
        Self {
            target_framerate,
            current_voxel_size: 1.0,
            frame_time_history: VecDeque::with_capacity(30),
            adjustment_rate: 0.05,
            history_size: 30,
        }
    }

    pub fn update(&mut self, frame_time: f32) -> Option<f32> {
        self.frame_time_history.push_back(frame_time);

        if self.frame_time_history.len() > self.history_size {
            self.frame_time_history.pop_front();
        }

        if self.frame_time_history.len() < 10 {
            return None;
        }

        let avg_frame_time = self.average_frame_time();
        let target_frame_time = 1.0 / self.target_framerate;

        if avg_frame_time > target_frame_time * 1.1 {
            // Too slow: increase voxel size (reduce quality)
            self.current_voxel_size = (self.current_voxel_size * 1.1).min(10.0);
            Some(self.current_voxel_size)
        } else if avg_frame_time < target_frame_time * 0.8 {
            // Fast enough: decrease voxel size (increase quality)
            self.current_voxel_size = (self.current_voxel_size * 0.95).max(0.1);
            Some(self.current_voxel_size)
        } else {
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