# EyeFlow v0.2 优化报告

> 日期：2026-09-08 · 方法：6 个并发子 Agent（1 源码审计 + 4 网络权威调研 + 1 发布迁移）→ 决策记录（ADR）→ 重写 → 实测验收
> 新仓库：`E:\Projects\MyGitHub\eyeflow`（原目录 `E:\Workspaces\Hanako\eyeflow` 保持原样作为备份）

## 1. 结论

v0.1 “效果不佳、不实用”不是参数没调好，而是**产品承诺的核心链路根本没有实现**：到点后发生的全部事情是一声 0.5 秒的合成音——“系统通知”是日志占位、没有倒计时、没有完成 / 跳过；更糟的是，心流和游戏两个状态会把到点的提醒**静默吞掉并重置计时**，重度打字者和玩家几乎永远收不到提醒；只用键盘判断“离开”，鼠标用户 3 分钟就被判离开。

v0.2 把它重写成一个真正“会看情况”的护眼提醒：**预告 → 休息 → 结算**闭环、四态上下文只顺延不取消、AOA 20-20-20 双层休息、坚持统计、全局热键、开机自启开关、旧配置自动迁移；空闲常驻 16 MB，27 个单元测试，CI + tag 发布流水线，NSIS 安装包已本地验证生成。

## 2. v0.1 根因（来自 docs/research/01-source-audit.md）

| 严重度 | 问题 | 证据 | v0.2 处置 |
|---|---|---|---|
| P0 | 心流状态到点提醒被吞并重置计时 | `main.rs:239` 空分支 + `main.rs:157` `reminder.reset()` | 顺延到停歇 8 秒后完整投递（`core.rs` Flow 分支） |
| P0 | 游戏状态彻底静默，连承诺的“仅提示音”都没有 | `main.rs:152` `Gaming \| Away => {}` | 到点立即播提示音，退出全屏后补发预告 |
| P0 | “系统通知”是 `log::info!` 占位；无倒计时 / 完成 / 跳过 | `tray.rs:112-116` | 预告浮窗 + 休息面板 + 统计结算 |
| P0 | Ctrl+Shift+E 从未注册 | 无 global-hotkey 依赖，`event.rs:22` 死代码 | global-hotkey 0.8，热键 = 立即休息 |
| P0 | 无音频设备直接 panic（panic=abort + 无窗口 → 静默崩溃循环） | `audio.rs:21` `expect("不可恢复")` | `Option<Sink>` 降级为静音并告警 |
| P0（隐藏） | 只用键盘判“离开”，鼠标用户 3 分钟即被判离开 | `state.rs:106-112` 基于 `last_activity`（仅键盘） | GetLastInputInfo（含鼠标） |
| P1 | 配置解析失败直接删除用户配置 | `config.rs:100-104` | 改名备份 `.bak-corrupt-*` 后重建 |
| P1 | 全屏检测只比主显示器尺寸，最大化窗口 + 自动隐藏任务栏会误判为游戏 | `detector.rs:31-37` | SHQueryUserNotificationState 为主 + 所在显示器矩形 & 无标题栏兜底 |
| P1 | 心流阈值 10 键/30 秒 = 0.33 键/秒，任何打字都算心流 | `config.rs:18` | 40 / 70 / 100 键/30 秒，过滤自动重复与修饰键 |
| P2 | 设置窗 `run_native` 阻塞主循环，开着设置时托盘 / 提醒全停 | `main.rs:193-196` | 核心逻辑 `Runtime::step` 在两种模式下都运行 |

## 3. 决策清单与依据（用户要求：按权威来源决策并注明）

