# ac_switch_rust: Windows 极简独立版 Steam Account Switching CLI

## Summary
- 在 `X:\code\steampp\ac_switch_rust` 下创建一个完全独立的 Rust 二进制工程，不接入现有 `.sln`、不修改现有项目文件。
- 语义以当前 C# 的 [SteamAccountPageViewModel.cs](x:\code\steampp\src\ST.Client\UI\ViewModels\Pages\SteamAccountPageViewModel.cs) 和 [SteamServiceImpl.cs](x:\code\steampp\src\ST.Client.Desktop.Windows\Services\Implementation\SteamServiceImpl.cs) 为参考，但实现按第一性原理收敛为两个真实状态源：
  1. `HKCU\Software\Valve\Steam\AutoLoginUser`
  2. `Steam\config\loginusers.vdf`
- v1 只做 Windows CLI，不做 GUI、不做网络请求、不做头像/远程资料拉取、不做 `config.vdf`/家庭共享/网页登录/Steam Guard 相关能力。

## Public Interface
- 二进制名固定为 `ac_switch_rust.exe`。
- 命令固定为 3 个：
  1. `ac_switch_rust refresh [--json] [--steam-dir <path>] [--steam-exe <path>]`
     - 重新扫描 Steam 安装和 `loginusers.vdf`，默认按表格输出。
     - `--json` 时输出稳定 JSON 数组，字段固定为：
       - `steam_id64`
       - `account_name`
       - `persona_name`
       - `remember_password`
       - `most_recent`
       - `wants_offline_mode`
       - `last_login_timestamp`
       - `last_login_time`
       - `is_auto_login_user`
     - 排序固定为：`most_recent desc` -> `remember_password desc` -> `last_login_timestamp desc`。
  2. `ac_switch_rust login-new [--steam-dir <path>] [--steam-exe <path>]`
     - 安全关闭/强杀 Steam。
     - 仅把 `AutoLoginUser` 清空。
     - 不改写 `loginusers.vdf`。
     - 重新启动 Steam，进入新账号登录流程。
  3. `ac_switch_rust start (--account <name> | --steamid64 <id>) --mode <express|offline> [--steam-dir <path>] [--steam-exe <path>]`
     - 先安全关闭/强杀 Steam。
     - 在 `loginusers.vdf` 中把目标账号设为 `MostRecent=1`，其余账号设为 `MostRecent=0`。
     - `--mode express`：目标账号写入 `WantsOfflineMode=0`，注册表 `AutoLoginUser` 设为该账号 `AccountName`，然后启动 Steam。
     - `--mode offline`：目标账号写入 `WantsOfflineMode=1` 且 `SkipOfflineModeWarning=1`，注册表同上，然后启动 Steam。
     - `express` 模式下如果该账号 `RememberPassword != 1`，命令直接报错退出，不尝试“伪快速登录”。

## Implementation Changes
- 工程结构保持最小但分层清晰：
  - `src/main.rs`：CLI 入口与退出码。
  - `src/app.rs`：3 个命令的应用服务编排。
  - `src/steam.rs`：Steam 安装定位、路径归一化、领域模型。
  - `src/loginusers_vdf.rs`：最小 KeyValues/VDF 解析与回写。
  - `src/windows_registry.rs`：`SteamPath` / `SteamExe` / `AutoLoginUser` 读取与写入。
  - `src/process_control.rs`：Steam 关闭、等待、强杀、启动。
  - `src/error.rs`：统一错误类型。
- 依赖固定为：
  - `clap`：CLI
  - `winreg`：Windows 注册表
  - `sysinfo`：查找/终止 `steam.exe`、`steamservice.exe`、`steamwebhelper.exe`
  - `serde` + `serde_json`：`refresh --json`
  - `anyhow` 或 `thiserror`：错误处理
- `loginusers.vdf` 不使用重量级第三方 VDF 库，直接实现一个只覆盖 Steam 该文件结构的最小 parser/writer：
  - 支持大小写差异：`MostRecent` / `mostrecent`
  - 保留未知字段
  - 保留用户节点顺序
  - 回写时只修改 `MostRecent` / `WantsOfflineMode` / `SkipOfflineModeWarning`
  - 支持可选 UTF-8 BOM，输出统一为无 BOM UTF-8
- Steam 关闭流程固定为：
  1. 若 `steam.exe` 存在，先执行 `steam.exe -shutdown`
  2. 等待最多 5 秒
  3. 若仍存活，强杀 `steam.exe`、`steamservice.exe`、`steamwebhelper.exe`
  4. 确认进程消失后再改注册表/VDF
- Steam 启动流程固定为：
  - 直接启动解析到的 `SteamExe`
  - 不附加 `-login`
  - 不复用现有项目里的启动参数配置
- Steam 路径解析优先级固定为：
  1. CLI 显式传入
  2. `HKCU\Software\Valve\Steam\SteamPath` / `SteamExe`
  3. 若缺失则报错，不做猜测
- 文件写入策略固定为原子替换：
  - 先写同目录临时文件
  - flush
  - replace 原文件
  - 失败时保留原文件不变

## Test Plan
- 单元测试：
  - 解析包含 `MostRecent` 和 `mostrecent` 两种字段名的 `loginusers.vdf`
  - 正确读取 `RememberPassword`、`WantsOfflineMode`、`Timestamp`
  - 回写后仅目标字段变化，未知字段保留
  - `login-new` 不改 VDF，只清空注册表目标值
  - `start --mode express` 在 `RememberPassword=0` 时返回错误
  - 账号选择同时覆盖 `--account` 和 `--steamid64`
- 夹具测试：
  - 在 `tests/fixtures/` 放 2 到 3 份最小 `loginusers.vdf` 样例，覆盖单账号、多账号、离线标记三种场景
- 端到端服务测试：
  - 用 trait/mock 替代真实注册表、进程控制和文件系统副作用
  - 验证 3 个命令的动作顺序和最终状态
- 验收标准：
  1. `refresh` 能稳定列出本地已记住账号
  2. `login-new` 会清空 `AutoLoginUser` 并启动 Steam
  3. `start --mode express` 会把目标账号设为最近账号并在线启动
  4. `start --mode offline` 会把目标账号设为最近账号并离线启动
  5. 不依赖当前 C# 工程运行时

## Assumptions
- v1 仅支持 Windows。
- `express login` 的定义已锁定为“切到 Steam 已记住密码的账号并直接上线启动”，不是重新输入用户名密码，因此不实现 `steam.exe -login`、不保存任何新凭据。
- `refresh` 是无状态重扫，不引入本地缓存文件。
- 当前机器未安装 Rust 工具链，后续实现阶段可以生成源码，但无法在本机会话内执行 `cargo build` / `cargo test`；若要做真实编译验证，需要用户先在工作空间外自行准备可用 Rust 工具链，或提供一个已放入工作空间的便携工具链。
- 按你的约束，实施和测试阶段不会读取或写入工作空间外的真实 Steam 文件；验证会基于仓库内 fixture/mock 完成。
