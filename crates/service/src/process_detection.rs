//! Process Detection Module
//!
//! Implements process monitoring for auto profile switching (GI-02)
//! Provides ≤500ms response time for game detection and profile switching

use anyhow::Result;
// Serde traits removed to avoid serialization issues with Instant
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::interval;
use tracing::{debug, info, warn};

#[cfg(target_os = "windows")]
use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
#[cfg(target_os = "windows")]
use winapi::um::tlhelp32::{
    CreateToolhelp32Snapshot, PROCESSENTRY32, Process32First, Process32Next, TH32CS_SNAPPROCESS,
};
#[cfg(target_os = "windows")]
use winapi::um::winnt::HANDLE;

#[cfg(target_os = "linux")]
use std::fs;

/// Process detection service for auto profile switching
pub struct ProcessDetectionService {
    /// Channel for sending process events
    event_sender: mpsc::UnboundedSender<ProcessEvent>,
    /// Currently running processes
    running_processes: HashMap<String, ProcessInfo>,
    /// Game process patterns to monitor
    game_patterns: HashMap<String, Vec<String>>,
    /// Detection interval (default 100ms for ≤500ms response)
    detection_interval: Duration,
}

/// Information about a detected process
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub game_id: Option<String>,
    pub detected_at: Instant,
}

/// Process detection events
#[derive(Debug, Clone)]
pub enum ProcessEvent {
    GameStarted {
        game_id: String,
        process_info: ProcessInfo,
    },
    GameStopped {
        game_id: String,
        process_info: ProcessInfo,
    },
    ProcessListUpdated {
        processes: Vec<ProcessInfo>,
    },
}

impl ProcessDetectionService {
    /// Create new process detection service
    pub fn new() -> (Self, mpsc::UnboundedReceiver<ProcessEvent>) {
        let (event_sender, event_receiver) = mpsc::unbounded_channel();

        let service = Self {
            event_sender,
            running_processes: HashMap::new(),
            game_patterns: HashMap::new(),
            detection_interval: Duration::from_millis(100), // 100ms for ≤500ms response
        };

        (service, event_receiver)
    }

    /// Add game process patterns to monitor
    pub fn add_game_patterns(&mut self, game_id: String, patterns: Vec<String>) {
        info!(game_id = %game_id, patterns = ?patterns, "Added game process patterns");
        self.game_patterns.insert(game_id, patterns);
    }

    /// Start process monitoring
    pub async fn start_monitoring(&mut self) -> Result<()> {
        info!("Starting process detection monitoring");

        let mut interval = interval(self.detection_interval);

        loop {
            interval.tick().await;

            match self.scan_processes().await {
                Ok(current_processes) => {
                    self.update_process_state(current_processes).await;
                }
                Err(e) => {
                    warn!(error = %e, "Failed to scan processes");
                }
            }
        }
    }

    /// Scan for currently running processes
    async fn scan_processes(&self) -> Result<Vec<ProcessInfo>> {
        #[cfg(target_os = "windows")]
        {
            self.scan_processes_windows().await
        }

        #[cfg(target_os = "linux")]
        {
            self.scan_processes_linux().await
        }

        #[cfg(not(any(target_os = "windows", target_os = "linux")))]
        {
            // Fallback for unsupported platforms
            Ok(Vec::new())
        }
    }

    #[cfg(target_os = "windows")]
    async fn scan_processes_windows(&self) -> Result<Vec<ProcessInfo>> {
        let mut processes = Vec::new();
        let snapshot = ProcessSnapshot::create()?;
        let mut entry = process_entry_buffer();

        if first_process(snapshot.as_raw(), &mut entry) {
            loop {
                let process_name = process_entry_name(&entry);

                let game_id = self.match_process_to_game(&process_name);

                processes.push(ProcessInfo {
                    pid: entry.th32ProcessID,
                    name: process_name,
                    game_id,
                    detected_at: Instant::now(),
                });

                if !next_process(snapshot.as_raw(), &mut entry) {
                    break;
                }
            }
        }

        Ok(processes)
    }

    #[cfg(target_os = "linux")]
    async fn scan_processes_linux(&self) -> Result<Vec<ProcessInfo>> {
        let mut processes = Vec::new();

        let proc_dir = fs::read_dir("/proc")?;

        for entry in proc_dir {
            let entry = entry?;
            let path = entry.path();

            if let Some(pid_str) = path.file_name().and_then(|n| n.to_str())
                && let Ok(pid) = pid_str.parse::<u32>()
            {
                let comm_path = path.join("comm");
                if let Ok(process_name) = fs::read_to_string(comm_path) {
                    let process_name = process_name.trim().to_string();
                    let game_id = self.match_process_to_game(&process_name);

                    processes.push(ProcessInfo {
                        pid,
                        name: process_name,
                        game_id,
                        detected_at: Instant::now(),
                    });
                }
            }
        }

        Ok(processes)
    }

