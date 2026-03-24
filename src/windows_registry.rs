//! Windows registry access for Steam installation paths and `AutoLoginUser`.

use crate::error::Result;
use anyhow::Context;
use std::io::ErrorKind;
use std::path::PathBuf;
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

const STEAM_REGISTRY_PATH: &str = r"SOFTWARE\Valve\Steam";

pub trait RegistryStore {
    fn read_steam_path(&self) -> Result<Option<PathBuf>>;
    fn read_steam_exe(&self) -> Result<Option<PathBuf>>;
    fn read_auto_login_user(&self) -> Result<Option<String>>;
    fn write_auto_login_user(&mut self, value: &str) -> Result<()>;
}

#[derive(Default)]
pub struct WindowsRegistry;

impl WindowsRegistry {
    pub fn new() -> Self {
        Self
    }

    fn open_steam_key(&self) -> Result<Option<RegKey>> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        match hkcu.open_subkey(STEAM_REGISTRY_PATH) {
            Ok(key) => Ok(Some(key)),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error)
                .with_context(|| format!("Failed to open registry key {STEAM_REGISTRY_PATH}")),
        }
    }

    fn read_string_value(&self, value_name: &str) -> Result<Option<String>> {
        let Some(key) = self.open_steam_key()? else {
            return Ok(None);
        };

        match key.get_value::<String, _>(value_name) {
            Ok(value) if value.trim().is_empty() => Ok(None),
            Ok(value) => Ok(Some(value)),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error).with_context(|| {
                format!("Failed to read registry value {STEAM_REGISTRY_PATH}\\{value_name}")
            }),
        }
    }
}

impl RegistryStore for WindowsRegistry {
    fn read_steam_path(&self) -> Result<Option<PathBuf>> {
        Ok(self.read_string_value("SteamPath")?.map(PathBuf::from))
    }

    fn read_steam_exe(&self) -> Result<Option<PathBuf>> {
        Ok(self.read_string_value("SteamExe")?.map(PathBuf::from))
    }

    fn read_auto_login_user(&self) -> Result<Option<String>> {
        self.read_string_value("AutoLoginUser")
    }

    fn write_auto_login_user(&mut self, value: &str) -> Result<()> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu
            .create_subkey(STEAM_REGISTRY_PATH)
            .with_context(|| format!("Failed to create registry key {STEAM_REGISTRY_PATH}"))?;
        key.set_value("AutoLoginUser", &value).with_context(|| {
            format!("Failed to write registry value {STEAM_REGISTRY_PATH}\\AutoLoginUser")
        })?;
        Ok(())
    }
}
