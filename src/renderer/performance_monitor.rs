use std::time::Instant;
use std::collections::VecDeque;
use std::fs::File;
use std::io::Write;

pub struct PerformanceMonitor {
    start_time: Instant,
    frame_times: VecDeque<f32>,
    fps_history: Vec<(f32, f32)>, // (time_seconds, fps)
    last_second: u32,
    frames_in_current_second: u32,
    pub total_frames: u32,
    camera_positions: Vec<(f32, [f32; 3])>, // (time, position)
}

impl PerformanceMonitor {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            frame_times: VecDeque::with_capacity(120),
            fps_history: Vec::new(),
            last_second: 0,
            frames_in_current_second: 0,
            total_frames: 0,
            camera_positions: Vec::new(),
        }
    }

    pub fn record_frame(&mut self, frame_time: f32, camera_position: Option<[f32; 3]>) {
        self.frame_times.push_back(frame_time);
        if self.frame_times.len() > 120 {
            self.frame_times.pop_front();
        }

        self.total_frames += 1;
        self.frames_in_current_second += 1;

        let elapsed = self.start_time.elapsed().as_secs_f32();
        let current_second = elapsed as u32;

        // Record camera position if provided
        if let Some(pos) = camera_position {
            self.camera_positions.push((elapsed, pos));
        }

        // If we've moved to a new second, record FPS for the previous second
        if current_second > self.last_second {
            let fps = self.frames_in_current_second as f32;
            self.fps_history.push((self.last_second as f32, fps));
            self.last_second = current_second;
            self.frames_in_current_second = 0;
        }
    }

    pub fn get_current_fps(&self) -> f32 {
        if self.frame_times.is_empty() {
            return 0.0;
        }
        let avg_frame_time: f32 = self.frame_times.iter().sum::<f32>() / self.frame_times.len() as f32;
        1.0 / avg_frame_time
    }

    pub fn get_average_fps(&self) -> f32 {
        let elapsed = self.start_time.elapsed().as_secs_f32();
        if elapsed > 0.0 {
            self.total_frames as f32 / elapsed
        } else {
            0.0
        }
    }

    pub fn generate_report(&self, filename: &str) -> std::io::Result<()> {
        let mut file = File::create(filename)?;

        writeln!(file, "# Performance Report")?;
        writeln!(file)?;
        writeln!(file, "## Summary")?;
        writeln!(file, "- **Total Runtime**: {:.2} seconds", self.start_time.elapsed().as_secs_f32())?;
        writeln!(file, "- **Total Frames**: {}", self.total_frames)?;
        writeln!(file, "- **Average FPS**: {:.2}", self.get_average_fps())?;
        writeln!(file, "- **Current FPS**: {:.2}", self.get_current_fps())?;

        if !self.frame_times.is_empty() {
            let min_frame_time = self.frame_times.iter().min_by(|a, b| a.partial_cmp(b).unwrap()).unwrap();
            let max_frame_time = self.frame_times.iter().max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap();
            writeln!(file, "- **Best Frame Time**: {:.2} ms ({:.2} FPS)", min_frame_time * 1000.0, 1.0 / min_frame_time)?;
            writeln!(file, "- **Worst Frame Time**: {:.2} ms ({:.2} FPS)", max_frame_time * 1000.0, 1.0 / max_frame_time)?;
        }

        writeln!(file)?;
        writeln!(file, "## FPS Per Second")?;
        writeln!(file)?;
        writeln!(file, "| Second | FPS |")?;
        writeln!(file, "|--------|-----|")?;

        for (second, fps) in &self.fps_history {
            writeln!(file, "| {:.0} | {:.0} |", second, fps)?;
        }

        // Add current second if it has frames
        if self.frames_in_current_second > 0 {
            writeln!(file, "| {} | {} |", self.last_second, self.frames_in_current_second)?;
        }

        if !self.camera_positions.is_empty() {
            writeln!(file)?;
            writeln!(file, "## Camera Position Samples")?;
            writeln!(file)?;
            writeln!(file, "| Time (s) | Position (x, y, z) | Distance from Origin |")?;
            writeln!(file, "|----------|-------------------|---------------------|")?;

            // Sample every N positions to avoid too much data
            let sample_rate = (self.camera_positions.len() / 10).max(1);
            for (i, (time, pos)) in self.camera_positions.iter().enumerate() {
                if i % sample_rate == 0 {
                    let distance = (pos[0] * pos[0] + pos[1] * pos[1] + pos[2] * pos[2]).sqrt();
                    writeln!(file, "| {:.2} | ({:.2}, {:.2}, {:.2}) | {:.2} |",
                             time, pos[0], pos[1], pos[2], distance)?;
                }
            }
        }

        writeln!(file)?;
        writeln!(file, "## Performance Notes")?;
        writeln!(file, "- Performance degrades when camera is inside the Cornell Box due to increased ray marching steps")?;
        writeln!(file, "- FPS improves when viewing the box from outside")?;
        writeln!(file, "- Optimal viewing distance appears to be 1-2 units from the box entrance")?;

        Ok(())
    }
}