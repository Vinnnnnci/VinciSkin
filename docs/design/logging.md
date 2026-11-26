# 日志与诊断包设计

- **状态**：部分实现（2026-11-25）。换肤链路修复、loader 日志、启动/panic/UI 日志、Windows 版本显示、脱敏写入器、run/session/game id、5 MB 轮转与总量上限、诊断页的 WebView2/架构/.NET 检查、诊断包构建与预览保存已落地。`--report`、启动失败原生对话框、GPU 检查、`ui-ready` 看门狗仍未落地。
- **范围**：`src-tauri/src/logging*`、`src-tauri/src/setup.rs`、`src-tauri/src/main.rs`、`crates/vinciskin-core/src/diagnostics`、安装器、设置页与诊断页
- **目标**：用户说“Win10 用不了”时，一次操作能拿到定位问题的证据。软件根本打不开时也要有证据。
- **非目标**：默认上传、默认遥测、自动修复、全量带走游戏日志

## 实现进度

| 项                                                                                       | 状态   | 位置                                                               |
| ---------------------------------------------------------------------------------------- | ------ | ------------------------------------------------------------------ |
| 换肤链路：失败不吞、应用检查宿主、游戏已运行提示、跟随先启动注入                         | 已实现 | `CnCatalog.tsx`、`CnSkinGrid.tsx`、`CnSessionControl.tsx`          |
| loader 输出捕获、`injected` 以 DLL ack 为准、job 结束进程树                              | 已实现 | `src-tauri/src/patcher.rs`                                         |
| 启动阶段日志、panic 日志、WebView2/OS 环境行                                             | 已实现 | `src-tauri/src/logging/`、`main.rs`、`setup.rs`                    |
| 前端错误回传、IPC 失败日志                                                               | 已实现 | `commands/app.rs`、`error.rs`、`main.tsx`、`ErrorBoundary.tsx`     |
| 日志文件路径与“打开日志文件”                                                             | 已实现 | `commands/app.rs`、`commands/shell.rs`                             |
| 诊断页 Windows 11 显示                                                                   | 已实现 | `diagnostics/windows.rs`                                           |
| 脱敏写入器：注册值 + 用户目录/URL/邮箱模式                                               | 已实现 | `redact.rs`、`logging/writer.rs`                                   |
| run/session/game id 写在每行                                                             | 已实现 | `logging/format.rs`、`logging/mod.rs`、`patcher.rs`                |
| 5 MB 单文件轮转、50 MB 总量、`boot.log` 256 KB                                           | 已实现 | `logging/writer.rs`、`logging/mod.rs`                              |
| `verbosePatcherLogging` 接到 reload handle                                               | 已实现 | `logging/mod.rs`、`state.rs`、`commands/settings.rs`               |
| 诊断页 WebView2、64 位、.NET Framework 4.8                                               | 已实现 | `diagnostics/windows.rs`                                           |
| 诊断包构建器：report.md、manifest、system/settings/library/patcher、incidents、logs、zip | 已实现 | `crates/vinciskin-core/src/support_report/`、`commands/support.rs` |
| 预览对话框、保存、复制摘要、设置页入口                                                   | 已实现 | `SupportReportDialog.tsx`、`AboutSection.tsx`                      |
| `--report` 无窗口路径、启动失败原生对话框、GPU 检查、`ui-ready` 看门狗                   | 未实现 | 见下文设计                                                         |

## 1. 结论

1. 现在已经有日志，但它只能回答“程序跑起来之后发生了什么”，回答不了“为什么程序没跑起来”。
2. Win10 与 Win11 的差异不在这个仓库的代码里。代码用到的 Win32 接口都是 Win10 就有的，没有 Win11 专用调用。差异在运行环境，最大的一项是 WebView2。
3. Windows 11 自带 Evergreen WebView2，Windows 10 大多数机器有，但不保证。当前的安装包在缺少运行时的机器上要联网下载。Win11 因为已经有运行时，安装器直接跳过这一步。这能解释“Win10 不行，Win11 行”。
4. 第二个原因是窗口以 `visible: false` 创建，只有前端渲染完成、调用 `show_main_window` 之后才显示。WebView2 缺失、版本过旧、前端脚本启动失败或 GPU 黑屏时，进程可能在后台活着，但窗口永远不出现。用户看到的是“点了没反应”，日志只写到 `Log directory` 一行。
5. 第三个原因是 Windows 10 版本低于 19045（22H2）或 32 位系统。代码里的诊断只把低版本标成 Bad，安装包没有拦截。
6. 设计分四层：启动日志、运行日志、环境事实、诊断包。前三层让日志能解释启动失败，最后一层让用户一次把证据发给维护者。

## 2. 现状与证据

### 2.1 已经有的

| 能力           | 位置                                                            | 说明                                                                  |
| -------------- | --------------------------------------------------------------- | --------------------------------------------------------------------- |
| 后端日志       | `src-tauri/src/logging.rs`                                      | `tracing`，写 `%APPDATA%\com.vinciskin.app\logs`，按天轮转，保留 7 天 |
| 开发期前端日志 | `src-tauri/src/log_layer.rs`                                    | 仅 `debug_assertions`，把 `tracing` 事件通过 `log-event` 发给前端     |
| 系统检查       | `crates/vinciskin-core/src/diagnostics/`                        | 13 项检查，含 Windows 版本、长路径、UAC、路径权限、存储介质           |
| 游戏事件记录   | `crates/vinciskin-core/src/diagnostics/incident.rs`、`store.rs` | 每局游戏一个 incident，存 `%APPDATA%\com.vinciskin.app\incidents`     |
| 诊断页         | `src/pages/Diagnostics.tsx`                                     | 运行检查、复制报告                                                    |
| 设置页日志入口 | `src/modules/settings/components/AboutSection.tsx`              | `Open Log File` 打开的是日志目录，不是文件                            |
| 遥测文档       | `docs/ux/TELEMETRY.md`                                          | 事件定义，不是本文的重点                                              |

### 2.2 缺口

