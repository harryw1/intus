use std::collections::HashSet;
use std::sync::{Arc, Mutex};

/// Thread-safe tracker for child processes spawned by the application.
#[derive(Debug, Clone, Default)]
pub struct ProcessTracker {
    pids: Arc<Mutex<HashSet<u32>>>,
}

impl ProcessTracker {
    pub fn new() -> Self {
        Self {
            pids: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Register a PID to be tracked.
    pub fn add_pid(&self, pid: u32) {
        let mut pids = self.pids.lock().unwrap();
        pids.insert(pid);
    }

    /// Remove a PID from tracking (e.g., when process completes naturally).
    pub fn remove_pid(&self, pid: u32) {
        let mut pids = self.pids.lock().unwrap();
        pids.remove(&pid);
    }

    /// Kill all tracked processes.
    pub fn kill_all(&self) {
        let pids = self.pids.lock().unwrap();
        if pids.is_empty() {
             return;
        }

        for &pid in pids.iter() {
            
            #[cfg(unix)]
            {
               // Use command line kill for simplicity and no heavy deps
               let _ = std::process::Command::new("kill")
                    .arg(pid.to_string())
                    .output();
            }
            #[cfg(windows)]
            {
                let _ = std::process::Command::new("taskkill")
                    .arg("/F")
                    .arg("/PID")
                    .arg(pid.to_string())
                    .output();
            }
        }
    }
}
