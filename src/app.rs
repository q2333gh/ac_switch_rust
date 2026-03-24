use crate::error::Result;
use crate::loginusers_vdf::LoginUsersVdf;
use crate::process_control::ProcessController;
use crate::steam::{
    resolve_launch_paths, resolve_steam_paths, sort_accounts, AccountSelector, PathOverrides, StartMode,
    SteamAccount,
};
use crate::windows_registry::RegistryStore;
use anyhow::{anyhow, bail, Context};
use std::ffi::c_void;
use std::fs::{self, File};
use std::io::Write;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const STEAM_PROCESS_NAMES: [&str; 3] = ["steam.exe", "steamservice.exe", "steamwebhelper.exe"];
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

pub trait FileStore {
    fn exists(&self, path: &Path) -> bool;
    fn read_to_string(&self, path: &Path) -> Result<String>;
    fn write_atomic_string(&mut self, path: &Path, contents: &str) -> Result<()>;
}

#[derive(Default)]
pub struct RealFileStore;

impl FileStore for RealFileStore {
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn read_to_string(&self, path: &Path) -> Result<String> {
        Ok(fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?)
    }

    fn write_atomic_string(&mut self, path: &Path, contents: &str) -> Result<()> {
        let parent = path
            .parent()
            .with_context(|| format!("{} does not have a parent directory", path.display()))?;
        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .with_context(|| format!("{} does not have a valid file name", path.display()))?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temp_path = parent.join(format!("{file_name}.{nonce}.tmp"));

        let write_result = (|| -> Result<()> {
            let mut file = File::create(&temp_path)
                .with_context(|| format!("Failed to create temporary file {}", temp_path.display()))?;
            file.write_all(contents.as_bytes())
                .with_context(|| format!("Failed to write temporary file {}", temp_path.display()))?;
            file.flush()
                .with_context(|| format!("Failed to flush temporary file {}", temp_path.display()))?;
            file.sync_all()
                .with_context(|| format!("Failed to sync temporary file {}", temp_path.display()))?;
            replace_file(&temp_path, path)?;
            Ok(())
        })();

        if write_result.is_err() {
            let _ = fs::remove_file(&temp_path);
        }

        write_result
    }
}

fn validate_existing_file(path: &Path, label: &str) -> Result<()> {
    if path.is_file() {
        Ok(())
    } else {
        bail!("{label} was not found at {}", path.display())
    }
}

fn validate_existing_dir(path: &Path, label: &str) -> Result<()> {
    if path.is_dir() {
        Ok(())
    } else {
        bail!("{label} was not found at {}", path.display())
    }
}

pub struct App<R, P, F> {
    pub(crate) registry: R,
    pub(crate) processes: P,
    pub(crate) files: F,
}

#[derive(Clone, Debug)]
pub struct StartRequest {
    pub selector: AccountSelector,
    pub mode: StartMode,
}