| 缺口                             | 证据                                                              | 后果                                                                                 |
| -------------------------------- | ----------------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| 启动失败没有记录                 | `main.rs:36` 先初始化日志，`main.rs:237` 用 `.expect` 结束进程    | `tauri::Builder::build()` 失败时只有 panic，GUI 子系统没有控制台，用户看不到任何东西 |
| 前端错误不进日志                 | `main.rs:15-16` 把 `log_layer` 限制在 `debug_assertions`          | release 下 React 崩溃、脚本解析失败、IPC 失败都没有记录                              |
| 没有 panic hook                  | 全仓库没有 `panic::set_hook`                                      | panic 信息只到 stderr，release 下丢失                                                |
| 没有 WebView2 事实               | 全仓库没有 WebView2 版本读取                                      | 无法区分“运行时缺失”和“前端崩溃”                                                     |
| 日志含用户名                     | 实际日志行 `C:\Users\<name>\AppData\...`                          | 用户把日志贴到公开 issue 时泄露 Windows 用户名                                       |
| 无 run id                        | `logging.rs` 的 formatter 不写 id                                 | 同一天多次启动的行混在一起，无法切出一次运行                                         |
| 过滤没有默认级别                 | `logging.rs:18-19` 只有四个 target                                | 其他 crate 的 warn 被丢掉                                                            |
| 没有大小上限                     | `logging.rs:96-101` 只有按天轮转                                  | 长时间运行或 verbose 下单个文件无上界                                                |
| 无法一键打包                     | 只有“打开日志目录”                                                | 用户不知道该发哪个文件，也不知道还要发系统信息                                       |
| `verbosePatcherLogging` 没有接线 | `src/lib/bindings/Settings.ts:161` 有字段，`src-tauri/src` 无读取 | 设置项是死的                                                                         |
| 隐藏窗口没有兜底                 | `tauri.conf.json:22`、`src/main.tsx:31-33`                        | 前端没起来时窗口永远不显示                                                           |

### 2.3 一条实际日志说明了什么

本机 2026-11-24 的日志只有 9 行：

```
INFO vinciskin: Starting VinciSkin v1.23.0
INFO vinciskin: Log directory: C:\Users\<name>\AppData\Roaming\com.vinciskin.app\logs
INFO vinciskin::state: Loaded settings from "C:\\Users\\<name>\\...\\settings.json"
INFO vinciskin::setup: League path already configured, skipping auto-detection
DEBUG ritoclient_core::endpoint: GET /product-session/v1/external-sessions failed: no Riot Client is running
...
```

它同时暴露了三件事：

1. Windows 用户名在日志里，明文，两种斜杠写法都有。
2. 一次运行和下一次运行之间没有分隔，11-18 的文件里 6 次启动的行是连着的。
3. 11-18 有几次启动只写了两行就结束，没有任何原因。可能是单实例把新进程转交给旧进程，也可能是启动失败。现有日志无法区分。

## 3. 目标与验收

### 3.1 目标

- 软件打不开时，仍然能留下“走到哪一步”的记录。
- 用户按一次，得到一个脱敏的 zip，里面有日志、系统事实、设置快照、mod 列表和最近的事件。
- 日志文件在磁盘上不含 Windows 用户名、账户标识、deep link 的一次性令牌。
- 维护者读完 zip 就能回答“是 WebView2、是系统版本、是权限、还是 patcher”。
- 用户能在发送前看到 zip 里每个文件的文本。

### 3.2 验收

- 在一台删掉 WebView2 运行时的 Win10 虚拟机上，双击程序会出现原生对话框，写明缺少 WebView2，并且能通过 `--report` 生成 zip。
- 在一台前端启动失败的机器上，`boot.log` 的最后一行停在 `page-load-finished`，没有 `ui-ready`。
- 生成的 zip 里搜不到 `C:\Users\<name>`，搜不到 deep link 的查询串。
- 日志单文件超过 5 MB 时轮转，7 天前的文件被删除。
- 诊断页在这台机器上显示 `Windows 11 (26200)`，不是 `Windows 10.0.26200`。

## 4. Win10 / Win11 调查

### 4.1 代码里没有 Win11 专用接口

`src-tauri/Cargo.toml:85-98` 打开的 `windows-sys` 特性是 Foundation、Security、Cryptography、WinTrust、FileSystem、ToolHelp、Ioctl、IO、Registry、Threading、Shell、WindowsAndMessaging。这些接口在 Windows 10 上都有。

`crates/vinciskin-core/src/diagnostics/windows.rs:19-28` 直接读 `KUSER_SHARED_DATA`，这是为了绕开 `GetVersionExW` 的兼容性谎言，不是 Win11 专用。

`tauri.conf.json` 没有使用 Mica、Acrylic、圆角或 Windows App SDK。窗口是普通 Win32 窗口，`decorations: false`。

所以“程序在 Win11 能跑、Win10 不能跑”不是代码里的版本分支造成的。

### 4.2 WebView2 是最大变量

Tauri 2 在 Windows 上必须用 WebView2。当前版本是 `tauri 2.12.5`、`wry 0.57.0`（`Cargo.lock`）。

Microsoft 的说明：

- Windows 11 自带 Evergreen WebView2 Runtime。
- Windows 10 的大多数设备有，但“A small number of Windows 10 devices don't have the WebView2 Runtime installed”，应用应当自己检查并在缺失时安装。

当前安装包配置是 `webviewInstallMode: { "type": "downloadBootstrapper" }`（`tauri.conf.json:51-53`）。它的行为是：安装时如果发现没有 WebView2，就联网下载一个约 2 MB 的 bootstrapper，再由 bootstrapper 下载运行时。

这带来一个和 Win11 的系统性差异：

| 机器                                               | 安装时的行为                                 |
| -------------------------------------------------- | -------------------------------------------- |
| Windows 11                                         | 运行时已在系统里，安装器跳过下载             |
| Windows 10，已有运行时                             | 跳过下载                                     |
| Windows 10，没有运行时，能联网                     | 下载并安装，通常成功                         |
| Windows 10，没有运行时，离线/微软域名被挡/精简系统 | 下载失败，安装可能中断，或者装完后程序起不来 |

中国用户的 Win10 常见来源是精简版、Ghost 版、LTSC、长期关闭 Windows Update 的机器。这些机器上 WebView2 缺失或版本过旧的概率明显高于 Win11。

### 4.3 隐藏窗口放大了所有启动失败

`tauri.conf.json:22` 把窗口设成 `visible: false`。`src/main.tsx:57` 在 React 渲染之后调用 `showAppWindow()`，后端 `show_main_window` 才把窗口显示出来。

这条链上任何一环断了，用户都只看到“双击没反应”：

| 断点                          | 用户看到                             | 现有日志                            |
| ----------------------------- | ------------------------------------ | ----------------------------------- |
| WebView2 运行时缺失           | 什么都没有，或者一个系统错误框       | 没有，`build()` 直接失败            |
| WebView2 版本低于前端构建目标 | 窗口不出现                           | 没有，脚本解析失败发生在 webview 内 |
| JS bundle 加载失败            | 窗口不出现                           | 没有                                |
| React 首次渲染抛错            | 窗口不出现，ErrorBoundary 也来不及挂 | 没有                                |
| GPU/驱动导致 webview 黑屏     | 窗口可能出现，也可能不出现           | 没有                                |
| 单实例把新进程转交给旧进程    | 旧窗口出现，新进程退出               | 只有两行启动日志，没有“转交”        |

`vite.config.ts:52` 把前端构建目标设成 `chrome105`。这表示前端只保证在 Chromium 105 及以上能跑。WebView2 低于 105 时，语法或 API 可能直接不可用。这个版本号应当成为启动检查的一条线。

