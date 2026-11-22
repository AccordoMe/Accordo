// SPDX-License-Identifier: GPL-3.0-or-later
//! What this app uses: memory, CPU, threads, disk, the data folder — shown in App settings →
//! About, sampled once a second only while that tab is open. Only our own process is read.

use std::collections::VecDeque;
use std::path::Path;
use std::time::{Duration, Instant};

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

/// Points kept for the little charts (one a second → the last minute).
pub const HISTORY: usize = 60;
/// The data folder is walked at most this often (it barely changes).
const FOLDER_EVERY: Duration = Duration::from_secs(10);
/// Stop walking a folder after this many entries (never hang on a huge tree).
const FOLDER_MAX_ENTRIES: usize = 20_000;

#[derive(Debug, Clone, Default)]
pub struct Sample {
    /// Resident memory (what the system counts against the app), bytes.
    pub memory: u64,
    /// The most seen since the tab was first opened.
    pub peak: u64,
    /// RAM in the computer, bytes.
    pub total_memory: u64,
    /// CPU over the last second, as a share of the whole machine (0–100).
    pub cpu: f32,
    pub cores: usize,
    pub threads: Option<usize>,
    pub uptime_s: u64,
    /// Read from / written to disk since the app started, bytes.
    pub read: u64,
    pub written: u64,
    /// The app data folder on disk (None until first measured).
    pub data_bytes: Option<u64>,
    /// Pictures decoded in memory: (count, frames, bytes).
    pub pictures: (usize, usize, u64),
}

#[derive(Default)]
pub struct Usage {
    sys: Option<System>,
    pid: Option<Pid>,
    peak: u64,
    data: Option<(u64, Instant)>,
    pub now: Option<Sample>,
    /// Memory, bytes — oldest first.
    pub memory_history: VecDeque<u64>,
    /// CPU, % of the machine — oldest first.
    pub cpu_history: VecDeque<f32>,
}

fn push<T>(q: &mut VecDeque<T>, v: T) {
    q.push_back(v);
    while q.len() > HISTORY {
        q.pop_front();
    }
}

fn folder_size(root: &Path) -> u64 {
    let (mut total, mut seen) = (0u64, 0usize);
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            seen += 1;
            if seen > FOLDER_MAX_ENTRIES {
                return total;
            }
            // symlink_metadata: links aren't followed (no loops, no counting what isn't ours).
            let Ok(meta) = e.path().symlink_metadata() else { continue };
            if meta.is_dir() {
                stack.push(e.path());
            } else if meta.is_file() {
                total += meta.len();
            }
        }
    }
    total
}

impl Usage {
    /// Takes a fresh sample. CPU needs two samples a second apart, so the first one reads 0.
    pub fn sample(&mut self, pictures: (usize, usize, u64)) {
        if self.sys.is_none() {
            self.sys = System::new().ok();
            self.pid = sysinfo::get_current_pid().ok();
        }
        let (Some(sys), Some(pid)) = (self.sys.as_mut(), self.pid) else { return };
        sys.refresh_memory();
        sys.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            true,
            ProcessRefreshKind::nothing().with_memory().with_cpu().with_disk_usage().with_tasks(),
        );
        let Some(p) = sys.process(pid) else { return };
        let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
        let memory = p.memory();
        self.peak = self.peak.max(memory);
        let disk = p.disk_usage();
        if self.data.is_none_or(|(_, at)| at.elapsed() >= FOLDER_EVERY) {
            if let Some(dir) = crate::api::client::config_dir() {
                self.data = Some((folder_size(&dir), Instant::now()));
            }
        }
        let cpu = (p.cpu_usage() / cores as f32).clamp(0.0, 100.0);
        let s = Sample {
            memory,
            peak: self.peak,
            total_memory: sys.total_memory(),
            cpu,
            cores,
            threads: p.tasks().map(|t| t.len()).filter(|n| *n > 0),
            uptime_s: p.run_time(),
            read: disk.total_read_bytes,
            written: disk.total_written_bytes,
            data_bytes: self.data.map(|(b, _)| b),
            pictures,
        };
        push(&mut self.memory_history, memory);
        push(&mut self.cpu_history, cpu);
        self.now = Some(s);
    }
}

/// 1536 → "1.5 KB" (decimal-free under 10 for small values, like system monitors).
pub fn bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{n} B") } else if v < 10.0 { format!("{v:.1} {}", UNITS[i]) } else { format!("{v:.0} {}", UNITS[i]) }
}

/// 3725 → "1 h 2 min".
pub fn duration(s: u64) -> String {
    let (d, h, m) = (s / 86_400, s / 3600 % 24, s / 60 % 60);
    if d > 0 {
        format!("{d} d {h} h")
    } else if h > 0 {
        format!("{h} h {m} min")
    } else if m > 0 {
        format!("{m} min")
    } else {
        format!("{s} s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(bytes(512), "512 B");
        assert_eq!(bytes(1536), "1.5 KB");
        assert_eq!(bytes(250 * 1024 * 1024), "250 MB");
        assert_eq!(duration(42), "42 s");
        assert_eq!(duration(3725), "1 h 2 min");
        assert_eq!(duration(90_000), "1 d 1 h");
    }

    #[test]
    fn samples_own_process() {
        let mut u = Usage::default();
        u.sample((0, 0, 0));
        let s = u.now.expect("a sample");
        assert!(s.memory > 0 && s.peak >= s.memory && s.cores >= 1);
        assert_eq!(u.memory_history.len(), 1);
    }
}
