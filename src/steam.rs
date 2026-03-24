//! Domain model and path resolution rules for local Steam account switching.
//!
//! This module keeps the first-principles rules that are independent from the CLI transport:
//! account identity, sort order, timestamp formatting, and registry override precedence.

use crate::error::Result;
use crate::windows_registry::RegistryStore;
use anyhow::Context;
use chrono::{Local, TimeZone};
use serde::Serialize;
use std::cmp::Ordering;
use std::path::PathBuf;

#[derive(Clone, Debug, Default)]
pub struct PathOverrides {
    pub steam_dir: Option<PathBuf>,
    pub steam_exe: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct ResolvedSteamPaths {
    pub steam_dir: PathBuf,
    pub steam_exe: PathBuf,
    pub loginusers_vdf: PathBuf,
}

#[derive(Clone, Debug)]
pub struct ResolvedLaunchPaths {
    pub steam_exe: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartMode {
    Express,
    Offline,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AccountSelector {
    AccountName(String),
    SteamId64(u64),
}

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct SteamAccount {
    pub steam_id64: u64,
    pub account_name: String,
    pub persona_name: String,
    pub remember_password: bool,
    pub most_recent: bool,
    pub wants_offline_mode: bool,
    pub last_login_timestamp: i64,
    pub last_login_time: String,
    pub is_auto_login_user: bool,
}

impl SteamAccount {
    pub fn matches(&self, selector: &AccountSelector) -> bool {
        match selector {
            AccountSelector::AccountName(value) => self.account_name == *value,
            AccountSelector::SteamId64(value) => self.steam_id64 == *value,
        }
    }
}

pub fn resolve_steam_paths<R: RegistryStore>(
    overrides: &PathOverrides,
    registry: &R,
) -> Result<ResolvedSteamPaths> {
    let steam_dir = overrides
        .steam_dir
        .clone()
        .or(registry.read_steam_path()?)
        .context("SteamPath is missing. Pass --steam-dir or ensure HKCU\\Software\\Valve\\Steam\\SteamPath exists.")?;
    let steam_exe = overrides
        .steam_exe
        .clone()
        .or(registry.read_steam_exe()?)
        .context("SteamExe is missing. Pass --steam-exe or ensure HKCU\\Software\\Valve\\Steam\\SteamExe exists.")?;

    Ok(ResolvedSteamPaths {
        loginusers_vdf: steam_dir.join("config").join("loginusers.vdf"),
        steam_dir,
        steam_exe,
    })
}

pub fn resolve_launch_paths<R: RegistryStore>(
    overrides: &PathOverrides,
    registry: &R,
) -> Result<ResolvedLaunchPaths> {
    let steam_exe = overrides
        .steam_exe
        .clone()
        .or(registry.read_steam_exe()?)
        .context("SteamExe is missing. Pass --steam-exe or ensure HKCU\\Software\\Valve\\Steam\\SteamExe exists.")?;

    Ok(ResolvedLaunchPaths { steam_exe })
}

pub fn sort_accounts(accounts: &mut [SteamAccount]) {
    accounts.sort_by(compare_accounts);
}

pub fn format_timestamp(timestamp: i64) -> String {
    if timestamp <= 0 {
        return String::new();
    }

    Local
        .timestamp_opt(timestamp, 0)
        .single()
        .map(|value| value.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default()
}

fn compare_accounts(left: &SteamAccount, right: &SteamAccount) -> Ordering {
    right
        .most_recent
        .cmp(&left.most_recent)
        .then_with(|| right.remember_password.cmp(&left.remember_password))
        .then_with(|| right.last_login_timestamp.cmp(&left.last_login_timestamp))
        .then_with(|| left.account_name.cmp(&right.account_name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::windows_registry::RegistryStore;

    #[derive(Default)]
    struct MockRegistry {
        steam_path: Option<PathBuf>,
        steam_exe: Option<PathBuf>,
    }

    impl RegistryStore for MockRegistry {
        fn read_steam_path(&self) -> Result<Option<PathBuf>> {
            Ok(self.steam_path.clone())
        }

        fn read_steam_exe(&self) -> Result<Option<PathBuf>> {
            Ok(self.steam_exe.clone())
        }

        fn read_auto_login_user(&self) -> Result<Option<String>> {
            Ok(None)
        }

        fn write_auto_login_user(&mut self, _value: &str) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn explicit_overrides_take_priority_over_registry() {
        let registry = MockRegistry {
            steam_path: Some(PathBuf::from(r"C:\RegistrySteam")),
            steam_exe: Some(PathBuf::from(r"C:\RegistrySteam\steam.exe")),
        };
        let overrides = PathOverrides {
            steam_dir: Some(PathBuf::from(r"D:\CustomSteam")),
            steam_exe: Some(PathBuf::from(r"D:\CustomSteam\steam.exe")),
        };

        let resolved = resolve_steam_paths(&overrides, &registry).expect("paths should resolve");

        assert_eq!(resolved.steam_dir, PathBuf::from(r"D:\CustomSteam"));
        assert_eq!(
            resolved.steam_exe,
            PathBuf::from(r"D:\CustomSteam\steam.exe")
        );
        assert_eq!(
            resolved.loginusers_vdf,
            PathBuf::from(r"D:\CustomSteam\config\loginusers.vdf")
        );
    }

    #[test]
    fn registry_is_used_when_overrides_are_missing() {
        let registry = MockRegistry {
            steam_path: Some(PathBuf::from(r"C:\RegistrySteam")),
            steam_exe: Some(PathBuf::from(r"C:\RegistrySteam\steam.exe")),
        };

        let resolved = resolve_steam_paths(&PathOverrides::default(), &registry)
            .expect("paths should resolve");

        assert_eq!(resolved.steam_dir, PathBuf::from(r"C:\RegistrySteam"));
        assert_eq!(
            resolved.steam_exe,
            PathBuf::from(r"C:\RegistrySteam\steam.exe")
        );
    }
}