### 4.4 其他可能

| 可能                    | 证据                                                                    | 如何确认                                       |
| ----------------------- | ----------------------------------------------------------------------- | ---------------------------------------------- |
| Windows 10 低于 19045   | `windows.rs:12` 的 `MIN_OK_BUILD = 19045`，注释说这是 League 的最低版本 | `winver`，或新日志里的 `os build`              |
| 32 位 Windows           | README 写“Windows 10 / 11 64 位”，安装包是 x64                          | 设置 → 系统 → 关于 → 系统类型                  |
| GPU/驱动                | 旧 Win10 机器常见旧驱动，WebView2 黑屏                                  | 新日志的 GPU 行，`--safe-mode` 试一次          |
| Patcher 提权/UAC        | `patcher.rs:98` 用 `runas` 启动 loader，用户取消 UAC 就没有 patcher     | patcher 日志里的 `ShellExecuteExW` 错误码 1223 |
| 杀毒软件拦截            | loader 是注入器，Defender/国产杀软可能直接删                            | 日志里的 `loader missing`、`access denied`     |
| .NET Framework 4.8 缺失 | `resources/pengu` 是 WPF 程序，目标框架 4.7.2/4.6.2，国服自动换肤要用   | `pengu::prepare` 的错误，诊断页的 .NET 检查    |

这些都不是“Win11 才能跑”的代码问题，而是环境问题。日志要把它们区分开。

### 4.5 优先级

1. WebView2 缺失或过旧。
2. 前端启动失败但窗口被隐藏。
3. Windows 10 版本过低或 32 位。
4. GPU/驱动黑屏。
5. Patcher 提权、杀软、.NET Framework 运行时。

### 4.6 现在就能让朋友做的检查

不需要改代码，先拿到这几项：

```bat
winver
```

看版本号。19045 以下就是不受支持。

```bat
reg query "HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" /v pv
reg query "HKCU\Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" /v pv
```

任何一条返回版本号即可。都没有就是 WebView2 没装。

```bat
dir "%APPDATA%\com.vinciskin.app\logs"
```

如果目录不存在，失败发生在日志初始化之前。如果存在，打开最新的文件，看最后一行停在哪：

| 最后一行             | 说明                                 |
| -------------------- | ------------------------------------ |
| `Starting VinciSkin` | 日志初始化后立刻失败                 |
| `Log directory`      | 失败在 Tauri 构建或 WebView2 创建    |
| `Loaded settings`    | 失败在 setup 或窗口显示              |
| 没有日志             | 失败在 `main` 之前，或安装器没装成功 |

再确认两件事：

- 任务管理器里有没有 `vinciskin.exe`，有没有 `msedgewebview2.exe`。
- 安装时有没有要求联网、有没有出现 WebView2 相关错误。

### 4.7 “我的系统是 Win11 吗”

是。这台机器是 Windows 11 企业版 25H2，版本号 `10.0.26200`，WebView2 `158.0.4370.33`。

注册表 `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion` 的 `ProductName` 写的是 `Windows 10 Enterprise`。这是 Windows 11 上的已知遗留字段，微软没有改这个字符串。`GetVersionExW` 也会返回 `10.0`。判断 Windows 11 要看 build 号：`>= 22000` 就是 Windows 11。

这也说明 `windows.rs:33` 现在显示 `Windows 10.0.26200` 是错的，应当显示 `Windows 11 (26200)`。

## 5. 设计总览

四层，按启动顺序：

```
进程启动
  |
  +-- 1. boot.log          不依赖 tracing、不依赖 Tauri、每阶段一行
  |
  +-- 2. 运行日志           tracing 文件日志、run/session/game id、脱敏写入
  |
  +-- 3. 环境事实           OS、WebView2、GPU、权限、安装器事实
  |
  +-- 4. 诊断包             zip：日志 + 事实 + 设置 + mod + incident
```

前两层解决“有没有记录”，第三层解决“记录里有没有关键事实”，第四层解决“用户怎么把记录发出来”。

## 6. 启动日志

### 6.1 文件

`%APPDATA%\com.vinciskin.app\logs\boot.log`，追加写，单文件上限 256 KB。超过上限时保留最后 128 KB，开头写一行 `--- trimmed ---`。

选择追加而不是覆盖，是因为用户经常连点两次。第一次失败、第二次也失败时，两次的记录都要在。

### 6.2 格式

一行一个阶段，UTC 偏移的本地时间，UTF-8，LF：

```
2026-11-24T20:29:25.001+08:00 run=4be1c07a stage=process-start pid=1234 version=1.23.0
2026-11-24T20:29:25.010+08:00 run=4be1c07a stage=logging-ready dir="%APPDATA%\com.vinciskin.app\logs"
2026-11-24T20:29:25.020+08:00 run=4be1c07a stage=tauri-build-start
2026-11-24T20:29:26.500+08:00 run=4be1c07a stage=webview-detected version=158.0.4370.33
2026-11-24T20:29:26.600+08:00 run=4be1c07a stage=setup-done
2026-11-24T20:29:27.000+08:00 run=4be1c07a stage=ui-ready
```

### 6.3 阶段表

| `stage`                     | 何时写                         | 失败时说明               |
| --------------------------- | ------------------------------ | ------------------------ |
| `process-start`             | `main` 第一行                  | 连这行都没有，进程没起来 |
| `logging-ready`             | file writer 建好               | 日志目录不可写           |
| `webview-detected`          | 读到 WebView2 版本，或确认缺失 | 运行时缺失/版本过低      |
| `tauri-build-start`         | 调用 `Builder::build()` 前     |                          |
| `tauri-build-failed`        | `build()` 返回 Err             | 窗口或 webview 创建失败  |
| `setup-done`                | `setup::run` 返回 Ok           | 初始化失败               |
| `page-load-start`           | `on_page_load(Started)`        | webview 开始加载         |
| `page-load-finished`        | `on_page_load(Finished)`       | 页面加载完成             |
| `ui-ready`                  | 前端调用新命令 `ui_ready`      | 前端脚本执行成功         |
| `ui-timeout`                | 看门狗 30 秒没等到 `ui_ready`  | 前端没起来               |
| `single-instance-forwarded` | 单实例插件转发给旧进程         | 新进程正常退出           |
| `exit`                      | `RunEvent::Exit`               | 正常退出                 |

启动时读上一段的最后几行。如果上一段有 `process-start`、没有 `ui-ready`，写一行：

```
stage=previous-startup-incomplete last-stage=tauri-build-start
```

这行会进入诊断包。

### 6.4 写入方式

`boot.log` 不经过 `tracing`，用 `std::fs::OpenOptions` 同步追加，每次写完 `flush`。原因是 `tracing_appender::non_blocking` 在进程崩溃时可能丢缓冲区。

`boot.log` 的写入器要能接受一个 `&str`，不依赖任何全局初始化。`main` 的第一行就创建 `run` id 和写入器。

