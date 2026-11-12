use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

const INTERVAL: Duration = Duration::from_secs(2);

pub struct Stats {
    system: System,
    pid: Option<Pid>,
    started: Instant,
    last: Option<Instant>,
    cpu: f32,
    memory: u64,
}

impl Stats {
    pub fn new() -> Self {
        Self {
            system: System::new(),
            pid: sysinfo::get_current_pid().ok(),
            started: Instant::now(),
            last: None,
            cpu: 0.0,
            memory: 0,
        }
    }

    pub fn refresh(&mut self) {
        if self.last.is_some_and(|at| at.elapsed() < INTERVAL) {
            return;
        }
        let Some(pid) = self.pid else {
            return;
        };
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
        if let Some(process) = self.system.process(pid) {
            self.cpu = process.cpu_usage();
            self.memory = process.memory();
        }
        self.last = Some(Instant::now());
    }

    pub fn badge(&self) -> String {
        if self.last.is_none() {
            return "cpu ... · mem ...".to_string();
        }
        format!("cpu {:.0}% · {}", self.cpu, format_memory(self.memory))
    }

    pub fn reading(&self) -> crate::shell::domain::process_usage::ProcessUsage {
        crate::shell::domain::process_usage::ProcessUsage {
            cpu: self.cpu,
            memory: self.memory,
            uptime: self.started.elapsed(),
        }
    }
}

pub fn format_memory(bytes: u64) -> String {
    let mb = bytes as f64 / (1024.0 * 1024.0);
    if mb >= 1024.0 {
        format!("{:.1} GB", mb / 1024.0)
    } else {
        format!("{mb:.0} MB")
    }
}

pub fn format_uptime(elapsed: Duration) -> String {
    let secs = elapsed.as_secs();
    let (hours, minutes, seconds) = (secs / 3600, (secs / 60) % 60, secs % 60);
    if hours > 0 {
        format!("{hours}h {minutes:02}m")
    } else if minutes > 0 {
        format!("{minutes}m {seconds:02}s")
    } else {
        format!("{seconds}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(format_memory(41 * 1024 * 1024), "41 MB");
        assert_eq!(format_memory(3 * 1024 * 1024 * 1024 / 2), "1.5 GB");
        assert_eq!(format_uptime(Duration::from_secs(5)), "5s");
        assert_eq!(format_uptime(Duration::from_secs(125)), "2m 05s");
        assert_eq!(format_uptime(Duration::from_secs(3700)), "1h 01m");
    }

    #[test]
    fn measures_this_process() {
        let mut stats = Stats::new();
        assert!(stats.badge().contains("..."));
        stats.refresh();
        assert!(stats.memory > 0);
        assert!(stats.badge().contains("MB"));
    }
}