| # | 决策 | 权威依据 | 落地 |
|---|---|---|---|
| D1 | 短休息默认 **15~25 分钟三角随机（峰值 20 分钟）、持续 30 秒**，取代 10~18 分钟 / 25 秒 | AOA 计算机视综合征指南（20-20-20 原文）https://www.aoa.org/healthy-eyes/eye-and-vision-conditions/computer-vision-syndrome ；20-20-20 唯一 RCT（Talens-Estarelles 2023）https://pubmed.ncbi.nlm.nih.gov/35963776/ ；自发微休息均值 27.4 s（Henning 1989）https://pubmed.ncbi.nlm.nih.gov/2806221/ | ADR-0001；`config.rs` 默认值 |
| D2 | 新增**长休息**：连续用屏 2 小时 → 15 分钟；不可延后，跳过后 10 分钟再提示 | AOA 同页“连续用电脑 2 小时后休息 15 分钟” | ADR-0001；`core.rs` `long_break_due` |
| D3 | **提醒只顺延、不消失**：心流顺延到停歇 8 秒；游戏 / 不可打扰只响声音、退出后补发 | 审计根因（P0-1/P0-2）；竞品最高热卸载原因是“全屏时弹窗抢焦点”（Stretchly #355，2019 至今）https://github.com/hovancik/stretchly/issues/355 | ADR-0002；`core.rs` tick |
| D4 | **默认温和**：预告（可延后 1 次 / 跳过 / 立即开始）→ 不抢焦点的休息面板 → 统计；全屏遮罩仅作“严格模式”默认关 | 全部阳性证据来自可忽略的非强制提醒软件（Leppe-Zamora 2025 Meta 分析 https://pubmed.ncbi.nlm.nih.gov/40514667/ ；Monsey 2003；Trujillo 2006）；“强制 vs 可跳过”无头对头研究；Stretchly 预告 + 限推迟一次 https://hovancik.net/stretchly/about/ ；LookAway “Starting in 3” https://lookaway.com/ ；Workrave 统计为留存核心 https://www.workrave.org/ | ADR-0003；`app.rs` 浮窗 / 面板；`stats.rs` |
| D5 | **不做蓝光过滤 / 色温**、不做“防近视 / 防眼损伤”文案 | Cochrane 2023（17 项 RCT）蓝光镜片对视疲劳可能无效 https://www.cochranelibrary.com/cdsr/doi/10.1002/14651858.CD013244.pub2/full ；AAO 不推荐蓝光眼镜 https://www.aao.org/eye-health/tips-prevention/should-you-worry-about-blue-light ；AAO 定位数字视疲劳为暂时性 https://www.aao.org/eye-health/tips-prevention/computer-usage | ADR-0003；`tips.rs` 文案 |
| D6 | 可打扰性以 **SHQueryUserNotificationState** 为主，前台窗口矩形 + 无 WS_CAPTION 兜底 | Microsoft Learn：自定义通知界面“必须”调用该 API https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shqueryusernotificationstate ；枚举仅 7 值 https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ne-shellapi-query_user_notification_state | ADR-0004；`detector.rs` |
| D7 | 浮窗**不抢焦点**：每次新建窗口 + `with_active(false)` + `WS_EX_NOACTIVATE`；不用 WinRT Toast | SetForegroundWindow 限制 https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow ；WS_EX_NOACTIVATE https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles ；桌面 Toast 需 AUMID + 开始菜单快捷方式 https://learn.microsoft.com/en-us/windows/win32/shell/quickstart-sending-desktop-toast ；winit `with_active` 仅首次显示生效（源码 `window_state.rs` `MARKER_ACTIVATE`） | ADR-0004/0006；`app.rs` `apply_noactivate` |
| D8 | 托盘约定：左键 / 双击 = 打开设置；菜单默认项在首、开关带勾选、Exit 在末；tooltip ≤128 字符 | Microsoft 通知区域 UX 指南 https://learn.microsoft.com/en-us/windows/win32/uxguide/winenv-notification ；NOTIFYICONDATA https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ns-shellapi-notifyicondataw | `tray.rs` |
| D9 | 开机自启 **HKCU\Run**，安装默认开、设置内可关、便携运行默认不写 | Run/RunOnce 键 https://learn.microsoft.com/en-us/windows/win32/setupapi/run-and-runonce-registry-keys ；任务管理器“启动应用”管控范围 https://learn.microsoft.com/en-us/windows/win32/w8cookbook/startup-apps ；停用 1 周获益消失（RCT） | ADR-0005；`autostart.rs`；`installer.nsi` |
| D10 | 热键 Ctrl+Shift+E 语义从“静音”改为“**立即休息**” | 竞品中“立即休息 / 现在开始”是被反复肯定的动作（LookAway、Stretchly）；温和默认下“静音”价值低 | `runtime.rs`；spec F10 |
| D11 | 技术栈：eframe/egui **0.36**（glow，非默认 wgpu）、tray-icon 0.24、global-hotkey 0.8、rodio 0.22（裁剪解码器）、toml 1、winresource；不引入 rdev / single-instance / notify-rust | docs/research/05：0.34 起 eframe 默认 wgpu 致体积内存膨胀 https://github.com/emilk/egui/blob/0.36.1/crates/eframe/Cargo.toml ；rdev 2023 停更 https://crates.io/crates/rdev ；single-instance 2021 停更 | `Cargo.toml` |
| D12 | UI 宿主**按需启动 eframe 会话**，空闲无 GL；否决“常驻 eframe” | 本机实测：空 eframe 窗口 163 MB 私有、常驻方案 203 MB；eframe `run_and_return` 支持反复进入 https://docs.rs/eframe/latest/eframe/struct.NativeOptions.html | ADR-0006（修订版）；`main.rs` / `runtime.rs` / `app.rs` |
| D13 | 仓库：MIT、提交 Cargo.lock、GitHub Actions tag 触发 release（NSIS + portable zip + sha256）、渠道 Releases → scoop → winget | choosealicense MIT https://choosealicense.com/licenses/mit/ ；Cargo Book “When in doubt, check Cargo.lock into VCS” https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html ；winget 提交流程 https://learn.microsoft.com/en-us/windows/package-manager/package/repository ；ripgrep release.yml 结构 https://github.com/BurntSushi/ripgrep/blob/master/.github/workflows/release.yml | `LICENSE`、`.github/workflows/*`、`installer.nsi` |
| D14 | 免打扰时段默认保留 00:00–08:00（全停），未实现“22 点后降级” | 科学报告建议夜间降级而非停止（AAO 睡前屏幕影响睡眠 https://www.aao.org/eye-health/tips-prevention/screen-use-kids ），但用户旧配置显示曾主动关闭免打扰；先尊重用户可配，降级逻辑列入 Roadmap | `config.rs` |