## 7. 运行日志

### 7.1 目录与文件

| 文件                       | 用途                            | 保留              |
| -------------------------- | ------------------------------- | ----------------- |
| `vinciskin.YYYY-MM-DD.log` | 运行日志，按天                  | 7 天              |
| `boot.log`                 | 启动阶段                        | 7 天，256 KB 上限 |
| `panic.json`               | 上一次 panic 的标记             | 读到后删除        |
| `loader-<pid>.log`         | patcher loader 的输出，后续阶段 | 7 天              |

`default_log_dir()` 保持 `%APPDATA%\com.vinciskin.app\logs`，没有 `APPDATA` 时回退 `%TEMP%\vinciskin`。

### 7.2 级别与过滤

默认过滤改为：

```
vinciskin=debug,ritoclient=debug,ritoclient_core=debug,ltk_overlay=info,tauri=info,warn
```

末尾的 `warn` 是默认级别。它保证 `vinciskin_core`、`ltk_*`、`reqwest` 等没有被点名的 crate 的 warn/error 能进文件。

`RUST_LOG` 仍然优先，方便开发和支持时临时放大。

`verbosePatcherLogging` 接到一个 `tracing_subscriber::reload::Handle` 上：

| 设置 | 效果                                           |
| ---- | ---------------------------------------------- |
| 关   | `ltk_overlay=info`，loader 输出只记 warn/error |
| 开   | `ltk_overlay=debug`，loader 输出全记           |

保存设置时调用 `handle.reload(...)`，不需要重启。

### 7.3 格式与 id

每行：

```
2026-11-24T20:29:25.114+08:00  INFO run=4be1c07a vinciskin: Starting VinciSkin v1.23.0
2026-11-24T20:30:02.663+08:00  WARN run=4be1c07a session=1 vinciskin_core::mods::health::sweep: ...
2026-11-24T20:31:12.000+08:00 DEBUG run=4be1c07a session=1 game=2026-11-24T20-30-02 vinciskin::patcher: ...
```

三个 id：

| id        | 何时设置           | 值                   |
| --------- | ------------------ | -------------------- |
| `run`     | 进程启动           | 8 位十六进制随机     |
| `session` | patcher 线程启动   | 本 run 内的计数      |
| `game`    | 第一次看到游戏进程 | incident 的时间戳 id |

id 放在 `INFO` 和 target 之间，修复 `tracing_subscriber::fmt::FormatEvent` 实现读取三个原子变量。不要用 span：这个应用的日志来自会话线程、库扫描线程、overlay 构建线程和命令线程，span 只属于进入它的线程。

`--report` 进程不写 run 行，它的日志行不带 id。

时间用 `chrono::Local` 的 RFC 3339 带偏移格式，毫秒精度。现在的日志是 UTC（`Z`），文件名是本地日期，用户对不上时间。

### 7.4 轮转、大小与保留

- 按天轮转保留。
- 单文件上限 5 MB。写入器累计字节，超过时把当前文件改名为 `vinciskin.YYYY-MM-DD.1.log`，新开一个。一天最多两个文件。
- 启动时清理 7 天前的 `vinciskin.*.log`、`boot.log`、`loader-*.log`。
- 总大小上限 50 MB，按修改时间从旧到新删。

`cleanup_old_logs` 按名字识别 `vinciskin.*.log`（含 `.1.log` 轮转文件）、`boot*.log`、`panic*.log`、`loader-*.log`，先按天数删，再按总量从旧到新删。

### 7.5 脱敏

日志写入器外面包一层 `RedactingWriter`，在 `write` 时按行替换。不要在调用点做，调用点会漏。

已知值优先，模式其次。

| 已知值                    | 替换为                                       |
| ------------------------- | -------------------------------------------- |
| `%USERPROFILE%` 的值      | `%USERPROFILE%`                              |
| `%APPDATA%` 的值          | `%APPDATA%`                                  |
| `%LOCALAPPDATA%` 的值     | `%LOCALAPPDATA%`                             |
| `settings.leaguePath`     | `%LEAGUE%`                                   |
| `settings.modStoragePath` | `%STORAGE%`                                  |
| `settings.workshopPath`   | `%WORKSHOP%`                                 |
| deep link 的完整 URL      | `scheme://host/path`，去掉 query 和 fragment |

同一个路径在日志里有两种写法：`C:\Users\<name>` 和 `C:\\Users\\<name>`（`{:?}` 的输出）。两种都要替换。

模式：

| 模式                                               | 替换为                 |
| -------------------------------------------------- | ---------------------- |
| `C:\Users\<name>`、`C:/Users/<name>`               | `%USERPROFILE%`        |
| `\\Users\\<name>`                                  | `%USERPROFILE%`        |
| `https://...?token=...`、`vinciskin://...?url=...` | 去掉 query 和 fragment |
| 邮箱                                               | `<email>`              |

不要记录：

- Riot 的 token、PUUID、召唤师名、账号 id
- deep link 的一次性 code
- 下载 URL 的签名参数
- mod 文件内容、游戏日志全文

`redact.rs` 是共用模块：`line()` 给写入器按行替换，`url()` 给 deep link 和网络日志用，`register*` 登记已知路径。设置加载和保存时登记 `%LEAGUE%`/`%STORAGE%`/`%WORKSHOP%`，启动时登记 `%USERPROFILE%`/`%APPDATA%`/`%LOCALAPPDATA%`。

脱敏不能保证 100%。它是让日志可以贴到公开 issue 的最低要求。诊断包在发送前给用户看，是第二道关。

### 7.6 记录点

| 区域    | 记什么                                                                               | 级别       |
| ------- | ------------------------------------------------------------------------------------ | ---------- |
| 启动    | 版本、exe 路径（脱敏）、OS build/edition/arch、WebView2 版本、是否管理员、安装器事实 | info       |
| 窗口    | 创建、`page-load-start`、`page-load-finished`、`ui-ready`、显示、尺寸                | info       |
| 设置    | 读取成功/失败；保存时非敏感字段的旧值→新值；路径只记占位符                           | info       |
| IPC     | 每个失败的 `IpcResult`：code、detail、`file:line`                                    | warn       |
| Patcher | 启动/停止、`runas` 结果、loader pid、状态变化、exit code、UAC 取消（1223）           | info/warn  |
| Overlay | 构建开始/结束、耗时、mod 数、archive 数、失败码                                      | info/error |
| 网络    | 请求 URL（去 query）、状态码、错误；不记 body                                        | debug/warn |
| 前端    | `window.onerror`、`unhandledrejection`、ErrorBoundary、路由错误                      | error      |
| Panic   | 消息、位置、线程、run id                                                             | error      |
| 退出    | `RunEvent::Exit`、退出码、是否还有 patcher 在跑                                      | info       |