impl<R, P, F> App<R, P, F>
where
    R: RegistryStore,
    P: ProcessController,
    F: FileStore,
{
    pub fn new(registry: R, processes: P, files: F) -> Self {
        Self {
            registry,
            processes,
            files,
        }
    }

    pub fn refresh(&self, overrides: PathOverrides) -> Result<Vec<SteamAccount>> {
        let paths = resolve_steam_paths(&overrides, &self.registry)?;
        validate_existing_dir(&paths.steam_dir, "SteamPath")?;
        validate_existing_file(&paths.steam_exe, "SteamExe")?;
        if !self.files.exists(&paths.loginusers_vdf) {
            return Ok(Vec::new());
        }

        let auto_login_user = self.registry.read_auto_login_user()?;
        let content = self.files.read_to_string(&paths.loginusers_vdf)?;
        let mut accounts = LoginUsersVdf::parse(&content)?.accounts(auto_login_user.as_deref())?;
        sort_accounts(&mut accounts);
        Ok(accounts)
    }

    pub fn login_new(&mut self, overrides: PathOverrides) -> Result<()> {
        let paths = resolve_launch_paths(&overrides, &self.registry)?;
        validate_existing_file(&paths.steam_exe, "SteamExe")?;
        self.shutdown_running_steam(&paths.steam_exe)?;
        self.registry.write_auto_login_user("")?;
        self.processes.launch_steam(&paths.steam_exe)?;
        Ok(())
    }

    pub fn start(&mut self, overrides: PathOverrides, request: StartRequest) -> Result<SteamAccount> {
        let paths = resolve_steam_paths(&overrides, &self.registry)?;
        validate_existing_dir(&paths.steam_dir, "SteamPath")?;
        validate_existing_file(&paths.steam_exe, "SteamExe")?;
        if !self.files.exists(&paths.loginusers_vdf) {
            bail!("{} was not found", paths.loginusers_vdf.display());
        }

        let preflight = self.read_accounts(&paths.loginusers_vdf)?;
        let target = preflight
            .iter()
            .find(|account| account.matches(&request.selector))
            .cloned()
            .ok_or_else(|| anyhow!("Target account was not found in loginusers.vdf"))?;

        if target.account_name.is_empty() {
            bail!("Target account is missing AccountName and cannot become AutoLoginUser");
        }

        if request.mode == StartMode::Express && !target.remember_password {
            bail!(
                "Account {} does not have RememberPassword=1 and cannot use express mode",
                target.account_name
            );
        }

        self.shutdown_running_steam(&paths.steam_exe)?;

        let content = self.files.read_to_string(&paths.loginusers_vdf)?;
        let mut vdf = LoginUsersVdf::parse(&content)?;
        let updated = vdf.set_active_account(&request.selector, request.mode, &target.account_name)?;
        self.files
            .write_atomic_string(&paths.loginusers_vdf, &vdf.render())?;
        self.registry.write_auto_login_user(&target.account_name)?;
        self.processes.launch_steam(&paths.steam_exe)?;

        Ok(updated)
    }

    fn read_accounts(&self, loginusers_path: &Path) -> Result<Vec<SteamAccount>> {
        let auto_login_user = self.registry.read_auto_login_user()?;
        let content = self.files.read_to_string(loginusers_path)?;
        LoginUsersVdf::parse(&content)?.accounts(auto_login_user.as_deref())
    }

    fn shutdown_running_steam(&mut self, steam_exe: &Path) -> Result<()> {
        if !self.processes.is_running(&STEAM_PROCESS_NAMES)? {
            return Ok(());
        }

        self.processes.shutdown_steam(steam_exe)?;
        if self
            .processes
            .wait_for_exit(&STEAM_PROCESS_NAMES, SHUTDOWN_TIMEOUT)?
        {
            return Ok(());
        }

        self.processes.force_kill(&STEAM_PROCESS_NAMES)?;
        if self
            .processes
            .wait_for_exit(&STEAM_PROCESS_NAMES, SHUTDOWN_TIMEOUT)?
        {
            return Ok(());
        }

        bail!("Steam processes are still running after force kill");
    }
}

#[link(name = "Kernel32")]
extern "system" {
    fn ReplaceFileW(
        replaced_file_name: *const u16,
        replacement_file_name: *const u16,
        backup_file_name: *const u16,
        replace_flags: u32,
        exclude: *mut c_void,
        reserved: *mut c_void,
    ) -> i32;
}