## 4. 做了什么

### 4.1 产品与体验
- 完整提醒链路：右下角预告浮窗（N 秒倒计时、现在开始 / 延后 5 分钟（限 1 次）/ 跳过）→ 居中休息面板（大倒计时、进度条、一条 AOA/AAO 贴士、[继续工作]=跳过、倒计时结束自动完成 + 结束音）→ 计入 `stats.toml`。
- 四态上下文：桌面 / 心流（顺延到停歇 8 秒）/ 游戏·不可打扰（仅声音 + 补发）/ 离开（计时暂停，进入离开即记一次自然休息，离开 ≥15 分钟清零长休息累计）。
- 双层休息 + 长休息不可延后、跳过后 10 分钟再提示；暂停 1 小时；免打扰时段。
- 托盘：勾选式菜单、tooltip 显示状态与下次休息时间；全局热键 Ctrl+Shift+E。
- 设置窗：分组卡片、min≤max 联动、时间拖拽编辑、恢复默认、开机自启开关（注册表为事实来源）、今日统计、立即休息、未保存提示。
- 旧配置迁移：你机器上的 v0.1 `config.toml` 已实测迁移（备份为 `config.toml.bak-v0.1-20260908-072447`），并尊重了你之前关闭免打扰的选择。

### 4.2 架构与工程
- `core.rs` 纯逻辑调度核心（12 个单元测试覆盖顺延 / 补发 / 延后 / 跳过 / 长休息 / 离开 / 免打扰 / 仅声音）。
- `Runtime::step` 统一步进；主线程轻量循环 ⇄ 按需 eframe 会话；键盘钩子过滤自动重复与修饰键；传感器线程 1 Hz。
- exe 嵌入程序化生成的图标与版本资源（build.rs + winresource，GNU/MSVC 均可）；单实例互斥；日志写 `%APPDATA%\eyeflow\eyeflow.log`；`EYEFLOW_DEMO=1` 演示模式。
- 仓库：MIT、CHANGELOG、CI（fmt/clippy/test/build）、tag 触发 release、NSIS 脚本版本注入、`.gitignore` 排除 970 MB 旧构建缓存与二进制。