IPC 失败的日志放在 `error.rs:213-220` 的 `From` 里，用 `#[track_caller]` 拿到命令的 `file:line`，只在这里记一次，命令内部不要重复记。

DLL/host 行如果后续接入，级别用行里的真实级别，不要统一写 INFO。当前本地 patcher 只检查进程是否存在（`patcher.rs:261-285`），没有读 loader 的输出。要定位注入失败，需要把 `cslol-inj.exe` 的 stdout/stderr 重定向到 `loader-<pid>.log`。

### 7.7 前端错误

新增一个命令：

```rust
#[tauri::command]
pub fn log_ui_error(level: String, message: String, stack: Option<String>, route: Option<String>) -> IpcResult<()>
```

- 只接受 `warn`/`error`。
- 限速：每分钟 20 条，超过丢弃并记一条 `ui-error-suppressed`。
- 消息和 stack 过脱敏再写文件。
- target 用 `ui`。

前端在 `main.tsx` 的 render 之前挂：

```ts
window.addEventListener("error", ...)
window.addEventListener("unhandledrejection", ...)
```

`ErrorBoundary` 的 `componentDidCatch`、TanStack Router 的 `defaultErrorComponent` 也调用同一个函数。

不要把 `console.log`/`console.warn` 全量转进日志，那会淹没有用信息。开发期的 `log_layer` 保留。

### 7.8 Panic

`main` 在初始化日志之后设置 hook：

```rust
std::panic::set_hook(Box::new(|info| {
    // 同步写 panic.json 和当前日志文件，再调用默认 hook
}));
```

`panic.json`：

```json
{
  "time": "2026-11-24T20:40:00+08:00",
  "version": "1.23.0",
  "run": "4be1c07a",
  "message": "....",
  "location": "src-tauri/src/main.rs:237"
}
```

下一次启动读到它：

- 写一行 `previous-panic` 到 `boot.log`。
- 写一条 `tracing::error!`。
- 在前端显示一条会话栏提示 `上次运行崩溃了`，带“获取帮助”。
- 读完删除。

panic hook 必须同步写文件，不能依赖 `non_blocking` 的 guard 在 panic 时析构。

### 7.9 看门狗

`setup::run` 里起一个线程，30 秒后检查 `AtomicBool`：

| 检查                                | 说明                         |
| ----------------------------------- | ---------------------------- |
| `ui_ready` 已置位                   | 正常，什么都不做             |
| 未置位，`page-load-finished` 也没有 | webview 没起来，或页面没加载 |
| 未置位，`page-load-finished` 有     | 页面加载了，脚本没跑完       |

超时后：

1. 写 `boot.log` 的 `stage=ui-timeout`。
2. 写运行日志 `ERROR target=startup`。
3. 弹原生 `MessageBoxW`：`VinciSkin 没有完成启动。` + 最后阶段 + `是否生成诊断包？`（是/否）。
4. 选“是”就启动 `vinciskin.exe --report`，生成后打开所在文件夹。

不要只是 `window.show()`。webview 已崩时显示一个空白窗口没有意义，原生对话框才能说明原因。

## 8. 诊断包

### 8.1 内容

zip 名：`VinciSkin-report-YYYYMMDD-HHMMSS-<run>.zip`。

只有命名构建器写出的文件才会进 zip。没有任何“复制整个目录”的构建器。

| 文件                  | 内容                                                                           |
| --------------------- | ------------------------------------------------------------------------------ |
| `report.md`           | 给人读的摘要：版本、系统、WebView2、问题类别、用户写的说明、findings、日志尾部 |
| `manifest.json`       | schema 版本、脱敏规则版本、每条规则的替换次数、文件列表和大小                  |
| `log/vinciskin.log`   | 当前 run 和前两个 run，最多 2 MB，新的优先                                     |
| `log/boot.log`        | 启动阶段                                                                       |
| `log/panic.json`      | 上一次 panic，如果有                                                           |
| `system.json`         | 诊断检查结果 + 环境事实 + 路径特征（长度、盘符、是否非 ASCII）                 |
| `settings.json`       | 白名单字段，路径只留占位符                                                     |
| `library.json`        | 当前 profile 的 mod：id、名字、版本、启用状态、优先级、健康结论、archive 数    |
| `patcher.json`        | loader 版本/摘要、overlay 路径占位符、最近一次状态、退出码                     |
| `incidents/*.json`    | 最近 5 个 incident                                                             |
| `incidents/*.md`      | 对应 incident 的 report text                                                   |
| `startup.json`        | 仅 `--report`：WebView2 版本或读取错误、最后阶段、启动错误                     |
| `game-log/r3dlog.txt` | 可选，默认不勾选；只有 incident 存在且用户勾选时加入                           |

不放进 zip：

- `settings.json` 原文（含路径、内置 mod 选择）
- Riot token、PUUID、账号名
- `Logs/GameCrashes` 下的内容
- 图片、截图、mod 文件、archive
- 完整游戏日志（除非用户勾选）

### 8.2 构建器

放在 `crates/vinciskin-core/src/support_report/`，不依赖 Tauri，`--report` 可以直接用。

```rust
pub struct ReportInputs {
    pub app_version: String,
    pub os: OsFacts,
    pub webview: WebviewFacts,
    pub diagnostics: Vec<Check>,
    pub settings: SettingsSnapshot,
    pub library: Vec<ModRow>,
    pub patcher: PatcherFacts,
    pub incidents: Vec<Incident>,
    pub logs: LogBundle,
    pub problem: ProblemKind,
    pub notes: String,
    pub startup: Option<StartupFacts>,
}

pub fn build(inputs: &ReportInputs) -> Result<Report, ReportError>;
```

每个文件一个函数：`report_md`、`manifest`、`system`、`settings`、`library`、`patcher`、`incidents`、`startup`。`build` 把它们收集成 `Vec<ReportFile>`，再由 `src-tauri` 写成 zip。

所有文本在写入前过同一个 redactor。`notes` 也过，用户可能在里面写账号。

### 8.3 生成流程

应用内：

```
点击“获取帮助”
  -> 选择问题类别（Mod 不显示 / 游戏崩溃 / 软件问题）
  -> 可选：选择 mod、填写说明
  -> 后端构建 report（不落盘）
  -> 预览对话框：文件列表 + 每个文件的文本
  -> 用户勾选/取消游戏日志
  -> 保存 zip / 复制摘要
```

构建两次：打开对话框时一次，选项变化时一次。保存时写的是预览用的那份，保证 zip 和预览一致。

### 8.4 大小与脱敏

- 目标 500 KB，上限 2 MB。
- 超限时先截断日志到最近 2 MB，再丢可选文件。
- 每个文件过 redactor，`manifest.json` 记每条规则的替换次数。
- 第一行写明 app 标识、版本、是否为本项目签名的构建。

### 8.5 预览与保存

对话框用现有的 `Dialog.Shell`、`SectionCard`、`Button`，文案进 `messages/en/`（如果以后加中文，进对应 catalog）。

