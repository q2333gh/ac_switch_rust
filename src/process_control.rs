//! Windows Steam process discovery, shutdown, kill, and restart helpers.

use crate::error::Result;
use anyhow::{bail, Context};
use std::collections::HashSet;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};
use sysinfo::{ProcessExt, Signal, System, SystemExt};

pub trait ProcessController {
    fn is_running(&mut self, process_names: &[&str]) -> Result<bool>;
    fn shutdown_steam(&mut self, steam_exe: &Path) -> Result<()>;
    fn wait_for_exit(&mut self, process_names: &[&str], timeout: Duration) -> Result<bool>;
    fn force_kill(&mut self, process_names: &[&str]) -> Result<()>;
    fn launch_steam(&mut self, steam_exe: &Path) -> Result<()>;
}

pub struct SystemProcessController {
    system: System,
}

impl SystemProcessController {
    pub fn new() -> Self {
        Self {
            system: System::new_all(),
        }
    }

    fn refresh(&mut self) {
        self.system.refresh_processes();
    }

    fn any_running(&mut self, process_names: &[&str]) -> bool {
        self.refresh();
        let process_names = lower_names(process_names);
        self.system
            .processes()
            .values()
            .any(|process| process_names.contains(&process.name().to_ascii_lowercase()))
    }
}

impl Default for SystemProcessController {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessController for SystemProcessController {
    fn is_running(&mut self, process_names: &[&str]) -> Result<bool> {
        Ok(self.any_running(process_names))
    }

    fn shutdown_steam(&mut self, steam_exe: &Path) -> Result<()> {
        Command::new(steam_exe)
            .arg("-shutdown")
            .spawn()
            .with_context(|| format!("Failed to start {} -shutdown", steam_exe.display()))?;
        Ok(())
    }

    fn wait_for_exit(&mut self, process_names: &[&str], timeout: Duration) -> Result<bool> {
        let deadline = Instant::now() + timeout;
        while Instant::now() <= deadline {
            if !self.any_running(process_names) {
                return Ok(true);
            }
            thread::sleep(Duration::from_millis(200));
        }
        Ok(!self.any_running(process_names))
    }

    fn force_kill(&mut self, process_names: &[&str]) -> Result<()> {
        self.refresh();
        let process_names = lower_names(process_names);
        for process in self.system.processes().values() {
            let process_name = process.name().to_ascii_lowercase();
            if process_names.contains(&process_name) {
                if let Some(false) = process.kill_with(Signal::Kill) {
                    if !process.kill() {
                        bail!("Failed to terminate process {}", process.name());
                    }
                }
            }
        }
        Ok(())
    }

    fn launch_steam(&mut self, steam_exe: &Path) -> Result<()> {
        Command::new(steam_exe)
            .spawn()
            .with_context(|| format!("Failed to start {}", steam_exe.display()))?;
        Ok(())
    }
}

fn lower_names(process_names: &[&str]) -> HashSet<String> {
    process_names
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect()
}