## 5. 实测数据（Windows 11，NVIDIA 独显，2026-09-08）

| 指标 | v0.1 | v0.2 |
|---|---|---|
| 到点后用户看到什么 | 一声 0.5 s 提示音 | 预告浮窗 → 休息面板 → 统计 |
| 心流 / 游戏中到点 | 提醒被吞、计时重置 | 顺延 / 仅声音 + 补发 |
| 空闲常驻内存（未开窗口） | 未实测（估 <15 MB） | 15.6 MB 工作集 / 2.5 MB 私有 |
| UI 会话期间 | — | ~175 MB 工作集 / ~200 MB 私有（GL 上下文，休息面板约 30 s） |
| 会话后空闲 | — | ~87 MB / ~60 MB（NVIDIA 驱动堆残留） |
| 便携 exe | 1.4 MB | 7.2 MB（含 egui/eframe + 默认字体） |
| 安装包 | 0.74 MB | 2.8 MB |
| 单元测试 | 9（未跑） | 27 全部通过；clippy 零警告 |

## 6. 已知限制与 Roadmap
- 会话后 ~60 MB 私有内存残留来自显卡驱动，彻底解决需浮窗改 Win32 自绘（Roadmap）。
- 浮窗与严格模式遮罩只覆盖主显示器（多显示器待做）。
- 看视频等无输入场景 3 分钟后会被判“离开”而暂停提醒（与 Stretchly / LookAway 行为一致，属有意取舍）。
- 未签名 exe 首次运行有 SmartScreen 提示（README 已说明；可申请 SignPath 免费签名）。
- 夜间“降级而非停止”、自定义热键、Toast 辅助通道、winget 清单：见 README Roadmap。

## 7. 迁移说明
- 新仓库 `E:\Projects\MyGitHub\eyeflow` 已 `git init`，基线提交为 v0.1 原始源码，随后一次提交为 v0.2 重写；未推送到远端。`Cargo.toml` 里 `repository` 写作 `https://github.com/lexingtonhibiki/EyeFlow`（按你的 git 用户名推断），创建远端后 `git remote add origin … && git push -u origin main`，打 `v0.2.0` 标签即触发发布流水线。
- 原目录 `E:\Workspaces\Hanako\eyeflow` 未删除、未修改源码，只新增了 `docs/research/*.md` 调研报告；其中 4 个 `target*` 目录（≈970 MB）是 rustup 迁盘留下的空壳缓存，确认无用后可整体删除该旧目录。
- 本地构建：`cargo build --release`（GNU 工具链 + windres 已验证），`build-release.cmd` 一键生成 `dist\EyeFlow-<ver>-Setup.exe`。

## 8. 调研材料
- `docs/research/01-source-audit.md` 源码审计（P0/P1/P2 清单，文件:行号）
- `docs/research/02-science-evidence.md` 22 个权威来源（AAO / AOA / OSHA / Cochrane / PubMed）
- `docs/research/03-competitor-ux.md` Stretchly / Workrave / EyeLeo / SafeEyes / LookAway / CareUEyes / Iris
- `docs/research/04-windows-ux-guidelines.md` 27 条 Microsoft Learn 规范
- `docs/research/05-rust-stack-upgrade.md` 依赖版本与破坏性变更
- `docs/research/06-release-migration.md` 仓库骨架、CI、发布渠道
- `docs/adr/0001~0006` 决策记录；`CONTEXT.md` 领域词汇表；`docs/plan-v0.2.md` 实现计划