文件列表显示：`概览`、`日志`、`系统检查`、`设置`、`mod 列表`、`事件`、`游戏日志（可选）`，各带大小。点一行看文本。

按钮：

- `保存诊断包`：保存对话框，默认 Downloads。
- `复制摘要`：给 QQ/Discord 的短文本，< 1500 字，含版本、类别、findings、incident token、zip 名。
- `打开 GitHub issue`：预填版本和 token。

### 8.6 无窗口 `--report`

`main` 的第一件事：

```rust
if std::env::args().any(|a| a == "--report") {
    return report_mode::run();
}
```

`report_mode::run()`：

1. 读日志目录、`boot.log`、`panic.json`。
2. 读 WebView2 版本。
3. 跑 `run_all` 的诊断（不需要 Tauri state 的检查用默认值）。
4. 构建 zip。
5. `rfd::FileDialog::new().set_file_name(...).save_file()`。
6. `explorer.exe /select,<zip>`。
7. `MessageBoxW` 显示保存路径。

不初始化 Tauri、不创建 webview、不写 run 行、不删除 `panic.json`、不启动单实例插件。

启动失败时，父进程用 `--report --auto-save --out <zip> --error "<message>"` 启动子进程。`--auto-save` 直接写 `--out` 指定的文件，不弹保存框。子进程的对话框独立于父进程，父进程退出后仍然在。

### 8.7 启动失败的原生对话框

`main` 不再 `.expect`：

```rust
match builder.build(tauri::generate_context!()) {
    Ok(app) => app.run(setup::handle_run_event),
    Err(error) => {
        boot::stage("tauri-build-failed", &error);
        startup_failure::handle(&error);
    }
}
```

`startup_failure::handle`：

1. 写 `boot.log` 和运行日志。
2. 在 Downloads 下算出 `--out` 路径，启动 `vinciskin.exe --report --auto-save --out <zip> --error "<error>"`。
3. `MessageBoxW`：`VinciSkin 没能启动。` + 原因 + `诊断包已保存到 <zip>。请把这个文件发给作者。`
4. 正常退出，不 panic。

`setup::run` 返回 Err 也走同一条路。

### 8.8 安全模式

`vinciskin.exe --safe-mode` 在 Tauri 启动前设置：

```rust
std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", "--disable-gpu");
```

用于旧显卡、虚拟机和远程桌面上的黑屏。启动日志记一行 `stage=safe-mode`，诊断包的 `system.json` 记下 `webview.arguments`。

启动失败的原生对话框可以加一个“是否用安全模式重试”的选择。MessageBox 的“是”按钮启动 `vinciskin.exe --safe-mode`，“否”只生成诊断包。

## 9. 安装器

### 9.1 运行时检查

在 `installer-hooks.nsh` 的 `NSIS_HOOK_PREINSTALL` 加：

```nsis
${IfNot} ${RunningX64}
  MessageBox MB_ICONSTOP "VinciSkin 需要 64 位 Windows 10 或 Windows 11。"
  Abort
${EndIf}
```

版本号 19045 的检查放在应用启动时做，安装器只拦 32 位。

### 9.2 WebView2 安装模式

四个选择：

| 模式                           | 安装包大小   | 安装时联网         | 适用                     |
| ------------------------------ | ------------ | ------------------ | ------------------------ |
| `downloadBootstrapper`（当前） | 最小         | 需要               | 网络稳定                 |
| `embedBootstrapper`            | +2 MB        | 需要（下载运行时） | 网络稳定                 |
| `offlineInstaller`             | +约 150 MB   | 不需要             | 中国用户、离线、精简系统 |
| `fixedRuntime`                 | +250 MB 以上 | 不需要             | 需要固定版本             |

建议改成 `offlineInstaller`。中国用户的网络环境、精简版 Win10 和被屏蔽的微软 CDN 让“安装时下载”成为最常见的失败点。安装包变大是一次性成本，装不上是每次都要处理的支持成本。

如果安装包大小不能接受，保留 `downloadBootstrapper`，但启动时必须检查 WebView2，并在缺失时给出原生对话框和下载地址，而不是静默退出。

### 9.3 开始菜单入口

安装器创建：

```
VinciSkin - 获取帮助.lnk  ->  %INSTDIR%\vinciskin.exe --report
```

每次安装和更新都写，因为旧版本没有这个入口。卸载时删除。

### 9.4 安装事实

安装器把下面几项写到 `%ProgramData%\VinciSkin\install.json`：

```json
{
  "installed_at": "2026-11-24T20:00:00+08:00",
  "os_build": "19045",
  "arch": "x64",
  "webview2_before_install": "missing",
  "webview2_after_install": "120.0.2210.91",
  "installer_mode": "offlineInstaller",
  "install_dir": "%PROGRAMFILES%\\VinciSkin"
}
```

`--report` 读它。软件打不开时，这个文件仍然能说明安装阶段发生了什么。

## 10. 设置页与诊断页

### 10.1 设置页

`AboutSection` 的 `Open Log File` 改成两个动作：

- `打开日志文件夹`：打开 `%APPDATA%\com.vinciskin.app\logs`。
- `生成诊断包`：打开 8.5 的对话框。

`Patching` 区域的 `Verbose patcher logging` 保留，接到 reload handle。如果短期不接线，就删掉这个开关，不要留一个看起来有效、实际无效的设置。

### 10.2 诊断页

新增检查：

| id                                | 检查                                    | 失败级别            |
| --------------------------------- | --------------------------------------- | ------------------- |
| `windows.webview2`                | 运行时是否存在、版本是否 >= 105         | 缺失 Bad，过旧 Warn |
| `windows.version`（改造现有检查） | 用 build 号显示 `Windows 10/11 (build)` | 低于 19045 Bad      |
| `system.arch`                     | 是否 64 位                              | 32 位 Bad           |
| `system.dotnet_framework`         | .NET Framework 4.8（Pengu 用）          | 缺失 Warn           |
| `system.gpu`                      | 适配器名、驱动版本                      | 仅 Info             |

诊断页顶部加两个动作：`生成诊断包`、`复制摘要`。

`windows.rs:33` 的显示改成：

```rust
fn windows_name(build: u32) -> String {
    if build >= 22000 {
        format!("Windows 11 ({build})")
    } else {
        format!("Windows 10 ({build})")
    }
}
```

### 10.3 错误提示

`IpcResult` 失败的 toast 加 `详情`，显示 code 和一行日志引用 `file:line`；未预期的错误再加 `获取帮助`，打开 8.5 的对话框，带上命令名和 code。

## 11. 代码结构

