# ac_switch_rust

<!-- cargo-rdme start -->

`ac_switch_rust` is a minimal standalone Steam account-switching CLI for Windows.

The implementation deliberately reduces the switching model to the two state sources that Steam
actually uses for remembered-account switching on Windows:

- `HKCU\Software\Valve\Steam\AutoLoginUser`
- `Steam\config\loginusers.vdf`

The binary exposes exactly three commands:

- `refresh`: rescan local account state from the registry and `loginusers.vdf`
- `login-new`: clear `AutoLoginUser` and restart Steam into the normal new-login flow
- `start`: switch to a remembered account in `express` or `offline` mode

Quick start:

```powershell
ac_switch_rust refresh
ac_switch_rust login-new
ac_switch_rust start --account alpha_user --mode express
ac_switch_rust start --steamid64 76561198000000001 --mode offline
```

Core behavior constraints:

- Steam is always shut down before registry or VDF mutation.
- `express` mode requires `RememberPassword=1` for the selected account.
- `login-new` only clears `AutoLoginUser`; it does not rewrite `loginusers.vdf`.
- `start` only rewrites `MostRecent`, `WantsOfflineMode`, and `SkipOfflineModeWarning`.

The generated command reference lives in `doc/cli.md`. Maintainer-facing details come from
`cargo doc --no-deps --document-private-items`.

To refresh generated documentation after editing source comments:

```powershell
.\scripts\generate-docs.ps1
```

<!-- cargo-rdme end -->