    /// Match process name to game ID using patterns
    fn match_process_to_game(&self, process_name: &str) -> Option<String> {
        for (game_id, patterns) in &self.game_patterns {
            for pattern in patterns {
                if process_name
                    .to_lowercase()
                    .contains(&pattern.to_lowercase())
                {
                    return Some(game_id.to_string());
                }
            }
        }
        None
    }

    /// Update process state and send events
    async fn update_process_state(&mut self, current_processes: Vec<ProcessInfo>) {
        let mut new_running = HashMap::new();
        let mut game_processes = HashMap::new();

        // Build new state and detect game processes
        for process in current_processes {
            let key = format!("{}:{}", process.name, process.pid);

            if let Some(game_id) = &process.game_id {
                game_processes.insert(game_id.clone(), process.clone());
            }

            new_running.insert(key, process);
        }

        // Detect newly started games
        for (game_id, process_info) in &game_processes {
            if !self.is_game_currently_running(game_id) {
                info!(game_id = %game_id, process = %process_info.name, pid = process_info.pid, "Game started");

                let _ = self.event_sender.send(ProcessEvent::GameStarted {
                    game_id: game_id.clone(),
                    process_info: process_info.clone(),
                });
            }
        }

        // Detect stopped games
        let currently_running_games: Vec<String> = self
            .running_processes
            .values()
            .filter_map(|p| p.game_id.as_ref().map(|s| s.to_string()))
            .collect();

        for game_id in currently_running_games {
            if !game_processes.contains_key(&game_id)
                && let Some(process_info) = self.get_game_process_info(&game_id)
            {
                info!(game_id = %game_id, process = %process_info.name, "Game stopped");

                let _ = self.event_sender.send(ProcessEvent::GameStopped {
                    game_id,
                    process_info,
                });
            }
        }

        // Update running processes
        self.running_processes = new_running;

        // Send process list update
        let all_processes: Vec<ProcessInfo> = self.running_processes.values().cloned().collect();
        let _ = self.event_sender.send(ProcessEvent::ProcessListUpdated {
            processes: all_processes,
        });

        debug!(
            process_count = self.running_processes.len(),
            "Updated process state"
        );
    }

    /// Check if a game is currently running
    fn is_game_currently_running(&self, game_id: &str) -> bool {
        self.running_processes
            .values()
            .any(|p| p.game_id.as_deref() == Some(game_id))
    }

    /// Get process info for a running game
    fn get_game_process_info(&self, game_id: &str) -> Option<ProcessInfo> {
        self.running_processes
            .values()
            .find(|p| p.game_id.as_deref() == Some(game_id))
            .cloned()
    }

    /// Get currently running games
    pub fn get_running_games(&self) -> Vec<String> {
        self.running_processes
            .values()
            .filter_map(|p| p.game_id.as_ref().map(|s| s.to_string()))
            .collect()
    }

    /// Set detection interval
    pub fn set_detection_interval(&mut self, interval: Duration) {
        self.detection_interval = interval;
        info!(
            interval_ms = interval.as_millis(),
            "Updated detection interval"
        );
    }
}

impl Default for ProcessDetectionService {
    fn default() -> Self {
        Self::new().0
    }
}

#[cfg(target_os = "windows")]
struct ProcessSnapshot {
    handle: HANDLE,
}

#[cfg(target_os = "windows")]
impl ProcessSnapshot {
    fn create() -> Result<Self> {
        // SAFETY: `CreateToolhelp32Snapshot` does not dereference caller-owned
        // pointers for this use; it receives a constant process-snapshot flag
        // and process id 0 to enumerate all processes. The returned handle is
        // checked against `INVALID_HANDLE_VALUE` before use and closed by
        // `Drop`.
        let handle = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if handle == INVALID_HANDLE_VALUE {
            Err(anyhow::anyhow!("Failed to create process snapshot"))
        } else {
            Ok(Self { handle })
        }
    }

    fn as_raw(&self) -> HANDLE {
        self.handle
    }
}