```
src-tauri/src/
  logging/
    mod.rs            初始化、LoggingGuards、reload handle
    boot.rs           boot.log、阶段、run id
    format.rs         自定义 FormatEvent（时间、run/session/game）
    redact_writer.rs  脱敏写入器
    panic.rs          panic hook、panic.json
    watchdog.rs       ui-ready 看门狗
    webview.rs        WebView2 版本读取与最低版本
  startup_failure.rs  原生对话框、--report 子进程
  report_mode.rs      --report 入口
  support_report.rs   Tauri 侧采集输入、调 core、写 zip
crates/vinciskin-core/src/
  support_report/
    mod.rs            ReportInputs、Report、build
    report_md.rs
    manifest.rs
    system.rs
    settings.rs
    library.rs
    patcher.rs
    incidents.rs
    zip.rs
```

依赖：

- `rfd`：原生存档对话框。`Cargo.lock` 里已有传递依赖，加为直接依赖。
- `zip`：已有。
- `chrono`：已有。
- `windows-sys`：已有，`MessageBoxW` 用 `Win32_UI_WindowsAndMessaging`。

## 12. 测试

| 测试                                                  | 位置                                                     |
| ----------------------------------------------------- | -------------------------------------------------------- |
| 脱敏：两种斜杠、`%USERPROFILE%`、URL query、邮箱      | `src-tauri/src/redact/tests.rs`                          |
| 脱敏写入器：一行分两次写、5 MB 轮转、50 MB 总量       | `src-tauri/src/logging/tests.rs`                         |
| 脱敏 canary：构造带用户名的输入，构建 zip，解压搜不到 | `crates/vinciskin-core/tests/redaction_canary.rs`        |
| boot.log 阶段顺序、trim、上一段未完成                 | `logging/boot.rs` 单测                                   |
| 过滤默认级别：未命名 crate 的 warn 能进文件           | `logging/mod.rs` 单测                                    |
| 报告构建：每个文件都出现、大小上限、路径占位符        | `support_report/tests.rs`                                |
| panic.json 写入、下次启动读取并删除                   | `logging/panic.rs` 单测                                  |
| 看门狗状态机                                          | `logging/watchdog.rs` 单测                               |
| WebView2 版本解析（存在/缺失/过旧）                   | `crates/vinciskin-core/src/diagnostics/windows/tests.rs` |

手工矩阵：

- Win10 22H2，正常。
- Win10 22H2，卸载 WebView2。
- Win10 1809 / LTSC 2019。
- 32 位 Win10 虚拟机。
- 用户名含中文或空格。
- 离线安装。
- 管理员/非管理员各一次。
- 国服路径含中文。

## 13. 分期

| 阶段 | 内容                                                                                                                               | 产出                 |
| ---- | ---------------------------------------------------------------------------------------------------------------------------------- | -------------------- |
| P0   | boot.log、阶段、panic hook、过滤默认级别、WebView2/OS 环境行、loader 输出与注入状态、看门狗、设置页的“打开日志文件夹/复制系统信息” | 能判断“卡在哪一步”   |
| P1   | 脱敏写入器、run/session/game id、5 MB 轮转、诊断页新增检查                                                                         | 日志可安全发给维护者 |
| P2   | 诊断包 zip、预览对话框、`--report`、启动失败原生对话框、开始菜单入口                                                               | 用户一次发全         |
| P3   | 安装器 offlineInstaller、install.json、错误 toast 的详情/获取帮助                                                                  | 减少 Win10 安装失败  |
| P4   | 上传和 report code（可选）                                                                                                         | 不用手动发文件       |

P0 一周内可完成。P0 完成后，即使不做 P2，也能让朋友把 `boot.log` 和日志目录发过来。

## 14. 决策与待定

| 问题                    | 选项              | 建议                            |
| ----------------------- | ----------------- | ------------------------------- |
| WebView2 安装模式       | 继续下载 / 离线包 | 离线包                          |
| 默认日志级别            | info / debug      | `vinciskin=debug` + `warn` 默认 |
| `verbosePatcherLogging` | 接线 / 删除       | 接线                            |
| 诊断包是否含 mod 名     | 含 / 只含哈希     | 含名字，用户发送前能预览        |
| 是否自动上传            | 是 / 否           | 否，保留手动保存                |
| 游戏日志默认            | 含 / 不含         | 不含，用户勾选                  |

## 15. 验收清单

- [ ] WebView2 缺失时，双击出现原生对话框，`--report` 仍能生成 zip。
- [ ] 前端启动失败时，`boot.log` 留下 `page-load-finished` 和 `ui-timeout`。
- [ ] 诊断页显示 `Windows 11 (26200)`。
- [ ] zip 里搜不到 Windows 用户名、deep link query、token。
- [ ] 预览的文件和 zip 里的文件逐字节一致。
- [ ] 日志 5 MB 轮转，7 天清理。
- [ ] 单实例转交写 `single-instance-forwarded`。
- [ ] 32 位系统在安装器阶段被拦住，提示可读。
- [ ] `--report` 在没有 webview 的机器上也能跑完。

## 16. 换肤链路的诊断

“软件启动了，皮肤没变”是另一条链路，和“打不开”不同。链路是：

```
选皮肤 -> overlay_apply 写 <storage>/overlay/DATA/FINAL/<Champion>.wad.client
       -> 启动 vinciskin-inj.exe（管理员），参数是 dll 路径和 overlay 目录
       -> 等 League of Legends.exe 的窗口能响应消息，取它的线程 id
       -> cslol-dll-legacy.dll 用这个线程注入 League of Legends.exe
       -> DLL hook CreateFileA，把游戏的 WAD 读取重定向到 overlay
       -> 游戏读 champion archive，看到皮肤
```

宿主是自己写的（ADR-0060）：`cslol-inj.exe` 按窗口标题在整台机器上找，可能挂到不处理消息的线程上，Windows 10 上就表现为 32 秒超时或者 `OpenThread … Win32=87`。

每一步都可能失败。现在的界面只报“注入中/已注入/已应用”，不报“游戏有没有真的读到 overlay”。

### 16.1 现在的假成功

| 界面                                      | 实际含义                                                                         | 用户以为                 |
| ----------------------------------------- | -------------------------------------------------------------------------------- | ------------------------ |
| `Injected · waiting for the game to exit` | `PatcherState::status` 看到 `League of Legends.exe` 存在（`patcher.rs:267-277`） | DLL 已经挂上，皮肤生效   |
| `Champ-select follow · applied X`         | `overlay_apply` 写完了文件（`cn_auto.rs:243-262`）                               | 游戏里已经换肤           |
| `Applied: X. Launch the game to see it.`  | `apply_cn_static_swap` 写了 overlay                                              | 直接开游戏就行，不用注入 |

`patcher.rs` 只看进程存在，不看 DLL 的 `Init done!`。cslol 的 DLL 会输出 `Failed to hook`、`Win32=5`、`Timeout out while waiting for init`、`Failed to configure` 这些错误，但宿主是用 `SW_HIDE` 启动的，stdout/stderr 没有被读取，所以这些错误在应用里不存在。

### 16.2 失败点