fn replace_file(source: &Path, target: &Path) -> Result<()> {
    if !target.exists() {
        fs::rename(source, target).with_context(|| {
            format!(
                "Failed to rename temporary file {} to {}",
                source.display(),
                target.display()
            )
        })?;
        return Ok(());
    }

    let mut target_wide: Vec<u16> = target.as_os_str().encode_wide().collect();
    target_wide.push(0);
    let mut source_wide: Vec<u16> = source.as_os_str().encode_wide().collect();
    source_wide.push(0);

    let result = unsafe {
        ReplaceFileW(
            target_wide.as_ptr(),
            source_wide.as_ptr(),
            std::ptr::null(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };

    if result == 0 {
        return Err(std::io::Error::last_os_error())
            .with_context(|| format!("Failed to replace {} atomically", target.display()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process_control::ProcessController;
    use crate::windows_registry::RegistryStore;
    use std::collections::HashMap;
    use std::path::PathBuf;

    const MULTI_USER: &str = include_str!("../tests/fixtures/multi_user.vdf");

    #[derive(Default)]
    struct MockRegistry {
        steam_path: Option<PathBuf>,
        steam_exe: Option<PathBuf>,
        auto_login_user: Option<String>,
        writes: Vec<String>,
    }

    impl RegistryStore for MockRegistry {
        fn read_steam_path(&self) -> Result<Option<PathBuf>> {
            Ok(self.steam_path.clone())
        }

        fn read_steam_exe(&self) -> Result<Option<PathBuf>> {
            Ok(self.steam_exe.clone())
        }

        fn read_auto_login_user(&self) -> Result<Option<String>> {
            Ok(self.auto_login_user.clone())
        }

        fn write_auto_login_user(&mut self, value: &str) -> Result<()> {
            self.auto_login_user = if value.is_empty() {
                None
            } else {
                Some(value.to_owned())
            };
            self.writes.push(value.to_owned());
            Ok(())
        }
    }

    #[derive(Default)]
    struct MockProcessController {
        log: Vec<String>,
        wait_results: Vec<bool>,
    }

    impl ProcessController for MockProcessController {
        fn is_running(&mut self, _process_names: &[&str]) -> Result<bool> {
            Ok(true)
        }

        fn shutdown_steam(&mut self, steam_exe: &Path) -> Result<()> {
            self.log.push(format!("shutdown:{}", steam_exe.display()));
            Ok(())
        }

        fn wait_for_exit(&mut self, _process_names: &[&str], _timeout: Duration) -> Result<bool> {
            Ok(self.wait_results.pop().unwrap_or(true))
        }

        fn force_kill(&mut self, _process_names: &[&str]) -> Result<()> {
            self.log.push("force_kill".to_owned());
            Ok(())
        }

        fn launch_steam(&mut self, steam_exe: &Path) -> Result<()> {
            self.log.push(format!("launch:{}", steam_exe.display()));
            Ok(())
        }
    }

    #[derive(Default)]
    struct MockIdleProcessController {
        log: Vec<String>,
    }

    impl ProcessController for MockIdleProcessController {
        fn is_running(&mut self, _process_names: &[&str]) -> Result<bool> {
            Ok(false)
        }

        fn shutdown_steam(&mut self, steam_exe: &Path) -> Result<()> {
            self.log.push(format!("shutdown:{}", steam_exe.display()));
            Ok(())
        }

        fn wait_for_exit(&mut self, _process_names: &[&str], _timeout: Duration) -> Result<bool> {
            Ok(true)
        }

        fn force_kill(&mut self, _process_names: &[&str]) -> Result<()> {
            self.log.push("force_kill".to_owned());
            Ok(())
        }

        fn launch_steam(&mut self, steam_exe: &Path) -> Result<()> {
            self.log.push(format!("launch:{}", steam_exe.display()));
            Ok(())
        }
    }

    #[derive(Default)]
    struct MockFileStore {
        files: HashMap<PathBuf, String>,
        writes: Vec<(PathBuf, String)>,
    }

    impl FileStore for MockFileStore {
        fn exists(&self, path: &Path) -> bool {
            self.files.contains_key(path)
        }

        fn read_to_string(&self, path: &Path) -> Result<String> {
            self.files
                .get(path)
                .cloned()
                .with_context(|| format!("Missing mock file {}", path.display()))
        }

        fn write_atomic_string(&mut self, path: &Path, contents: &str) -> Result<()> {
            self.files.insert(path.to_path_buf(), contents.to_owned());
            self.writes.push((path.to_path_buf(), contents.to_owned()));
            Ok(())
        }
    }

    fn build_app(
        auto_login_user: Option<&str>,
        include_vdf: bool,
    ) -> (App<MockRegistry, MockProcessController, MockFileStore>, PathBuf) {
        let steam_dir = std::env::temp_dir();
        let steam_exe = std::env::current_exe().expect("test host exe should exist");
        let loginusers = steam_dir.join("config").join("loginusers.vdf");

        let registry = MockRegistry {
            steam_path: Some(steam_dir.clone()),
            steam_exe: Some(steam_exe),
            auto_login_user: auto_login_user.map(str::to_owned),
            writes: Vec::new(),
        };

        let mut files = MockFileStore::default();
        if include_vdf {
            files.files.insert(loginusers.clone(), MULTI_USER.to_owned());
        }

        (App::new(registry, MockProcessController::default(), files), loginusers)
    }

    #[test]
    fn login_new_clears_registry_without_touching_vdf() {
        let (mut app, loginusers) = build_app(Some("alpha_user"), true);

        app.login_new(PathOverrides::default())
            .expect("login-new should succeed");

        assert_eq!(app.registry.writes, vec![String::new()]);
        assert!(app.files.writes.is_empty());
        assert_eq!(
            app.files
                .files
                .get(&loginusers)
                .expect("fixture should still exist"),
            MULTI_USER
        );
    }

    #[test]
    fn start_express_requires_remember_password() {
        let (mut app, _) = build_app(Some("alpha_user"), true);

        let error = app
            .start(
                PathOverrides::default(),
                StartRequest {
                    selector: AccountSelector::AccountName("beta_user".to_owned()),
                    mode: StartMode::Express,
                },
            )
            .expect_err("express mode should fail for non-remembered account");

        assert!(error.to_string().contains("RememberPassword=1"));
        assert!(app.files.writes.is_empty());
        assert!(app.registry.writes.is_empty());
    }

    #[test]
    fn start_offline_updates_vdf_and_launches() {
        let (mut app, loginusers) = build_app(Some("alpha_user"), true);

        let updated = app
            .start(
                PathOverrides::default(),
                StartRequest {
                    selector: AccountSelector::SteamId64(76561198000000001),
                    mode: StartMode::Offline,
                },
            )
            .expect("offline start should succeed");

        assert!(updated.most_recent);
        assert!(updated.wants_offline_mode);
        assert_eq!(app.registry.writes, vec!["alpha_user".to_owned()]);
        assert_eq!(app.processes.log.len(), 2);

        let written = app
            .files
            .files
            .get(&loginusers)
            .expect("updated vdf should exist");
        assert!(written.contains("\"WantsOfflineMode\"\t\t\"1\""));
        assert!(written.contains("\"SkipOfflineModeWarning\"\t\t\"1\""));
    }

    #[test]
    fn start_express_by_account_name_updates_online_mode() {
        let (mut app, loginusers) = build_app(Some("beta_user"), true);

        let updated = app
            .start(
                PathOverrides::default(),
                StartRequest {
                    selector: AccountSelector::AccountName("alpha_user".to_owned()),
                    mode: StartMode::Express,
                },
            )
            .expect("express start should succeed");

        assert_eq!(updated.account_name, "alpha_user");
        assert!(!updated.wants_offline_mode);
        assert_eq!(app.registry.writes, vec!["alpha_user".to_owned()]);

        let written = app
            .files
            .files
            .get(&loginusers)
            .expect("updated vdf should exist");
        assert!(written.contains("\"WantsOfflineMode\"\t\t\"0\""));
    }

    #[test]
    fn login_new_skips_shutdown_when_steam_is_not_running() {
        let steam_dir = std::env::temp_dir();
        let steam_exe = std::env::current_exe().expect("test host exe should exist");
        let registry = MockRegistry {
            steam_path: Some(steam_dir),
            steam_exe: Some(steam_exe.clone()),
            auto_login_user: Some("alpha_user".to_owned()),
            writes: Vec::new(),
        };
        let files = MockFileStore::default();
        let mut app = App::new(registry, MockIdleProcessController::default(), files);

        app.login_new(PathOverrides::default())
            .expect("login-new should succeed");

        assert_eq!(app.processes.log, vec![format!("launch:{}", steam_exe.display())]);
        assert_eq!(app.registry.writes, vec![String::new()]);
    }

    #[test]
    fn refresh_sorts_accounts_and_marks_auto_login_user() {
        let (app, _) = build_app(Some("alpha_user"), true);

        let accounts = app.refresh(PathOverrides::default()).expect("refresh should succeed");

        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0].account_name, "alpha_user");
        assert!(accounts[0].is_auto_login_user);
        assert_eq!(accounts[1].account_name, "beta_user");
        assert!(!accounts[1].is_auto_login_user);
    }

    #[test]
    fn refresh_returns_empty_when_loginusers_vdf_is_missing() {
        let (app, _) = build_app(Some("alpha_user"), false);

        let accounts = app.refresh(PathOverrides::default()).expect("refresh should succeed");

        assert!(accounts.is_empty());
    }

    #[test]
    fn start_force_kills_after_shutdown_timeout_then_continues() {
        let (mut app, _) = build_app(Some("alpha_user"), true);
        app.processes.wait_results = vec![true, false];

        let updated = app
            .start(
                PathOverrides::default(),
                StartRequest {
                    selector: AccountSelector::SteamId64(76561198000000001),
                    mode: StartMode::Offline,
                },
            )
            .expect("offline start should succeed after force kill");

        assert_eq!(updated.account_name, "alpha_user");
        assert_eq!(
            app.processes.log,
            vec![
                format!("shutdown:{}", app.registry.steam_exe.as_ref().expect("steam exe").display()),
                String::from("force_kill"),
                format!("launch:{}", app.registry.steam_exe.as_ref().expect("steam exe").display()),
            ]
        );
        assert_eq!(app.registry.writes, vec!["alpha_user".to_owned()]);
    }

    #[test]
    fn start_fails_if_processes_still_exist_after_force_kill() {
        let (mut app, _) = build_app(Some("alpha_user"), true);
        app.processes.wait_results = vec![false, false];

        let error = app
            .start(
                PathOverrides::default(),
                StartRequest {
                    selector: AccountSelector::SteamId64(76561198000000001),
                    mode: StartMode::Offline,
                },
            )
            .expect_err("start should fail if steam is still running");

        assert!(error.to_string().contains("still running after force kill"));
        assert!(app.registry.writes.is_empty());
        assert!(app.files.writes.is_empty());
        assert_eq!(
            app.processes.log,
            vec![
                format!("shutdown:{}", app.registry.steam_exe.as_ref().expect("steam exe").display()),
                String::from("force_kill"),
            ]
        );
    }
}
