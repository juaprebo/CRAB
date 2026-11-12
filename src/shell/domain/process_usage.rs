use std::time::Duration;

pub struct ProcessUsage {
    pub cpu: f32,
    pub memory: u64,
    pub uptime: Duration,
}