| 阶段       | 失败                                                        | 现在的证据                                                                                  | 要补的记录                                                 |
| ---------- | ----------------------------------------------------------- | ------------------------------------------------------------------------------------------- | ---------------------------------------------------------- |
| 选皮肤     | 没有 archive、skin 不在安装里、写文件失败                   | toast 有错误                                                                                | `overlay_apply` 的完整错误和路径（脱敏）                   |
| 启动宿主   | 没有 overlay 目录、游戏已运行、另一个 loader 在跑、UAC 取消 | `start_cn_patcher` 返回错误，但 `CnCatalog.launch` 的 `catch` 吞掉（`CnCatalog.tsx:62-68`） | 每次 `startPatcher` 的结果、错误码 1223                    |
| 宿主启动   | 参数路径含空格被拆开                                        | 无                                                                                          | 启动命令行（脱敏）和 loader 的 `Failed to set prefix`      |
| 注入       | 游戏高完整性、DLL 没加载、hook 失败、超时                   | 无                                                                                          | `loader-<pid>.log`，解析关键行                             |
| 游戏读取   | 游戏在 overlay 写入前已打开 WAD，或从另一个安装启动         | 无                                                                                          | 游戏 PID、启动时间、overlay 文件 mtime、配置的 League 路径 |
| 客户端跟随 | Pengu IFEO 注册失败、客户端没重启、.NET Framework 4.8 缺失  | 只有 `client loader not prepared` 的 warn（`cn_auto.rs:114-116`）                           | 把 bridge 失败写进 `AutoStatus.error`，并在页面显示        |

### 16.3 设计补充

1. **捕获 loader 输出。** 把 `spawn_runas` 改成通过 `cmd.exe /c "\"cslol-inj.exe\" \"<overlay>\" > \"loader-<pid>.log\" 2>&1"` 启动，或者用管道继承。解析 `[LoaderDiag]`、`Init done!`、`Failed to hook`、`Win32=5`、`Timeout`。
2. **状态以 DLL 为准。** `injected` 只在 loader 报告 `Init done!` 或 DLL ack 后设置。仅看到游戏进程时是 `attached`，不是 `injected`。
3. **等宿主就绪再启动游戏。** `launch` 先等 loader 的 `Prepared` 或 `Init done!`，超时提示，不静默继续。
4. **不要吞错。** `CnCatalog.launch` 只忽略“已经有一个宿主在跑”，其他错误必须 toast 并停止启动。
5. **给 overlay 路径加引号。** `patcher.rs:340` 的 `spawn_runas(..., &overlay.to_string_lossy())` 传的是裸参数。路径含空格时（例如 Windows 用户名里有空格，或自定义存储路径有空格），loader 的 `CommandLineToArgvW` 会把参数拆开。
6. **overlay 目录先建。** `start_cn_patcher` 在 overlay 不存在时创建空目录，或者允许空 overlay 启动。否则“先开跟随、再点启动”会在第一次游戏前失败。
7. **客户端跟随的错误上屏。** `pengu::prepare` 失败、另一个 loader 已注册、客户端未重启、.NET Framework 4.8 缺失，都要显示在 `CnSessionControl`，不要只写 warn。
8. **换肤报告。** 诊断包加 `skin.json`：选中的英雄/皮肤、overlay 文件与 hash、宿主启动时间/退出码、loader 日志尾部、游戏 PID、客户端跟随状态、配置的 League 路径。
9. **应用结果分两种情况。** `apply_cn_static_swap` 成功只代表 overlay 写了。toast 和状态栏分两种：宿主在跑，显示“已写入本地补丁，进游戏生效”；宿主没跑，显示“已写入本地补丁，但注入未启动，先启动注入再进游戏”。不要统一说 `Applied: X. Launch the game to see it.`。
10. **游戏已经在运行时提示。** 游戏进程存在时 apply 仍会写 overlay，但这一局读不到。页面应提示“游戏已经在运行，这一局不会生效；下一局前先启动注入”，而不是只报成功。

这一节的目标不是把换肤修好，而是让下一次“启动了但没效果”的报告能直接回答：是没写 overlay、没启动宿主、宿主没注入，还是游戏没读到。

## 附录 A：日志行示例

启动：

```
2026-11-24T20:29:25.001+08:00 run=4be1c07a stage=process-start pid=1234 version=1.23.0
2026-11-24T20:29:25.010+08:00 run=4be1c07a stage=logging-ready dir="%APPDATA%\com.vinciskin.app\logs"
2026-11-24T20:29:25.020+08:00 run=4be1c07a stage=webview-detected version=158.0.4370.33
2026-11-24T20:29:25.611+08:00 run=4be1c07a stage=page-load-start
2026-11-24T20:29:26.204+08:00 run=4be1c07a stage=page-load-finished
2026-11-24T20:29:26.512+08:00 run=4be1c07a stage=ui-ready
```

运行：

```
2026-11-24T20:29:25.114+08:00  INFO run=4be1c07a vinciskin: Starting VinciSkin v1.23.0
2026-11-24T20:29:25.614+08:00  INFO run=4be1c07a vinciskin::state: Loaded settings from "%APPDATA%\com.vinciskin.app\settings.json"
2026-11-24T20:30:02.663+08:00  WARN run=4be1c07a session=1 vinciskin_core::mods::health::sweep: Could not sync the hashtables before the library sweep: ...
2026-11-24T20:30:04.347+08:00  INFO run=4be1c07a session=1 vinciskin::patcher: starting the injection loader (elevated) loader="%INSTALL%\cslol-inj.exe" overlay="%STORAGE%\overlay"
2026-11-24T20:31:12.000+08:00 ERROR run=4be1c07a session=1 game=2026-11-24T20-30-02 ui: Uncaught TypeError: ...
```

启动失败：

```
2026-11-24T20:40:00.001+08:00 run=8c21aa3f stage=process-start pid=4321 version=1.23.0
2026-11-24T20:40:00.002+08:00 run=8c21aa3f stage=webview-detected missing
2026-11-24T20:40:00.010+08:00 run=8c21aa3f stage=tauri-build-failed error="WebView2 runtime not found"
```

## 附录 B：给朋友的检查清单

按顺序问，问题会自己收敛：

1. `winver` 的版本号是多少。低于 19045 就是系统不支持。
2. 系统是 64 位还是 32 位。32 位装不上。
3. 双击后任务管理器里有没有 `vinciskin.exe`，有没有 `msedgewebview2.exe`。
4. `%APPDATA%\com.vinciskin.app\logs` 在不在。在的话，最新日志的最后一行是什么。
5. 上面两条 `reg query` 有没有返回版本号。没有就是 WebView2 缺失。
6. 安装时有没有要求联网，有没有报 WebView2 错误。

这六项拿到之前，不要猜“Win10 不兼容”。代码里没有 Win11 分支，兼容性问题会落在上面某一项上。
