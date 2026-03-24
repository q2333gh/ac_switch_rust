//! Command-line schema and user-facing console rendering.
//!
//! This module is the single source of truth for command descriptions, option help text, and the
//! generated Markdown command reference.

use crate::app::{App, FileStore, StartRequest};
use crate::error::Result;
use crate::process_control::ProcessController;
use crate::steam::{sort_accounts, AccountSelector, PathOverrides, StartMode, SteamAccount};
use crate::windows_registry::RegistryStore;
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// Minimal standalone Steam account switching CLI for Windows.
///
/// The CLI only relies on Steam's local account-switching state:
/// `HKCU\Software\Valve\Steam\AutoLoginUser` and `Steam\config\loginusers.vdf`.
#[derive(Debug, Parser)]
#[command(name = "ac_switch_rust", version)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Rescan local Steam accounts from the registry and `loginusers.vdf`.
    Refresh(RefreshArgs),
    /// Clear `AutoLoginUser` and restart Steam into the normal new-login flow.
    LoginNew(LoginNewArgs),
    /// Switch to a remembered account and restart Steam in `express` or `offline` mode.
    Start(StartArgs),
}

#[derive(Clone, Debug, Args, Default)]
struct PathArgs {
    /// Override the Steam installation directory instead of reading `SteamPath` from the registry.
    #[arg(long)]
    steam_dir: Option<PathBuf>,
    /// Override the Steam executable path instead of reading `SteamExe` from the registry.
    #[arg(long)]
    steam_exe: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct RefreshArgs {
    #[command(flatten)]
    paths: PathArgs,
    /// Print the refreshed account list as JSON instead of a table.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct LoginNewArgs {
    #[command(flatten)]
    paths: PathArgs,
}

#[derive(Debug, Args)]
struct StartArgs {
    #[command(flatten)]
    paths: PathArgs,
    /// Select the target account by Steam account name.
    #[arg(long, group = "selector")]
    account: Option<String>,
    /// Select the target account by 64-bit Steam ID.
    #[arg(long, group = "selector")]
    steamid64: Option<u64>,
    /// Choose whether Steam should start online (`express`) or in offline mode.
    #[arg(long, value_enum)]
    mode: ModeArg,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ModeArg {
    /// Start the selected remembered account online.
    Express,
    /// Start the selected remembered account in offline mode.
    Offline,
}

impl Cli {
    pub(crate) fn execute<R, P, F>(self, app: &mut App<R, P, F>) -> Result<()>
    where
        R: RegistryStore,
        P: ProcessController,
        F: FileStore,
    {
        match self.command {
            Commands::Refresh(args) => {
                let mut accounts = app.refresh(args.paths.into())?;
                sort_accounts(&mut accounts);
                if args.json {
                    println!("{}", serde_json::to_string_pretty(&accounts)?);
                } else {
                    print_accounts(&accounts);
                }
            }
            Commands::LoginNew(args) => {
                app.login_new(args.paths.into())?;
                println!("Steam restarted for new-account login.");
            }
            Commands::Start(args) => {
                let selector = if let Some(account) = args.account {
                    AccountSelector::AccountName(account)
                } else if let Some(steam_id64) = args.steamid64 {
                    AccountSelector::SteamId64(steam_id64)
                } else {
                    unreachable!("clap enforces selector group");
                };
                let mode = StartMode::from(args.mode);
                let updated = app.start(args.paths.into(), StartRequest { selector, mode })?;
                println!(
                    "Steam switched to {} ({}) in {} mode.",
                    display_or_dash(&updated.account_name),
                    updated.steam_id64,
                    mode_label(mode)
                );
            }
        }

        Ok(())
    }
}

/// Renders the generated CLI reference as stable ASCII Markdown.
pub fn render_cli_markdown() -> String {
    clap_markdown::help_markdown::<Cli>()
        .replace("↴", "")
        .replace("—", "-")
}

impl From<PathArgs> for PathOverrides {
    fn from(value: PathArgs) -> Self {
        Self {
            steam_dir: value.steam_dir,
            steam_exe: value.steam_exe,
        }
    }
}

impl From<ModeArg> for StartMode {
    fn from(value: ModeArg) -> Self {
        match value {
            ModeArg::Express => StartMode::Express,
            ModeArg::Offline => StartMode::Offline,
        }
    }
}

fn print_accounts(accounts: &[SteamAccount]) {
    if accounts.is_empty() {
        println!("No remembered Steam accounts found.");
        return;
    }

    let headers = [
        "steam_id64",
        "account_name",
        "persona_name",
        "remember_password",
        "most_recent",
        "wants_offline_mode",
        "last_login_time",
        "is_auto_login_user",
    ];

    let rows: Vec<[String; 8]> = accounts
        .iter()
        .map(|account| {
            [
                account.steam_id64.to_string(),
                display_or_dash(&account.account_name).to_owned(),
                display_or_dash(&account.persona_name).to_owned(),
                bool_digit(account.remember_password).to_owned(),
                bool_digit(account.most_recent).to_owned(),
                bool_digit(account.wants_offline_mode).to_owned(),
                display_or_dash(&account.last_login_time).to_owned(),
                bool_digit(account.is_auto_login_user).to_owned(),
            ]
        })
        .collect();

    let widths: Vec<usize> = headers
        .iter()
        .enumerate()
        .map(|(column, header)| {
            let max_cell = rows
                .iter()
                .map(|row| row[column].len())
                .max()
                .unwrap_or_default();
            header.len().max(max_cell)
        })
        .collect();

    for (index, header) in headers.iter().enumerate() {
        print!("{header:<width$} ", width = widths[index]);
    }
    println!();

    for width in &widths {
        print!("{:-<width$} ", "", width = *width);
    }
    println!();

    for row in &rows {
        for (index, cell) in row.iter().enumerate() {
            print!("{cell:<width$} ", width = widths[index]);
        }
        println!();
    }
}

fn bool_digit(value: bool) -> &'static str {
    if value {
        "1"
    } else {
        "0"
    }
}

fn display_or_dash(value: &str) -> &str {
    if value.is_empty() {
        "-"
    } else {
        value
    }
}

fn mode_label(value: StartMode) -> &'static str {
    match value {
        StartMode::Express => "express",
        StartMode::Offline => "offline",
    }
}
