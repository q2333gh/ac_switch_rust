# Command-Line Help for `ac_switch_rust`

This document contains the help content for the `ac_switch_rust` command-line program.

**Command Overview:**

* [`ac_switch_rust`](#ac_switch_rust)
* [`ac_switch_rust refresh`](#ac_switch_rust-refresh)
* [`ac_switch_rust login-new`](#ac_switch_rust-login-new)
* [`ac_switch_rust start`](#ac_switch_rust-start)

## `ac_switch_rust`

Minimal standalone Steam account switching CLI for Windows.

The CLI only relies on Steam's local account-switching state: `HKCU\Software\Valve\Steam\AutoLoginUser` and `Steam\config\loginusers.vdf`.

**Usage:** `ac_switch_rust <COMMAND>`

###### **Subcommands:**

* `refresh` - Rescan local Steam accounts from the registry and `loginusers.vdf`
* `login-new` - Clear `AutoLoginUser` and restart Steam into the normal new-login flow
* `start` - Switch to a remembered account and restart Steam in `express` or `offline` mode



## `ac_switch_rust refresh`

Rescan local Steam accounts from the registry and `loginusers.vdf`

**Usage:** `ac_switch_rust refresh [OPTIONS]`

###### **Options:**

* `--steam-dir <STEAM_DIR>` - Override the Steam installation directory instead of reading `SteamPath` from the registry
* `--steam-exe <STEAM_EXE>` - Override the Steam executable path instead of reading `SteamExe` from the registry
* `--json` - Print the refreshed account list as JSON instead of a table



## `ac_switch_rust login-new`

Clear `AutoLoginUser` and restart Steam into the normal new-login flow

**Usage:** `ac_switch_rust login-new [OPTIONS]`

###### **Options:**

* `--steam-dir <STEAM_DIR>` - Override the Steam installation directory instead of reading `SteamPath` from the registry
* `--steam-exe <STEAM_EXE>` - Override the Steam executable path instead of reading `SteamExe` from the registry



## `ac_switch_rust start`

Switch to a remembered account and restart Steam in `express` or `offline` mode

**Usage:** `ac_switch_rust start [OPTIONS] --mode <MODE>`

###### **Options:**

* `--steam-dir <STEAM_DIR>` - Override the Steam installation directory instead of reading `SteamPath` from the registry
* `--steam-exe <STEAM_EXE>` - Override the Steam executable path instead of reading `SteamExe` from the registry
* `--account <ACCOUNT>` - Select the target account by Steam account name
* `--steamid64 <STEAMID64>` - Select the target account by 64-bit Steam ID
* `--mode <MODE>` - Choose whether Steam should start online (`express`) or in offline mode

  Possible values:
  - `express`:
    Start the selected remembered account online
  - `offline`:
    Start the selected remembered account in offline mode




<hr/>

<small><i>
    This document was generated automatically by
    <a href="https://crates.io/crates/clap-markdown"><code>clap-markdown</code></a>.
</i></small>