#[cfg(target_os = "windows")]
impl Drop for ProcessSnapshot {
    fn drop(&mut self) {
        // SAFETY: `handle` was returned by `CreateToolhelp32Snapshot` and
        // validated as not `INVALID_HANDLE_VALUE`. `Drop` is the sole owner of
        // this wrapper, so the snapshot handle is closed at most once.
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

#[cfg(target_os = "windows")]
fn process_entry_buffer() -> PROCESSENTRY32 {
    // SAFETY: `PROCESSENTRY32` is a C POD buffer whose all-zero state is a valid
    // initialization baseline for the Windows enumeration APIs. `dwSize` is set
    // immediately after zero-initialization as required by `Process32First` and
    // `Process32Next`.
    let mut entry: PROCESSENTRY32 = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32>() as u32;
    entry
}

#[cfg(target_os = "windows")]
fn first_process(snapshot: HANDLE, entry: &mut PROCESSENTRY32) -> bool {
    // SAFETY: `snapshot` comes from `ProcessSnapshot::create`, and `entry` was
    // initialized by `process_entry_buffer` with the correct `dwSize` before
    // being passed to the Windows API.
    unsafe { Process32First(snapshot, entry) != 0 }
}

#[cfg(target_os = "windows")]
fn next_process(snapshot: HANDLE, entry: &mut PROCESSENTRY32) -> bool {
    // SAFETY: `snapshot` remains owned by `ProcessSnapshot`, and `entry` remains
    // a valid mutable `PROCESSENTRY32` buffer across the enumeration loop.
    unsafe { Process32Next(snapshot, entry) != 0 }
}

#[cfg(target_os = "windows")]
fn process_entry_name(entry: &PROCESSENTRY32) -> String {
    let nul = match entry.szExeFile.iter().position(|ch| *ch == 0) {
        Some(idx) => idx,
        None => entry.szExeFile.len(),
    };
    let bytes: Vec<u8> = entry.szExeFile[..nul].iter().map(|ch| *ch as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::{Result, anyhow};
    use tokio::time::timeout;

    #[tokio::test]
    async fn test_process_detection_service_creation() {
        let (service, _receiver) = ProcessDetectionService::new();
        assert_eq!(service.detection_interval, Duration::from_millis(100));
        assert!(service.game_patterns.is_empty());
        assert!(service.running_processes.is_empty());
    }

    #[tokio::test]
    async fn test_add_game_patterns() {
        let (mut service, _receiver) = ProcessDetectionService::new();

        service.add_game_patterns(
            "iracing".to_string(),
            vec!["iRacingSim64DX11.exe".to_string()],
        );

        assert_eq!(service.game_patterns.len(), 1);
        assert!(service.game_patterns.contains_key("iracing"));
    }

    #[test]
    fn test_match_process_to_game() {
        let (mut service, _receiver) = ProcessDetectionService::new();

        service.add_game_patterns(
            "iracing".to_string(),
            vec!["iRacingSim64DX11.exe".to_string()],
        );

        service.add_game_patterns(
            "acc".to_string(),
            vec!["AC2-Win64-Shipping.exe".to_string()],
        );

        assert_eq!(
            service.match_process_to_game("iRacingSim64DX11.exe"),
            Some("iracing".to_string())
        );

        assert_eq!(
            service.match_process_to_game("AC2-Win64-Shipping.exe"),
            Some("acc".to_string())
        );

        assert_eq!(service.match_process_to_game("notepad.exe"), None);
    }

    #[tokio::test]
    async fn test_process_event_handling() -> Result<()> {
        let (mut service, mut receiver) = ProcessDetectionService::new();

        service.add_game_patterns("test_game".to_string(), vec!["test.exe".to_string()]);

        // Simulate process detection
        let test_process = ProcessInfo {
            pid: 1234,
            name: "test.exe".to_string(),
            game_id: Some("test_game".to_string()),
            detected_at: Instant::now(),
        };

        service
            .update_process_state(vec![test_process.clone()])
            .await;

        // Should receive a game started event
        let event = timeout(Duration::from_millis(100), receiver.recv())
            .await
            .map_err(|_| anyhow!("Should receive event"))?
            .ok_or_else(|| anyhow!("Should have event"))?;

        assert!(matches!(event, ProcessEvent::GameStarted { .. }));
        if let ProcessEvent::GameStarted { game_id, .. } = event {
            assert_eq!(game_id, "test_game");
        }

        Ok(())
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn process_entry_name_stops_at_nul_terminator() {
        let mut entry = process_entry_buffer();
        let name = b"example.exe";
        for (idx, byte) in name.iter().copied().enumerate() {
            entry.szExeFile[idx] = byte as i8;
        }

        assert_eq!(process_entry_name(&entry), "example.exe");
    }
}
