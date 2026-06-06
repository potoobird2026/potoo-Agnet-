use super::super::config::PidConfig;
use super::super::services::PidService;
use super::super::types::PidPhase;

const COLD_START_THRESHOLD: usize = 50;

pub struct PidController {
    config: PidConfig,
    integral: f64,
    prev_error: f64,
    update_count: usize,
}

impl PidController {
    pub fn new(config: PidConfig) -> Self {
        Self {
            config,
            integral: 0.0,
            prev_error: 0.0,
            update_count: 0,
        }
    }
}

impl PidService for PidController {
    fn update(&mut self, error: f64, dt: f64) -> f64 {
        self.update_count += 1;
        self.integral += error * dt;
        let derivative = if dt > 0.0 {
            (error - self.prev_error) / dt
        } else {
            0.0
        };
        self.prev_error = error;
        let output =
            self.config.kp * error + self.config.ki * self.integral + self.config.kd * derivative;
        if output.abs() < self.config.deadband {
            0.0
        } else {
            output.clamp(-1.0, 1.0)
        }
    }
    fn phase(&self) -> PidPhase {
        if self.update_count < COLD_START_THRESHOLD {
            PidPhase::ColdStart
        } else {
            PidPhase::Normal
        }
    }
    fn reset(&mut self) {
        self.integral = 0.0;
        self.prev_error = 0.0;
        self.update_count = 0;
    }
}
