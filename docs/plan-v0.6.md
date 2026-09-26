# EyeFlow v0.6 架构设计（内部工作文档）

> 依据：src/ 全部 14 个文件逐行通读（5293 行）、docs/spec.md、CONTEXT.md、docs/lessons.md、
> docs/adr/0001~0006、docs/research/07~08、docs/optimization-report-v0.2.md、docs/plan-v0.2.md、
> git log 全量历史决策。
> 目标：**（一）按高星开源项目的标准做一次极限优化升级**（性能 / 内存 / 审美 / 功能完整性）；
> **（二）补齐中英双语**（所有用户可见文案 + README 双份；CHANGELOG / ADR / 决策记录 / 内部注释保持中文）。
> 硬约束：不动用户个人配置的任何默认值；不做多显示器遮罩；不引入 Qt；不用 UPX；
> 不推翻 ADR-0006（按需 eframe 会话）除非有实测证据；体积 profile 保持 `opt-level="z" + lto + codegen-units=1`。
> 本文中所有标 **估算** 的数字均为工程估算（无法在本机离线环境实测），标 **实测** 的来自仓库已有记录。

---

## 0. 结论摘要

三条主线，按"每单位风险的收益"排序：

| 主线 | 内容 | 关键收益 | 风险 |
|---|---|---|---|
| A. 字体与会话内存 | 中文字体按语言惰性加载 + 预热 + 缓存共享 | 英文模式每次 UI 会话峰值工作集 **-18.8 MB**；首次弹窗不再有 18.8 MB 同步读盘卡顿 | 低（英文漏字由测试兜底） |
| B. 主循环与热路径 | 事件驱动唤醒替代 5 Hz 轮询；去掉每步的无谓分配与 Win32 调用 | 托盘点击延迟 200 ms → ~0；空闲唤醒 5/s → ~1/s；每步少 2 次堆分配 + 1 次 `SetMenuItemInfoW` | 中（消息泵语义，必须实测） |
| C. i18n 架构 | 自研 `tr!` + build.rs 代码生成 + 纯 toml 源 + 类型化错误 | 编译期缺 key 即构建失败；运行时零新依赖（+0 KB 二进制） | 低 |

一句话概括 v0.6：**"更省、更快、更好看、能说英文"——但每一条都必须能用一条命令验证，不靠感觉。**

---

## 1. 现状审计

> 方法：逐行通读 src/ 全部 14 个文件。每条给出 `file:line` 证据、"省什么 / 代价什么"、以及严重度。
> 严重度：**H** = 用户可感知或有正确性风险；**M** = 明确浪费；**L** = 整洁度。

### 1.1 会话内存与字体（最大单项）

| # | 位置 | 问题 | 量化 | 严重度 |
|---|---|---|---|---|
| A-1 | `app.rs:477-502` `install_fonts` | **无条件同步读 `C:\Windows\Fonts\msyh.ttc`（18.8 MB）**，包成 `Arc<FontData>` 塞进 `FontDefinitions`，在 UI 会话第一帧的主线程上完成 | 该 `Arc` 由 `Fonts` 持有到**会话结束**；峰值工作集 +18.8 MB（估算，按需访问模式下的常驻部分）。同函数还会在 `msyh.ttc` 不存在时依次尝试 11.6 MB 的 `msyhl.ttc` 与 9.3 MB 的 `simhei.ttf` | **H** |
| A-2 | `app.rs:484` | 上面那次读盘发生在 `UiSession::new` 里，**正是浮窗要出现的那一刻**。冷启动（文件不在 page cache）时是 18.8 MB 真实磁盘读 | SSD 上 **20~40 ms**（估算）卡在浮窗出现前；HDD 上更差。这是"预告浮窗弹出有延迟"的直接来源之一 | **H** |
| A-3 | `app.rs:490-496` | 字体被同时插入 `Proportional` 与 `Monospace` 两个 family 的 index 0；egui 默认字体（Ubuntu-Light / Hack / NotoEmoji）仍全量保留在二进制与图集里 | `default_fonts` feature（`Cargo.toml:16`）内嵌约 1.1~1.5 MB，占 9.75 MB exe 的 **11~15%**（估算，需 `cargo bloat` 验证）。因 `app.rs:288` 的 👀 与 `app.rs:347` 的 👏 依赖 emoji 字体，**不能直接关掉** | L |
| A-4 | `ui.rs:21` + `app.rs:45` | **同一张壁纸被缓存两次**：`SettingsState.wallpaper`（设置窗预览）与 `UiSession.wallpaper`（严格模式全屏）。两者各自调 `image::open` + `to_rgba8` + 可选 `resize` | 4K JPEG 同步解码 + 三角滤波缩放 **150~400 ms**（估算），发生在**休息开始的瞬间**、主线程。2560×1440 RGBA 纹理 = 14.7 MB 显存 + 14.7 MB CPU 侧，同时存活时翻倍 | **H** |
| A-5 | `wallpaper.rs:28` `MAX_TEXTURE_SIDE = 2560` | 上限合理，但与 A-4 叠加时没有总量约束 | 单图上限 14.7 MB；共享缓存后全局只有一份 | L |

### 1.2 主循环与热路径

| # | 位置 | 问题 | 量化 | 严重度 |
|---|---|---|---|---|
| B-1 | `main.rs:119` `std::thread::sleep(LIGHT_LOOP_TICK)` + `main.rs:156-167` `pump_win32_messages` | 轻量循环是**纯睡眠轮询**：每 200 ms 醒一次泵消息。托盘点击的最大响应延迟 = 200 ms（`main.rs:30` 自己也这么写） | 空闲唤醒 **5 次/秒 × 24 h = 43.2 万次/天**。改为 `MsgWaitForMultipleObjectsEx(hHandles=0, MWMO_INPUTAVAILABLE, timeout=1000ms)`：消息到达即返回（延迟 → ~0），无消息时 1 Hz 唤醒。**这里我不确定 muda 在 Windows 上是否 100% 走主线程消息队列**——见 §5 决策 D6 | **H** |
| B-2 | `runtime.rs:199-250` `step()` 的调用频率 | 同一个 `Runtime::step` 在三种不同节拍下被调用：轻量循环 5 Hz（`main.rs:96`）、UI 会话空闲 ~2 Hz（`app.rs:246` 的 500 ms）、休息面板显示中 ~4 Hz（`app.rs:168` 的 250 ms）。而 `core.rs:257` 的文档明确写"**每秒调用一次**" | 契约与实现不符。`dt` 粒度随模式漂移（0.2 / 0.5 / 0.25 s），`FLOW_PAUSE`（8 s）与 `RESUME_GRACE`（60 s）的判定抖动不可控。修法：`Core::tick` 内部按 `dt >= 1 s` 自门控，调用频率从此无关 | **M** |
| B-3 | `runtime.rs:242` → `tray.rs:166-172` `set_paused` | **无变化守卫**：每步都调 `pause_item.set_text(...)` → muda 内部加锁 + `SetMenuItemInfoW` 系统调用 | 轻量循环 5 次/秒、UI 会话每帧 1 次，**100% 无效工作**。`tray.rs:161` 的 `set_stats_line` 有守卫、`tray.rs:176` 的 `set_tooltip` 有守卫，唯独这个没有——`docs/research/08-code-quick-wins.md` #15 在 v0.4 就提了，v0.5.2 仍在 | **M** |
| B-4 | `runtime.rs:240-241` → `runtime.rs:499-514` `tooltip_text` | `set_tooltip` 有守卫，但**实参在守卫之前就构造好了**：每步 4~5 个 `String` 分配（`format!` × 3 + `state.label()` + `chrono` 的 `%H:%M`） | 5 次/秒 × 5 = **25 次堆分配/秒**，× 24 h = 216 万次/天。每次约 1~3 µs → **15~30 µs/s**（估算）。CPU 上可忽略，但让分配器永远处于 churn 状态 | L |
| B-5 | `core.rs:259` `let mut actions = Vec::new()` + `tray.rs:112` `let mut out = Vec::new()` | 两个**保证为空**的 `Vec` 在每次 `step` 里都做一次堆分配 | 5 次/秒 × 2 = 10 次/秒。`core.rs` 各分支经核对**每次 tick 至多 push 1 个 Action**，因此 `Vec<Action>` → `Option<Action>` 是**可证明等价**的化简。合计 **5~10 µs/s**（估算） | L |
| B-6 | `core.rs:258` `Vec<Action>` 的分配 | 每次 `tick` 无条件 `Vec::new()`，即使整条 tick 一路 return（`core.rs:272/308/321/333` 四个提前返回） | 同 B-5 | L |
| B-7 | `core.rs:248-250` `rand::rng()` | `schedule_next` 内 `rand::rng()` 每次重新取线程局部 RNG | 每次休息一次，非热路径。**无需改** | L |
| B-8 | `icon.rs:16` `render_rgba` | **不是热点**：唯一的运行时调用点是 `tray.rs:87`（启动时一次，DPI 尺寸）。4×4 超采样在 48×48 上是 36,864 次迭代，< 1 ms | — | 无 |
| B-9 | `build.rs:15` `ICON_SIZES = [16,20,24,32,40,48,64,128,256]` | `encode_bmp_entry` 对每个尺寸各跑一次 `render_rgba`；256×256 × 16 超采样 = **1,048,576 次**迭代（每次一个 `sqrt`） | 每次 `src/icon.rs` 变动触发构建时 **20~40 ms**（估算）。仅构建期，不影响运行时 | L |

### 1.3 磁盘 IO 与阻塞

| # | 位置 | 问题 | 量化 | 严重度 |
|---|---|---|---|---|
| C-1 | `main.rs:56-69` 的顺序 | 启动顺序是：键盘钩子 → 传感器线程 → **`audio::AudioPlayer::new()`** → 托盘。`rodio::DeviceSinkBuilder::open_default_sink()`（`audio.rs:108`）在 Windows 上要初始化 COM + 枚举并打开默认端点，**同步、主线程、在托盘出现之前** | WASAPI 打开默认端点 **5~50 ms**（估算），设备处于异常状态时可能更久。spec N3（`spec.md:87`）承诺"< 1 秒托盘出现"，把最慢的一项放在托盘之前是纯粹的顺序错误 | **M** |
| C-2 | `runtime.rs:122-123` `on_update_checked` → `persist_config()` | 更新检查**本身不阻塞 UI**（`runtime.rs:88-101` 已正确放后台线程，`ureq` 有 10 s 超时）。但**结果回调在主线程上同步写 config.toml** | 一次 < 1 ms 的 TOML 序列化 + 写盘。低频，不痛 | L |
| C-3 | `runtime.rs:237-239` `stats.save()` | 同上：主线程同步落盘 `stats.toml`，且 `config.rs:332-338` / `stats.rs:45-54` 的 `fs::write` **非原子** | 断电/崩溃可能留下半截 toml。`config.rs:290-322` 的"解析失败即改名备份重建"能兜住数据不丢，但会重置用户全部设置。`docs/research/08-code-quick-wins.md` #9 已提，未做 | **M** |
| C-4 | `ui.rs:664` `Config::path().display()` | **每帧**调用 `Config::path()` → `config_dir()`（`config.rs:387-392`：`std::env::var("APPDATA")` + `PathBuf::from` + `.join`）→ 至少 3 次堆分配 + 1 次环境变量查询，然后 `format!` 再一次分配 | 设置窗打开时**每帧 4+ 次分配**，60 fps 下 **240 次/秒**。egui 的 `weak()` 无缓存机制。修法：`SettingsState::new` 里算一次存字段 | **M** |
| C-5 | `ui.rs:638` `format!("当前版本 v{}", update::current_version())` | 每帧一次 `format!`（`current_version()` 是 `env!` 常量，无成本） | 60 次/秒 × 1 次分配。极小 | L |
| C-6 | `ui.rs:463` `s.draft.strict_wallpaper_path.clone()` + `wallpaper.rs:58` `PathBuf::from(path)` | 每帧为了**确认缓存命中**而克隆 String + 构造 PathBuf | 严格模式下 2~3 次分配/帧 | L |
| C-7 | `detector.rs:62` 无界 channel | 传感器线程 1 Hz 无条件 `tx.send`，而消费者在 UI 会话期间可能 15 分钟不跑（严格模式长休息） | 积压 900 条 `Sensors`（24 B）≈ 21 KB；`runtime.rs:199` 的 `while let` 会一次性全排空。无实际危害，但通道无背压 | L |
| C-8 | `main.rs:208-210` `std::panic::set_hook` | **在 `panic = "abort"`（`Cargo.toml:64`）下这是死代码**——没有 unwind，hook 永远不会被调用，而程序又没有 console（日志写文件） | 一旦真的 panic，用户侧**什么线索都没有**。要么去掉 `panic = "abort"`，要么改成"崩了就重启"（`spawn` 自己 + 单实例 mutex 交接） | **M** |

### 1.4 音频

| # | 位置 | 问题 | 量化 | 严重度 |
|---|---|---|---|---|
| D-1 | `audio.rs:194` `let base = generate(preset)` | `play_cue` 每次都重新合成基础图案：`gentle_chime()` = 2 次 `sweep`，各 24,000 样本，每样本一次 `sin()` | 48,000 次 f64 `sin` ≈ **0.5~1.5 ms**（估算），主线程。5 个预设可用 `OnceLock<[Vec<f32>; 5]>` 缓存，5 行代码。频率是"一天几次"，**不是热路径** | L |
| D-2 | `audio.rs:195-210` `render()` | 每次播放分配 `duration × 48000 × 4 B`：3 s = 576 KB，5 s = 960 KB，全量过一遍增益与限幅 | 0.2~0.4 ms + 半~1 MB 分配/次。低频 | L |
| D-3 | `audio.rs:99` `play_custom` 每次重新 `open_decoder` | 每次试听都重开文件、重解析容器头 | 流式打开，只读头，**< 1 ms**。可接受 | 无 |
| D-4 | `audio.rs:95-106` `play_custom` 无预览截断 | 点"试听"会把 5 分钟的自定义音频**以全音量完整播完** | 纯 UX 问题。修法：加 `take_duration(3s)` 的试听变体 | **M** |
| D-5 | `audio.rs:167` 固定 48 kHz | `Mixer::add` 自动重采样（已确认，`lessons.md:54`）。44.1 kHz 设备上会再产生一份重采样缓冲 | 每次播放 +576 KB 量级。**已被 rodio 内部处理，不改** | L |

### 1.5 托盘与图标

| # | 位置 | 问题 | 量化 | 严重度 |
|---|---|---|---|---|
| E-1 | `tray.rs:41-65` | 菜单文案在 `Tray::new` 里**一次性硬编码**为中文字面量 | 引入 i18n 后语言切换必须重写这 7 项 + tooltip，否则托盘菜单中英混杂 | 见 §5 D7 |
| E-2 | `tray.rs:87` | 托盘图标只有一份、只渲染一次，之后**永不更新** | 不是性能问题，是**功能缺失**：`docs/research/07-quick-wins-features.md` #14（托盘进度环）需要运行时重绘。`icon.rs` 的 `render_rgba(size)` 已经是纯函数，O(n²·16)，48×48 约 36,864 次迭代 ≈ 0.3 ms——**完全可以承担每 1 s 一次的重绘** | **M** |
| E-3 | `tray.rs:90, 175-182` | 初始 tooltip 硬编码 `"EyeFlow 护眼提醒"`，随后被 `runtime.rs:240` 每步覆盖 | 初始值实际只活 1 tick。无害 | L |
| E-4 | `tray.rs:177` `.chars().take(120)` | tooltip 截断按 **char** 数，但 `NOTIFYICONDATA.szTip` 上限是 **128 个 WCHAR（含 NUL）**。char 数 ≤ 120 在正常文本体上够用，但含 emoji（代理对，1 char = 2 WCHAR）时会溢出 | 极端情况下 `SetWindowTextW` 静默失败。`tray.rs:174` 的注释说 128，与代码的 120 不一致 | L |

### 1.6 UI 审美与无障碍（对照高星项目标准）

| # | 位置 | 问题 | 判定 | 严重度 |
|---|---|---|---|---|
| F-1 | `app.rs:463-465` `accent()` = `ui.visuals().selection.bg_fill` | 强调色**直接借用 egui 默认蓝**，没有品牌色 | "Rust 极简"是一种美学主张，但对比 VS Code / Zed / GitHub 的做法，全部有**具名语义色板**。当前无法在代码里表达"这是强调色"，也无法在换主题时保证对比度 | **H**（审美） |
| F-2 | `ui.rs:167-169` 与 `app.rs:167-169` | 状态色 `rgb(70,170,100)` / `rgb(230,150,40)` / `rgb(220,80,80)` **在两个文件里各写一遍**，没有名字、没有 light/dark 变体 | 浅色主题下 `rgb(70,170,100)` 对白底的对比度约 **2.3:1**，远低于 WCAG AA 正文 4.5:1（`docs/research/02-science-evidence.md` 已引用 WCAG 1.4.7 做音量/时长依据，说明项目在意无障碍，这里漏了）。**这是实打实的无障碍缺陷** | **H** |
| F-3 | `ui.rs:220, 293, 412, 444, 464, 473, 477, 490, 496, 502, 504, 512, 529, 544, 546, 549, 561` | **用全角空格 `"　"` 做手工缩进**（`ui.label("　　　　　")` 五连）而不是 `ui.indent()` / `ui.add_space()` | 在中文字体下碰巧对齐；一旦字体回退、或切到英文（`　` 是一个全角空白，视觉宽度不变但语义为空）就彻底错位。**引入 i18n 后会放大**。这是 v0.6 必须清掉的债 | **H** |
| F-4 | 无 `ui_zoom_pct` | 界面字号不可调；Windows 的"放大文本"（125~500%）**对 Win32 桌面应用不自动生效** | `docs/research/08-code-quick-wins.md` #12 已标 S 成本。egui 0.36 有 `ctx.set_zoom()` | **H**（无障碍） |
| F-5 | `ui.rs:98-119` `show()` | 设置窗 6 张卡片、约 30 个控件，**无搜索/过滤、无折叠** | 对照 `ez` / `egui_demo` 的 settings 模式：搜索框 + 分组折叠是这类面板的标配。当前把"提示音音量"和"蒙层浓度"排在同一滚动流里 | **M** |
| F-6 | `ui.rs:159` `format!("● {} · {}", ...)` | 状态点 `●` 恒为默认文字色，四个上下文态视觉上无差别 | 用 F-1 的色板给 Desktop / Flow / Gaming / Away 各一个色，状态一眼可辨 | M |
| F-7 | `app.rs:467-474` `tune_style` | 只设了 4 个 `corner_radius` 与 2 个 spacing。没有字号阶梯、没有字重、没有控件高度、没有 focus ring 定制 | 对照 `egui` 的 `Visuals` 能力，我们只用了 5%。一个 30 行的 `Visuals` 覆写 + 一份 `design tokens` 就能把整体质感拉一档 | **M** |
| F-8 | `app.rs:110-114` 面板定位 | 预告浮窗用 `monitor.x - w - 24.0, monitor.y - h - 84.0` 硬编码 84 px 的任务栏余量 | 多显示器被用户明确否决，但**单屏下 84 也是猜的**（任务栏 40 px 自动隐藏 / 60 px 常规 / 120 px 大图标都不同）。用 `monitor_size` + `MonitorFromPoint` 拿工作区矩形（`SPI_GETWORKAREA`）一次解决 | M |
| F-9 | `app.rs:228-247` `App::logic` | `request_repaint_after(500ms)` 与 `app.rs:168` 的 `request_repaint_after(250ms)` 是两套独立调度器 | 逻辑无害但语义混乱；统一到一个"下次重绘时刻"来源后，B-2 的自门控才好做 | L |

### 1.7 工程与文档一致性（"高星标准"的硬指标）

| # | 位置 | 问题 | 严重度 |
|---|---|---|---|
| G-1 | `README.md:196` "共 27 个用例" | 实际 **49** 个（`grep -c '#\[test\]' src/*.rs` = 8+10+14+3+2+2+10） | **H** |
| G-2 | `docs/lessons.md:60` "53 个测试锁住调度规则" | 与 49 也不一致 | **M** |
| G-3 | `docs/spec.md:89` N5 "便携 exe 约 7 MB" | `target/release/eyeflow.exe` 实测 **10,224,640 B ≈ 9.75 MB** | **H** |
| G-4 | `docs/spec.md:98-107` 配置项清单 | 缺 `strict_overlay_pct` / `strict_overlay_gradient` / `start_cue_enabled` / `esc_skip_enabled` 四个 v0.4~v0.5 新增字段 | **H** |
| G-5 | `README.md:139-168` config 块 | 缺同上的四个字段；`custom_sound_path = ""` 但 serde 是 `Option<String>`（`config.rs:182`），`""` 反序列化后是 `Some("")`——README 与真实格式**不一致** | **H** |
| G-6 | `.github/workflows/ci.yml:31-40` | `cargo fmt --check` 与 `cargo clippy` 都是 `continue-on-error: true`，注释写"稳定后可移除"——那是 v0.2 的 TODO，v0.5.2 仍在。等于**格式化与 lint 完全没有把关** | **H** |
| G-7 | `.github/workflows/ci.yml:17` | `runs-on: windows-latest`（= MSVC）单矩阵。本地默认 GNU 工具链。`lessons.md:61` 明确记录过"edgedim 的测试断言在 GNU（本地）能推断通过、在 MSVC（CI）报错" | **H** |
| G-8 | `ci.yml` | 无 `cargo doc`、无 MSRV 校验（`Cargo.toml:5` 声明 `rust-version = "1.95"` 却没有对应的 `1.95.0` job）、无 `cargo audit` | M |
| G-9 | `src/*.rs` 混用全角与半角标点 | `audio.rs:70` 用半角 `,` / `:`；`ui.rs:238` 用全角 `，：`。同类界面文案标点不统一 | L |

### 1.8 用户可见字符串完整清单

> 判定口径：**真用户** = 会出现在界面上；**日志** = 只进 `%APPDATA%\eyeflow\eyeflow.log`；**测试** = `#[test]` 断言文本；**注释** = 不显示。
> 统计：源码中带中文的字符串字面量共 **188 处**，其中真正面向用户的 **约 152 处**（另 36 处是 log/测试断言/注释）。

#### src/ui.rs —— 85 处，全部为**真用户**（设置窗）

| 类别 | 行号 | 说明 |
|---|---|---|
| 卡片标题 | 157, 201, 310, 510, 598 | "现在" / "提醒节奏" / "提醒方式" / "上下文感知" / "系统" |
| 复选框 / 开关 | 202, 255, 313, 320, 328, 373, 561, 602, 614, 633 | 10 个 |
| 按钮 | 161, 345, 413, 416, 445, 448, 639, 665, 674 | "立即休息" / "▶ 试听" / "选择音频文件…" / "清除" / "选择图片…" / "立即检查" / "打开目录" / "恢复默认" |
| 标签 | 210, 244, 264, 272, 291, 293, 386, 391, 444, 451, 473, 477, 512, 529, 544, 546, 549 | 17 个 |
| 单位后缀 | 214, 224, 245, 269, 274, 292, 294, 388, 392, 531 | " 分钟" / " 秒" / " 小时" / " %" / " 分钟无操作"——**必须逐语言重写**：英文是 " min" / " s" / " h" / "%" / " min idle" |
| 说明 / 帮助文本 | 179, 188, 193, 195, 238, 282, 367, 394, 437, 464, 496, 502, 524, 537, 628, 645, 648, 653, 655, 658, 681, 685 | 22 个 |
| 状态统计格式化 | 186, 191, 194 | `format!("{} 次", n)` ×3 —— **复数点** |
| 时长格式化 | 691-700 `human_duration` | `"{n} 秒"` / `"{n} 分钟"` / `"{n} 小时 {m} 分"` —— **英文明写为 "s / min / h"** |
| 动态 | 638（当前版本）, 664（配置文件路径） | 路径是用户数据，不翻译 |

#### src/app.rs —— 17 处，全部为**真用户**（预告浮窗 / 休息面板 / 闪屏 / 窗口标题）

| 行号 | 内容 | 备注 |
|---|---|---|
| 191 | `.with_title("EyeFlow 设置")` | **原生窗口标题栏**，走 SetWindowTextW，不经 egui 字体 |
| 291, 293, 297, 299 | 预告浮窗的标题与副标题 | 293 是 `"{n} 秒后休息一下"` —— 复数点 |
| 305, 308, 321 | 按钮 "现在开始" / `"延后 {n} 分钟"` / "跳过" | 308 复数点 |
| 312, 314, 316 | 按钮 hover 提示 ×3 | |
| 347, 353 | 欢迎回来闪屏 | 含 👏 emoji |
| 412, 414, 444, 449 | 休息面板标题 / "继续工作" / 底部说明 | |
| 288 | `"👀"` | **语言中性，无需翻译**，但依赖 egui 的 NotoEmoji |

#### src/tray.rs —— 12 处，全部为**真用户**（原生 Win32 菜单与 tooltip，**不走 egui 字体系统**）

| 行号 | 内容 | 备注 |
|---|---|---|
| 41, 44, 51, 56, 59, 64, 65 | 7 个菜单项 | 51 含 `"\tCtrl+Shift+E"` 制表符加速键，**英译时不能破坏 `\t`** |
| 90 | 初始 tooltip `"EyeFlow 护眼提醒"` | |
| 153, 155 | `set_hotkey_hint` 的两个变体 | |
| 168, 170 | `set_paused` 的两个变体 | |

#### src/runtime.rs —— 14 处，其中 **9 处真用户 / 5 处日志或被日志包裹**

| 行号 | 内容 | 归类 |
|---|---|---|
| 245 | `"今日休息 {n} 次 · 跳过 {n} · 延后 {n}"` | **真用户**（托盘菜单禁用行）· 复数点 |
| 406 | `format!("播放失败：{e}")` | **真用户**（设置窗红字） |
| 412, 413 | rfd 文件对话框标题 / 过滤器名 | **真用户**（原生对话框） |
| 450, 451 | rfd 图片对话框标题 / 过滤器名 | **真用户** |
| 481, 483, 491, 493, 494 | `reminder_line`：提醒已关闭 / 已暂停 1 小时 / 下次休息 HH:MM / 即将休息 / 休息中 | **真用户**（tooltip + 设置窗状态卡共用） |
| 502, 507 | tooltip 的多行模板 | **真用户** |
| 136 | `Err("热键管理器不可用")` | 仅进 `log::warn`（`runtime.rs:73`）→ **日志** |
| 107（main.rs） | UI 会话失败 | **日志** |

#### src/config.rs —— 12 处真用户 + 3 处仅日志

| 行号 | 内容 |
|---|---|
| 36, 37, 38 | `FlowSensitivity::label()` 三档 |
| 73~78 | `SoundPreset::label()` 六个预设 |
| 107~109 | `WallpaperFit::label()` 三种自适应 |
| 478, 479, 480 | `ConfigError` 的 Display —— **仅日志**（`config.rs:301/309/327/344/356` 全部走 log） |

#### src/core.rs —— 4 处真用户（`ContextState::label()`），其余 10 处是 `#[test]` 断言文本

#### src/tips.rs —— **7 处真用户，且就是天然的字符串表**

> `tips.rs` 全文 17 行、7 条贴士。确认：它是一张纯静态文案表，**没有 key、没有语言维度**。它应该整体变成 `tr!("tip.0") … tr!("tip.6")`，并顺带解决另一个问题——`research/07` #8 提的"自定义贴士"只需要在同一张表上加"用户追加"层。

#### src/update.rs —— 5 处真用户

| 行号 | 内容 |
|---|---|
| 52 | `"GitHub 上还没有发布版本（Release）"` → 英文应纯 ASCII：`"No release published on GitHub yet"` |
| 53, 57, 58, 65 | 网络 / 读取 / 解析失败原因（经 `ui.rs:658` 显示） |

#### src/audio.rs —— 4 处真用户（3 处错误 + 1 处时长错误），3 处是测试断言

| 行号 | 内容 |
|---|---|
| 70 | `"时长 {m} 分 {s} 秒,超过 {n} 分钟上限"` |
| 80, 82 | `"文件不存在"`（出现两次，建议合并） |
| 83 | `"无法解码:格式不支持或文件损坏"` |

#### src/autostart.rs —— 4 处，仅经 `ui.rs:608` 的红字显示 → **真用户**

| 行号 | 内容 |
|---|---|
| 49, 52, 58, 69 | 打开 Run 键失败 / 获取程序路径失败 / 写入自启项失败 / 删除自启项失败 |

#### src/wallpaper.rs —— 3 处真用户

| 行号 | 内容 |
|---|---|
| 95 | `"无法读取图片:{err}"`（经 `ui.rs:504` 显示） |
| 253 | `"（上深下浅）"` |
| 258 | hover 提示 `"原图 {w}×{h} px · 当前模式：{} · 蒙层 {n}%{shape}"` |

#### src/detector.rs —— 2 处，是 `.expect()` 消息，**不是用户可见**（panic 在 `panic=abort` 下也不写日志，见 C-8）

#### 不翻译的部分（明确边界）

| 位置 | 理由 |
|---|---|
| 所有 `log::info!/warn!/error!/debug!` 字面量（`main.rs:37,40,49,66,78,86,107,123`；`runtime.rs:73,111,118,143,148,217,220,266,276,318,331,334,338,347,349,360,363,366,386,474`；`config.rs:301,315,328`；`wallpaper.rs:75`；`audio.rs:114`；`detector.rs:192`；`autostart.rs`） | 只进日志文件；**保持中文**（项目约定） |
| `#[test]` 断言文本（`core.rs:521,527,538,543,550,585,661,726,738,739`；`config.rs:560,568,594,606,613`；`update.rs:117,119,122,124`；`audio.rs:401,419,437`） | 开发期文本，中文 |
| `CONTEXT.md` / `CHANGELOG.md` / `docs/**` / `CONTRIBUTING.md` | 中文 |
| `build.rs:47` `res.set("FileDescription", "EyeFlow 护眼提醒")` | **在任务管理器"详细信息"页与文件属性里可见 → 属于用户可见**。但 Windows 生态惯例是英文，建议改为纯英文 `"EyeFlow - Eye Care Reminder"`（见 §5 D3 的附带讨论） |

---

## 2. i18n 架构方案

### 2.1 依赖选型：结论是**不引入任何 crate，自研 `tr!` + build.rs 代码生成**

| 方案 | 编译期类型安全 | 体积影响（`opt-level="z"` + LTO + `panic=abort`，当前 exe 9.75 MB） | 对 49 个测试的影响 | egui 运行时切换 | 判定 |
|---|---|---|---|---|---|
| **自研 `tr!` + build.rs** | ✅ 最强：`Key` 是生成的 enum，`tr(Key::X)` 写错即编译失败；**缺任何语言的 key 直接 `build.rs` 报错，构建不通过** | **+0 KB**。字符串本来就是 `.rodata` 常量，且总量约 6 KB；无新 crate、无新运行时依赖 | 3 个（仅当错误本地化下沉到领域层，见 2.6） | ✅ 一个 `AtomicU8` + `request_repaint` | **采用** |
| `rust-i18n` | ✅ 宏在编译期校验 key | 需要 `serde_yaml`（**该 crate 已于 2024 年被 dtolnay 正式归档弃用**）+ proc-macro 链，估计 +250~400 KB，且引入一个已弃用的传递依赖——对"高星标准"是减分项 | 3 个 + 新增 1 个 | ✅ 全局 `set_locale` | 否决 |
| `i18n-embed`（+ `fluent` feature） | ⚠️ 运行时解析 FTL，key 拼错**运行时不报错，只显示 key 本身** | +`unic-langid` +`fluent-bundle` +`intl-memoizer` +`self_cell`，估计 **+400~800 KB**（+5~8%），编译时间明显增加 | 0 | ✅ `select(&lang)` | 否决 |
| `fluent`（`fluent-bs` 裸用） | ⚠️ 同上，运行时解析 | 同上（`i18n-embed` 的子集） | 0 | ✅ | 否决（复数能力过剩） |
| `i18n-embed-fl` | — | — | — | — | **这里我不确定**：`i18n-embed` 的 Fluent 支持是通过自身的 `fluent` feature 提供的，我未能确认存在一个被广泛使用的独立 `i18n-embed-fl` crate（本次环境无网络，无法查证 crates.io）。若确有，其形态应与 `i18n-embed` + fluent 等价，否决理由同上 |

**理由归纳**：
1. **体积是这个项目的第一约束**。ADR-0006 已经为了 160 MB 内存砍掉了常驻 eframe；`opt-level="z"` 是有意识的取舍。在这个 profile 下引入 400~800 KB 的纯文案依赖，是用一个 6 KB 的问题换 5~8% 的二进制——不成比例。
2. **规模不匹配**。Fluent 的价值在"语言 × 变体（复数/性别/格）"的组合爆炸。EyeFlow 是 **1 个产品 × 2 种语言 × 单数/复数二值**，用一个 `|` 分隔的二元表就能完全覆盖。为 6 种斯拉夫语族的复数规则设计一套 DSL 属于过度工程。
3. **编译期保证是最硬的收益**，而自研方案恰好在这一维度上**优于所有 crate**：`build.rs` 可以在构建时让"缺 key"直接失败。`i18n-embed` / `fluent` 是运行时解析，漏掉的 key 会在用户界面上显示成 `"tray.open_setting"`。
4. **不引入 `serde_yaml`** 这个已归档的传递依赖，保住依赖树的健康度。

### 2.2 目录与源文件格式

```
eyeflow/
├── locales/
│   ├── zh-CN.toml          # 人类编辑的源文件，中文
│   └── en-US.toml          # 人类编辑的源文件，英文
├── build.rs                # + 第 2 步：生成 i18n.rs
└── src/
    ├── i18n.rs             # 生成物（include!(concat!(env!("OUT_DIR"), "/i18n.rs"))）
    └── tr.rs               # tr! 宏、Lang 枚举、set_language、tr 访问器
```

`locales/*.toml` 格式（扁平、单层、key 全小写点分）：

```toml
# locales/zh-CN.toml
"tray.open_settings" = "打开设置"
"tray.rest_now"       = "立即休息\tCtrl+Shift+E"
"stats.done_count"    = "完成 {n} 次|完成 {n} 次"   # {n}==1 | {n}!=1
"unit.minute"         = "{n} 分钟"                  # 中文无需复数
```

`locales/en-US.toml`：

```toml
"tray.open_settings" = "Open settings"
"tray.rest_now"      = "Take a break now\tCtrl+Shift+E"
"stats.done_count"   = "{n} break|{n} breaks"
"unit.minute"        = "1 min|{n} min"
```

### 2.3 `build.rs` 生成什么

生成的 `$OUT_DIR/i18n.rs` 包含：

```rust
// 自动生成，请勿编辑。源：locales/*.toml
pub enum Lang { ZhCn = 0, EnUs = 1 }
pub const LANG_COUNT: usize = 2;
pub const KEY_COUNT: usize = /* 唯一 key 数 */;

pub const KEYS: [&str; KEY_COUNT] = [ /* 排序后的 key */ ];
pub const ZH_CN: [&str; KEY_COUNT] = [ /* 索引与 KEYS 一一对应 */ ];
pub const EN_US: [&str; KEY_COUNT] = [ /* ... */ ];
```

`build.rs` 的三步：

1. 读 `locales/zh-CN.toml`（**基准语言**）与 `locales/en-US.toml`；
2. 以基准语言的 key 集合为唯一事实来源，**逐一比对**：
   - 某语言缺 key → `panic!("locales/en-US.toml 缺少 key: {k}")`，**构建失败**；
   - 某语言多 key → `panic!("locales/en-US.toml 有基准语言不存在的 key: {k}")`，**构建失败**；
3. 按基准语言的 key 排序写出 `KEYS` 与各语言的 `&[&str; KEY_COUNT]`（数组长度即 key 数，编译器保证与 `KEY_COUNT` 一致）。

`Cargo.toml` 增加 `[build-dependencies] toml = "1"`（`toml` 已是运行时依赖，构建期重复编一份只影响构建时间，**不影响二进制体积**）。若不接受这个重复编译，退路是让 `build.rs` 用纯 `str::lines()` 解析一个 `key = "value"` 的扁平子集（`locales` 里的值都是单行、无转义需求）——**这里我不确定 toml 在 build-dependencies 里重复编译的实际耗时，落地时先测**。

### 2.4 `tr!` 与参数化

```rust
// src/tr.rs
use std::sync::atomic::{AtomicU8, Ordering};

static LANG: AtomicU8 = AtomicU8::new(Lang::ZhCn as u8);
static LANG_VERSION: AtomicU32 = AtomicU32::new(0);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang { ZhCn, EnUs }
impl Lang {
    pub fn from_config(s: &str) -> Self {
        // "zh" / "zh-CN" / "zh-Hans" → ZhCn；其余一律 EnUs（不回退到中文）
        if s.starts_with("zh") { Lang::ZhCn } else { Lang::EnUs }
    }
    pub fn code(self) -> &'static str { match self { Lang::ZhCn => "zh-CN", Lang::EnUs => "en-US" } }
}
pub fn language() -> Lang { match LANG.load(Ordering::Relaxed) { 0 => Lang::ZhCn, _ => Lang::EnUs } }
pub fn language_version() -> u32 { LANG_VERSION.load(Ordering::Relaxed) }

/// 切换语言。返回 true 表示确实发生了变化。
pub fn set_language(l: Lang) -> bool {
    if language() == l { return false; }
    LANG.store(l as u8, Ordering::Relaxed);
    LANG_VERSION.fetch_add(1, Ordering::Relaxed);
    true
}

/// 无参数翻译。key 必须是生成的 enum 变体 → 写错编译不过。
#[macro_export]
macro_rules! tr {
    ($key:ident)      => { $crate::i18n::lookup($crate::i18n::Key::$key) };
    ($key:ident, $name:ident = $v:expr) => {{ /* 见下 */ }};
}
```

`tr!` 的实现要点：

- **静态表 + 运行期语言索引**，零分配、零解析：`lookup(k) = match language() { ZhCn => ZH_CN[k as usize], EnUs => EN_US[k as usize] }`。
- **参数化只在调用点做一次 `replace`**：`"完成 {n} 次"` 里 `n` 是唯一占位符时，编译期就能确定；带两个以上占位符时按 `str::match_indices("{")` 逐个替换。`tr!` 宏为**每个调用点**生成一个 `&'static str` 形参类型的校验——占位符名与 `Key` 一一对应，写错参数名在宏展开时就报"no field named"。

**复数 / 量词规则**（就 zh-CN + en-US 而言这是完备的）：

| 场景 | 约定 | 例 |
|---|---|---|
| 中文无复数变化 | 两形式写相同 | `"完成 {n} 次\|完成 {n} 次"` |
| 英文 1 / other | `\|` 分隔，索引 = `n == 1 ? 0 : 1` | `"1 break\|{n} breaks"` |
| 时间单位不是复数 | 单独 key，各语言各自缩写 | zh `"{n} 分钟"`；en `"1 min\|{n} min"` |
| 中文没有的量词 | 英文需要时由英文表自己补 | zh `"现在"`；en `"Start now"` |

> **这里我不确定**：这套 `|` 二元约定无法覆盖 ru/pl/uk 的 6 种复数形式，也覆盖不了 en 的 "1.5 hours"。如果将来加第三种语言，**要么**给 key 加 `{n}` 之外的显式类别前缀（`unit.hour.one` / `unit.hour.few` / `unit.hour.other`），**要么**换 Fluent。届时的迁移成本 = 重写 `locales/*.toml`，因为代码侧的 `tr!` 调用点不变——这是我把它设计成"数据层而非代码层"的原因。

### 2.5 语言切换入口与即时刷新

**入口**：`ui.rs` 的 `system_card`（`ui.rs:592` "系统" 卡片）顶部加一行 `egui::ComboBox`（"界面语言：简体中文 / English"）。**不放托盘**——托盘菜单是高频点击区，塞语言项会挤掉"立即休息"；托盘只作为首次启动的兜底（见 §5 D3）。

**即时刷新机制**（四件事，缺一不可）：

1. **egui 视口内容**：全部是每帧从 `tr!()` 现取的，egui 每帧重排整个 pass，**无需重建视口**。切完立刻调用 `ctx.request_repaint()` 即可。
2. **字体**：`install_fonts`（`app.rs:477`）改为按 `language()` 惰性决策。
   - `Lang::EnUs` → **完全跳过 CJK 字体加载**，只留 egui 默认字体（默认字体覆盖 ASCII + emoji）→ 每次 UI 会话峰值工作集 **-18.8 MB**（估算）。
   - `Lang::ZhCn` → 加载（`en → zh` 切换时按需补）。
   - 切换时调 `ctx.set_fonts(build_font_defs(language()))`；字体表变了**必须** `set_fonts`（egui 会重建图集），这是唯一需要显式通知 egui 的地方。
3. **原生窗口标题**：`app.rs:191` 的 `.with_title("EyeFlow 设置")` 改为 `tr!(settings_window_title)`。`show_settings` 每帧重建 `ViewportBuilder`（`app.rs:190-195`），egui 会与上一帧的 builder 比对并下发 `ViewportCommand::Title`。**这一条需要一次人工验证**（"这里我不确定"：egui 0.36 的 viewport 状态比对是否在只改 `title` 时确实下发命令；最保险的兜底是切换时显式 `ctx.send_viewport_cmd(id, ViewportCommand::Title(t.into()))`）。
4. **非 egui 表面**（托盘菜单、tooltip、rfd 对话框）→ 见 §5 D7 与下面 2.7。

**`language_version` 计数器的用途**：给那些"需要在语言变化时做副作用"的地方一个可测的钩子。

```rust
// UiSession 增加字段
last_lang_version: u32,
// App::logic 每帧：
if crate::tr::language_version() != self.last_lang_version {
    self.last_lang_version = crate::tr::language_version();
    ctx.set_fonts(crate::tr::build_font_defs(crate::tr::language()));  // 步骤 2
    self.rt.tray.apply_language(crate::tr::language());               // 步骤 4
    ctx.request_repaint();
}
```

`App::logic` 已经是"每帧必跑"的（`app.rs:228-247`），不需要额外的通道。`build_font_defs` 内部用 `OnceLock` 缓存**每种语言**的 `FontDefinitions`，避免重复读 18.8 MB。

### 2.6 配置持久化与向后兼容

**字段**：

```rust
// src/config.rs，紧跟 strict_overlay_gradient 之后
/// 界面语言（zh-CN / en-US）
#[serde(default = "d_language")]
pub language: String,
fn d_language() -> String { "zh-CN".into() }   // ← 默认值与现状完全一致
```

**向后兼容论证**（这是必须锁死的一条）：

| 场景 | 行为 |
|---|---|
| 已有用户的 `config.toml` **没有** `language` 行 | serde `default` → `"zh-CN"`。**行为与今天完全相同**（今天只有中文）。不触发 `migrate_legacy`（`config.rs:400` 的判据是"没有 `short_break_min_secs` 且有 v0.1 键"），不触发备份重写 |
| 全新安装 | `Config::default()` → `"zh-CN"`；`main.rs:45` 的 `first_run` 已存在，首跑开设置窗，引导里就有语言选择 |
| 手工写了 `language = "en-US"` | `Lang::from_config` 解析；无法识别的值（`"klingon"`）**回退到 `en-US`**，不 panic、不静默变成中文 |
| `sanitized()`（`config.rs:355-370`） | 加一条 `self.language = Lang::from_config(&self.language).code().to_string()`，把任何非法值规范化并落盘 |

**为什么默认 `zh-CN` 而不是"跟随系统语言"**：现有用户的中文配置**必须**保持中文（用户明确要求不动任何默认值来配合设计）；而"跟随系统"对新装用户是更友好的默认值。两者只有在"全新安装"这一种情形下才有分歧——见 §5 D3 决策。

### 2.7 非 egui 渲染路径的文案

| 路径 | 现状机制 | v0.6 做法 |
|---|---|---|
| 托盘菜单 7 项 | `Tray::new` 一次性硬编码（`tray.rs:41-65`） | 拆成 `Tray::new` 构造空菜单 + **`Tray::apply_language(lang)`** 逐项 `set_text`。`tray.rs:151-172` 已经示范了 `set_hotkey_hint` / `set_paused` 的 `set_text` 用法，模式现成。`apply_language` 内需同时更新 tooltip（`set_tooltip` 的守卫会自动放行，因为文本变了） |
| 托盘 tooltip | `runtime.rs:240` 每步重算 | 文案本身改 `tr!`；**顺带按 B-4 加"先比后构造"守卫** |
| 托盘气泡 / Toast | **当前不存在**（`update.rs` 只显示在设置窗里，README 明确"无网络无通知"，`update_check_enabled` 默认 `false`） | 无需处理。`docs/research/07` #5 的 Toast 尚未实现，届时文案自动走 `tr!` |
| rfd 原生文件对话框 | `runtime.rs:412,450` 每次调用时传 `.set_title()` / `.add_filter()` | 已在调用点，直接换 `tr!`。**不缓存**——对话框是一次性的 |
| 原生窗口标题栏 | `app.rs:135,191` | 见 2.5 步骤 3 |
| Win32 版本资源 | `build.rs:47` `FileDescription` | 构建期常量，**不进 i18n 系统**。见 §5 D3 |

### 2.8 测试策略

**现有 49 个测试的受影响面**：

| 文件 | 用例数 | 是否需改 | 原因 |
|---|---|---|---|
| `core.rs` | 14 | **否** | 全部断言 `Phase` / `ContextState` 枚举与计时，**没有一个测试碰 `label()`** |
| `config.rs` | 10 | **否**（+1 新增） | 断言静默时段、清洗、hhmm、迁移。新增一条：老配置无 `language` 字段时读到 `"zh-CN"` |
| `wallpaper.rs` | 10 | **否** | 断言 `Rect` / `u8` 数学；`preview()` 的 hover 文本无测试 |
| `audio.rs` | 8 | **3 个需改** | `audio.rs:401` `err.contains("文件不存在")`、`:419` `err.contains("超过")`、`:437` `err.contains("无法解码")` |
| `icon.rs` | 3 | 否 | 纯字节 |
| `stats.rs` | 2 | 否 | 纯数值 |
| `update.rs` | 2 | 否 | 版本号数学 |
| **合计** | **49** | **3 个**（6%） | |

**3 个 audio 测试怎么改**：按 §5 D5 的"错误类型化"路线，把 `probe_custom` / `open_decoder` 的 `Result<_, String>` 换成 `Result<_, AudioError>`（`enum AudioError { NotFound, Undecodable, TooLong { d: Duration, limit: Duration } }`），测试断言**枚举变体**而不是文本子串。这样既修了 3 个测试，又顺手干掉了"英文界面弹出中文错误"这个必然的 bug——这正是 D5 存在的理由。

**新增测试（把"没漏"变成可自动验证的性质）**：

| 测试 | 位置 | 断言 |
|---|---|---|
| `T1 语言表键位对齐` | `src/tr.rs` | `KEYS.len() == ZH_CN.len() == EN_US.len() == KEY_COUNT`；且 `KEYS` 严格递增（无重复 key） |
| `T2 占位符对齐` | `src/tr.rs` | 逐 key 提取 `{name}` 集合：`ZH_CN[i]` 与 `EN_US[i]` 的占位符名**完全相同**（允许英文多写一个 `{n}` 用于单复数？→ 不允许，必须一致，多余的 `{n}` 应写在两形式里） |
| `T3 占位符均被使用` | `src/tr.rs` | `tr!` 的编译期展开已经保证；此处再断言表里**没有未闭合的 `{`**（防手滑） |
| `T4 无裸中文字面量` | `tests/no_hardcoded_cjk.rs` | 递归扫 `src/**/*.rs`（**排除 `locales/`、`#[cfg(test)]` 块、注释、`log::` 调用**），任何落在 `tr!` 之外的中文字面量 → 测试失败 |
| `T5 英文表无 CJK` | `src/tr.rs` | `EN_US.iter().all(|s| !s.chars().any(is_cjk))`——堵住"忘了翻的那一条" |
| `T6 中文表无英文残渣` | `src/tr.rs` | `ZH_CN` 中不得出现连续的 3 个以上 ASCII 单词（`Ctrl+Shift+E`、`wav` 等专有名词白名单） |
| `T7 config 向后兼容` | `src/config.rs` | `toml::from_str::<Config>("enabled = false")` → `language == "zh-CN"` |
| `T8 语言往返` | `src/config.rs` | `Lang::from_config("zh")`/`("zh-CN")` → `ZhCn`；`("en")`/`("en-US")`/`("")`/`("xx")` → `EnUs`；`set_language` 后 `tr!` 返回对应语言，`language_version` +1 |
| `T9 托盘文案同步` | `src/tray.rs` | 两种语言下遍历 7 个菜单项的 `text()`，断言与 `tr!` 一致（防 D7 的"忘了重写"） |
| `T10 README 一致性` | `tests/readme_consistency.rs` | 见 §4.3 |

`T4` 的实现要点：正则 `"[^"]*[\u4e00-\u9fff][^"]*"`，逐行剔除 `//` 开头与行内注释、`log::` 开头、`#[cfg(test)]` 之后的部分、`mod tests` 到文件尾。这一条是整套 i18n 的**守门测试**，价值最高。

### 2.9 README 双语结构方案

**要求**：不能两份独立文件互相漂移，也不能纯机翻混排。

**方案**：`README.md` 是**唯一文件、唯一入口**（GitHub 默认展示，仓库没有文档站），结构为：

```markdown
# EyeFlow

**Context-aware eye-care break reminder for Windows**
**上下文感知的极简护眼提醒工具**

> English below · 中文在下 ｜ [English](#english) · [中文](#中文)

（徽章 / 界面预览表格 / 竞品对比表 —— **语言中性，原样保留一次，不重复**）

---

<a id="english"></a>
## English
（完整英文：Why EyeFlow / Features / Science / Install / Usage / Configuration /
 Build from source / Uninstall / Privacy / Roadmap / Contributing / Design records / License）

---

<a id="中文"></a>
## 中文
（完整中文，同结构）
```

关键设计决定：

1. **语言中性内容只出现一次**（徽章、截图表格、功能对照表、MIT License 链接）。这些是图片/数字/代码，不存在翻译问题，重复只会制造第二个漂移源。
2. **散文内容两段完整副本**，不混排。混排（每节里中英对照）对 README 这种"扫读型"文档是灾难：视觉噪音大、GitHub 目录只显示一份标题、锚点混乱。
3. **漂移用测试堵，不靠自觉**（`T10`）：

   | 断言 | 堵住什么 |
   |---|---|
   | 两段的 `cargo test` 用例数 == 源码 `#[test]` 计数 | `README.md:196` 的 "27" 与 `lessons.md:60` 的 "53" |
   | 两段出现的版本号 == `Cargo.toml` 的 `version` | 手改版本漏改文档 |
   | 两段 ` ```toml ` 块的 key 集合 == `Config` 的 serde 字段集合（从 `config.rs` 正则提取 `pub <name>:` 推导） | `spec.md:98-107` 与 `README.md:139-168` 的四字段缺失、`custom_sound_path = ""` 与 `Option<String>` 的类型不一致 |
   | 两段的 exe 体积声明 == 某个受控常量的值 | `spec.md:89` 的 "7 MB" vs 实测 9.75 MB |
   | 两个 `##` 标题序列（去掉语言名）结构相同 | 手改一端漏改另一端的结构 |
   | 文档里出现的每个 `docs/adr/*` 与 `docs/*` 相对链接真实存在 | 死链 |

4. **`docs/spec.md` 保持唯一事实来源**：配置项表、默认值、依据 URL 都以 `spec.md` 为准，README 两段是它的**投影**。`T10` 从 `config.rs`（真正的代码）而不是从 `spec.md` 取字段，因为代码才是唯一不会说谎的。
5. **不放 `.github/README.en.md` 双文件**：GitHub 不会自动切换语言，用户要手动点，且两个文件在 Issues/PR 模板和搜索上完全不互通。星标项目里用双文件的都是有 i18n 平台（GitHub Pages + Crowdin）的项目——EyeFlow 没有那个基础设施。

---

## 3. 性能与内存：v0.6 实施清单

按"风险调整后收益"排序。每条给出验证方式。

| # | 改动 | 收益（估算） | 风险 | 验证方式 |
|---|---|---|---|---|
| P1 | `install_fonts` 改为按语言惰性 + `OnceLock` 预热（`app.rs:477`） | 英文模式每次 UI 会话峰值工作集 **-18.8 MB**；冷启动浮窗不再有 18.8 MB 同步读盘卡顿（-20~40 ms） | 低 | `EYEFLOW_DEMO=1` 切语言后对比任务管理器"工作集"；`cargo test` 覆盖 `build_font_defs` 的选择逻辑 |
| P2 | 壁纸缓存上移到会话级共享（`ui.rs:21` + `app.rs:45` → `UiSession` 单份） | 严格模式首次进入不再有 150~400 ms 的同步解码卡顿；同图并存时 -14.7 MB CPU + -14.7 MB 显存 | 中（`SettingsState` 与 `UiSession` 的生命周期不同） | 设置窗选定壁纸 → 保持打开 → 触发长休息，观察是否还有解码停顿；`cargo test` 覆盖缓存命中/失效语义 |
| P3 | 轻量循环改 `MsgWaitForMultipleObjectsEx`（`main.rs:119`） | 托盘点击延迟 200 ms → ~0；空闲唤醒 5/s → ~1/s | **中**（见 D6） | 任务管理器"CPU"曲线 24 小时；自动点击托盘的响应主观测试；**必须连续跑满 24 小时 soak** |
| P4 | `Core::tick` 自门控到 1 Hz（`core.rs:258`） | 消除 B-2 的模式间节拍漂移；`FLOW_PAUSE` 判定抖动收敛 | 中（碰调度核心） | 现有 14 个 `core.rs` 测试**必须全绿且不改**——这是设计好的护栏 |
| P5 | `tray.set_paused` 加守卫（`tray.rs:166`） | 每步少 1 次 `SetMenuItemInfoW`（5/s 或 每帧） | 极低 | 照抄 `tray.rs:161` 的 `set_stats_line` 模式；`T9` |
| P6 | tooltip 先比后构造（`runtime.rs:240`） | 每步少 4~5 次堆分配 | 极低 | 复用 `tray.rs:176` 已有守卫；补一个单测 |
| P7 | `Vec<Action>` → `Option<Action>`（`core.rs:259,258` + `runtime.rs:226`）；`Tray::poll` 复用缓冲（`tray.rs:112`） | 每步少 2 次堆分配 | 极低（各分支至多 1 个 Action，已逐一核对） | `core.rs` 14 测试全绿 |
| P8 | `ui.rs:664` 的配置路径缓存进 `SettingsState` | 设置窗每帧少 4 次分配 | 极低 | — |
| P9 | `Config::save` / `Stats::save` 改原子写（`config.rs:332`, `stats.rs:45`） | 崩溃不再丢配置 | 极低 | `docs/research/08` #9 已给实现；`config.rs` 10 测试全绿 |
| P10 | 启动顺序：`AudioPlayer::new()` 移到托盘之后 / 后台线程（`main.rs:59` ↔ `:63`） | 托盘出现提前 5~50 ms | 低 | 测 `main` 到托盘首帧的毫秒数 |
| P11 | `audio.rs:194` 缓存 5 个基础图案（`OnceLock`） | 每次播报少 0.5~1.5 ms 主线程 | 极低 | `audio.rs` 8 测试全绿 |
| P12 | `Config::path()` 在 `app.rs:191` / `main.rs:133` 也缓存 | 每次 UI 会话少几次 env 查询 | 极低 | — |
| P13 | `app.rs:394-396` 装饰性行与 `ui.rs:220/293/412/...` 的全角空格缩进 → `ui.add_space` / `ui.indent` | 修掉字体回退与英文下的错位 | 极低 | 目视；`T4` 顺带会把 `"　"` 标出来 |
| P14 | `wallpaper.rs:95` 等错误信息改类型化（`enum ImageError`） | 英文界面不再弹中文 | 低 | — |

**明确不做**（避免把 v0.6 变成 v0.1）：

| 不做 | 理由 |
|---|---|
| 常驻 eframe / 常驻 GL | ADR-0006 实测 163 MB，已否决 |
| 多显示器遮罩 | 用户明确否决 |
| 浮窗改 Win32 自绘（GDI/Direct2D） | README Roadmap 里的大工程，收益是消除 ~60 MB 驱动残留；应作为独立的 v0.7 主题，且要先有 v0.6 的实测数据支撑"值不值得" |
| 构建期字体子集化 | 见 D4；先做 P1，看数据再决定 |
| Qt / UPX | 用户明确否决 |

---

## 4. UI 审美与功能完整性

### 4.1 设计令牌层（`src/theme.rs`，新增约 120 行）

把散落的字面量收成具名令牌，两套（Dark / Light）：

```
accent        强调色（代替 ui.rs:464 的 selection.bg_fill）
accent_text   强调色上的文字
success       语义绿：替代 ui.rs:167 rgb(70,170,100)
success_ink   该绿在 dark / light 主题下的**文字**变体（修 F-2 的对比度缺陷）
warning       语义橙：替代 ui.rs:168
warning_ink
danger        语义红：替代 ui.rs:169
danger_ink
on_accent     按钮上的文字
surface / surface_alt / border / text / text_muted
state_desktop / state_flow / state_gaming / state_away   （修 F-6）
```

- `success_ink` 的 light 变体取 `#1B7A3D`（对白底对比度约 4.6:1，满足 WCAG AA），dark 变体保持 `rgb(90,200,130)` 级别。
- `UiSession::new` 里 `ctx.style_mut_of(Theme::Dark, |s| s.visuals = visuals_for(Theme::Dark, &tokens))` 替换现有的 `tune_style`（`app.rs:467-474`）。
- 全部颜色**只在这一处**定义，删掉 `ui.rs:167-169` 与 `app.rs:167-169` 的重复。

### 4.2 字号阶梯与控件度量

| 层级 | 字号 | 字重 | 用途 |
|---|---|---|---|
| Display | 104 / 68 | Strong | 休息面板倒计时（`app.rs:423`） |
| Title | 26 / 20 | Strong | 面板主标题（`app.rs:416`） |
| Subtitle | 18 / 17 | Strong / Normal | 预告标题（`app.rs:295`）、闪屏 |
| Body | 15 | Normal | 贴士、设置项标签 |
| Caption | 12 | Normal | 倒计时说明（`app.rs:449`） |
| Mono | 15 | Normal | 配置文件路径（`ui.rs:664`）——`egui::TextStyle::Monospace` 已经在字体表里，**顺手修正它现在用错字体回退的问题** |

配套：`ui_zoom_pct`（`config.rs` 新字段 + `ctx.set_zoom()`，`sanitized()` 夹 80~150）满足 F-4。默认值 **100**（不变）。

### 4.3 设置窗的信息架构

1. **顶部固定一个"现在"状态条**（从 `status_card` 提出来，脱离 `ScrollArea`）：状态点（按 `state_*` 着色）+ `reminder_line` + 今日完成/跳过/延后 + `立即休息` 按钮。滚动时它始终可见——这是"我现在处于什么状态"唯一的答案。
2. **搜索框 + 分组折叠**：`ui.rs:98-119` 的 `ScrollArea` 换成"搜索框 → 命中的卡片才渲染"（约 30 个控件，搜索能把 6 张卡片收敛到 1~2 张）。
3. **语言选择器**放在"系统"卡片第一条（§2.5）。
4. **关闭确认**：`research/08` #10 指出的"点 X 静默丢弃未保存修改"在 v0.5.2 仍然存在（`app.rs:221-223` 直接 `rt.settings = None`）。虽然现在是"修改即保存"，`SettingsState::has_unsaved`（`ui.rs:40`）被标了 `#[allow(dead_code)]` 说明这个意识没消失。加一个轻量确认条。

### 4.4 功能完整性缺口（来自 `docs/research/07`，v0.6 能做的）

| 项 | 成本 | 说明 |
|---|---|---|
| #14 托盘进度环 | S | `icon.rs::render_rgba` 是纯函数，48×48 约 0.3 ms，每秒重绘一次毫无压力。E-2 证明运行时更新路径完全缺失，v0.6 顺手补上 |
| #12 自定义提示音试听截断 | S | D4 |
| #8 自定义贴士 | S | `tips.rs` 变成 i18n 表后，加"读 `%APPDATA%\eyeflow\tips.txt` 追加"是 20 行 |
| #1 电源/锁屏事件桥 | M | `research/07` Top 1 但从未做。`core.rs:267` 的 10 分钟 dt 阈值是兜底，不是替代。v0.6 若有余力再做，但**它不阻塞 v0.6 发布** |
| #3 多显示器 | — | **用户明确否决，不做** |

---

## 5. 候选决策清单（供 A/B 独立审查）

### D1 · i18n 引擎：自研 `tr!` vs `rust-i18n` vs `i18n-embed`/Fluent

- **选项 A**：自研 `tr!` 宏 + `build.rs` 代码生成 + `locales/*.toml` 纯数据源
- **选项 B**：`rust-i18n`（proc-macro + YAML）；或 C：`i18n-embed` + fluent feature
- **倾向 A。** 理由：(1) `opt-level="z"` 下 B/C 引入 250~800 KB（约 3~8% 二进制）换 6 KB 文案，与本项目"轻量"第一卖点不成比例；(2) B 的 `serde_yaml` 已被 dtolnay 正式归档弃用；(3) C 是**运行时解析**，漏 key 表现为界面上显示 `"tray.open_setting"`，而 A 能在 `build.rs` 里让构建直接失败——**A 在"编译期保证"这一维度上优于所有 crate**；(4) 只有 2 种语言、只有二元复数，`|` 分隔足以覆盖，fluent 的 6 形态复数是过度设计。
- **选错的最坏后果**：为 6 KB 文案写了 200 行基础设施；未来加第三种语言时发现二元复数不够用。但这 200 行与语言数据完全解耦，迁移成本 = 重写 `locales/*.toml`，**代码调用点零改动**。

### D2 · 语言表源格式：`locales/*.toml` + build.rs 生成 vs 单文件内联宏表

- **选项 A**：`locales/zh-CN.toml` + `locales/en-US.toml` 作为人类编辑的源，`build.rs` 生成 `Key` enum 与 `&[&str; N]` 数组
- **选项 B**：`src/i18n.rs` 里一个 `i18n! { "key" => "中文" | "English", ... }` 宏，一次写完
- **倾向 A。** 理由：翻译者/未来的自己改一个语言不需要在另一种语言的行之间来回跳；两语言天然同 key 相邻便于对齐；`build.rs` 的"缺 key 即构建失败"是 A 独有的强保证（B 要靠宏内的 `assert!` 数组长度，等价但更脆弱）。代价是 `Cargo.toml` 多一个 `toml` build-dependency（重复编译，**不影响二进制**）。
- **选错的最坏后果**：如果 `[build-dependencies] toml` 的重复编译时间不可接受，退路是 `build.rs` 纯 `lines()` 解析扁平子集——**这是一个已知的、已准备好的退路，不是死路**。

### D3 · 默认语言：`zh-CN` 固定 vs 首次运行跟随系统 locale

- **选项 A**：默认恒为 `zh-CN`（`d_language()`），用户在设置里手动切
- **选项 B**：全新安装时读 `GetUserDefaultLocaleName()` 作为初值；老配置一律 `zh-CN`
- **倾向 A。** 理由：用户明确要求"不要改动任何默认值来配合设计"，A 对现有用户的**行为改变为零**。B 虽然对英文用户更友好，但它引入一个"为什么这台机器首次运行是英文、那台是中文"的行为分叉，且与 `lessons.md:9` 记录的教训（"最后一公里没有验收"）同类——B 需要额外的 first-run 验收。
- **选错的最坏后果**：英文 locale 的新用户首次启动看到中文界面，必须找到语言开关。对策是在**首次运行的引导里**把语言选择器放第一条（`main.rs:84-89` 已经会自动开设置窗，成本近乎为零），并提供 `eyeflow.exe --lang en-US` 命令行覆盖。
- **附带讨论**：`build.rs:47` 的 `FileDescription = "EyeFlow 护眼提醒"` 出现在任务管理器"详细信息"与文件属性里，属于用户可见，但**它是构建期常量，不进 i18n 系统**。Windows 生态惯例是英文。建议直接改成 `"EyeFlow - Eye Care Reminder"`（纯 ASCII，资源脚本安全）。**这里我不确定**这算不算"改动文案"，所以单列出来由用户定。

### D4 · 中文字体：惰性加载 / 换更小的字重 / 构建期子集化

- **选项 A**：按语言惰性（英文不加载）+ `OnceLock` 预热（读盘挪出关键路径）；仍用 `msyh.ttc`
- **选项 B**：把候选顺序改成 `msyhl.ttc`（11.6 MB，Light 字重）以省 7.2 MB
- **选项 C**：构建期用 `fontations`/`write-fonts` 子集化出 ~100 KB 的内嵌字体
- **倾向 A，且**在 A 之后才评估 C。**否决 B**：Light 字重会让所有标题失去视觉层级，省 7.2 MB 的代价是全局观感退化——这与"从高星项目标准衡量审美"直接冲突。C 是唯一的量级级收益（18.8 MB → ~0.1 MB），但它是新增 build-dependency + 一条新的构建期失败模式（子集化后缺字会渲染成豆腐，而**缺字恰恰是 i18n 最怕的 bug**——英文表漏翻一条就会被 C 的子集"永远补不上"）。
- **选错的最坏后果**：选 A → 英文模式省下 18.8 MB，中文用户维持现状（可接受）。选 B → 审美退化，最坏情况是用户以为程序坏了。选 C 而子集化有遗漏 → 英文用户看到方块，**且因为字体是构建期嵌入的，运行时无法补救**。缓解：`T5`（英文表无 CJK）+ `T2`（占位符对齐）+ 构建期把"被 `tr!` 引用的所有字符"也纳入子集输入（需要一个 key→字符的静态推导，复杂度不低——这正是我把 C 排在最后的原因）。

### D5 · 错误文案边界：领域层返回本地化字符串 vs 返回类型化错误

- **选项 A**：把 `probe_custom` / `autostart::set_enabled` / `update::check` / `decode_texture` 的 `Result<_, String>` 改成 `Result<_, SomeEnum>`，UI 层用 `tr!` 渲染
- **选项 B**：只在 UI 层用 `tr!` 包一层，领域层继续返回中文 `String`
- **倾向 A。** 理由：B 会让 `runtime.rs:406`（`format!("播放失败：{e}")`）、`ui.rs:658`（`format!("检查失败：{e}")`）这类"用中文格式串包一层"的地方**必然漏翻**——这正是我们想消灭的 bug。类型化同时修好了 3 个断言中文子串的 audio 测试（`audio.rs:401/419/437`），是 A 独有的额外收益。代价是约 150 行、跨 4 个模块。
- **选错的最坏后果**：选 B 的最坏后果不是"测试挂了"（B 其实 0 个测试要改），而是**英文界面在出错路径上仍然弹中文**，而且这类 bug 只在出错时才出现、测试抓不到、最难被用户复现反馈。

### D6 · 主循环唤醒：`sleep(200ms)` 轮询 vs `MsgWaitForMultipleObjectsEx`

- **选项 A**：保留 5 Hz 轮询，只做 P5~P8 的"去掉无谓工作"
- **选项 B**：`MsgWaitForMultipleObjectsEx(hHandles=0, MWMO_INPUTAVAILABLE, timeout=1000ms)`，消息到达即泵、否则 1 Hz 醒一次做调度
- **倾向 B。** 理由：托盘点击的 200 ms 延迟是**用户能感知**的（`main.rs:30` 把它写成了设计参数），B 同时把它降到 ~0 并把空闲唤醒从 5/s 降到 1/s；两者在同一次改动里拿到。
- **选错的最坏后果**：**这是本次清单里风险最高的一条**。若 muda（tray-icon 0.24 的菜单层）或 `global-hotkey` 在 Windows 上并非完全经由主线程消息队列投递事件，`MsgWaitForMultipleObjectsEx` 就可能在有事件时也不返回 → 托盘/热键"假死"，且**只在特定菜单操作路径上出现**。我不确定 muda 在 Windows 上的确切投递方式（本次环境无网络，无法查 muda 0.x 源码确认）。因此：**必须先写一个 5 分钟的手工验证脚本（反复开关托盘菜单、反复按热键、观察日志时间戳），再上 24 小时 soak**。若验证失败，退路是 A + 把 tick 降到 1 Hz（仍然省 4/5 的唤醒，只是延迟不变）。

### D7 · 语言切换后刷新非 egui 表面的方式：逐项 `set_text` vs 重建托盘

- **选项 A**：`Tray::apply_language(lang)`，逐个 `MenuItem::set_text` + `set_tooltip`
- **选项 B**：销毁并重建整个 `TrayIcon`（重渲染图标 + 重建菜单）
- **倾向 A。** 理由：`tray.rs:151-172` 的 `set_hotkey_hint` / `set_paused` 已经是这个模式，零新概念；B 会让托盘图标**闪一下**（`Shell_NotifyIcon` 的 NIM_DELETE + NIM_ADD 序列有可见闪烁），并且要重跑 `icon::render_rgba`（E-2 说那是 0.3 ms，但闪烁的代价远大于 0.3 ms）。
- **选错的最坏后果**：选 A 而**忘了改某一项** → 托盘菜单中英混杂（例如菜单翻了但"退出"还是中文）。这类 bug 极难被自动发现。缓解是 `T9`（遍历 7 个菜单项断言与 `tr!` 一致）。**关键设计约束**：`TrayCommand` 的 `MenuId`（`tray.rs:19-26`）必须保持为**稳定的 ASCII 标识符**，永远不翻译——否则 `tray.rs:114` 的 `match ev.id.0.as_str()` 会在切语言后全部失配，**托盘彻底失效**。这是 A/B 都必须守住的红线。

### D8 · CI 严格度：解除 `continue-on-error` + 增加 GNU 矩阵

- **选项 A**：`cargo fmt --check` 与 `cargo clippy --all-targets -- -D warnings` 转为阻塞；CI 矩阵加 `windows-latest`（MSVC）与自托管/容器 GNU
- **选项 B**：保持 advisory，只加测试
- **倾向 A。** 理由：单维护者仓库没有"挡住外部贡献者"的顾虑，而 `lessons.md:61` 已经记录过 GNU/MSVC 差异导致"本地绿 CI 红"的真实事故；G-6 的两条 `continue-on-error` 注释写的是 v0.2 的 TODO，四轮迭代后仍在，说明"以后再说"已经失效。
- **选错的最坏后果**：A 若一次性把 clippy 告警全部变成错误，可能需要**一轮纯机械的清理**（预计 20~60 处），推迟发布。缓解：先在一个 `dev` 提交里把告警清零，再在下一个提交里打开门禁。GNU 矩阵的成本是 CI 时长翻倍 + 需要一个能跑 MinGW 的 runner（`windows-latest` 上装 MinGW 是一个 setup 步骤，约 2 分钟）。

### D9 · 审美投入范围：设计令牌层 + 排版阶梯 vs 只修最扎眼的

- **选项 A**：新增 `src/theme.rs`，做完整的语义色板（Dark/Light 双套）+ 6 级字号阶梯 + 替换 `tune_style`（约 120 行）
- **选项 B**：只修 F-2（对比度）与 F-3（全角空格缩进），不动色板与排版
- **倾向 A。** 理由：用户明确要求"从高星开源项目的标准去衡量……审美/UI 质量"。VS Code / Zed / GitHub 的共同点不是"某个具体颜色"，而是**具名语义令牌 + 两套主题 + 单一事实来源**；当前代码里 `rgb(70,170,100)` 在两个文件里各写一遍（`ui.rs:167` 与 `app.rs:167`），是"没有设计系统"的直接证据。B 只解决"能看见的 bug"，解决不了"改不动"的结构问题——下一次调色还是要改 4 个地方。
- **选错的最坏后果**：A 若过度设计（令牌粒度太细、引入不必要的抽象层）会拖慢后续迭代。对策：令牌表**只列当前 UI 真实用到的颜色**，不预留"以后可能用"的槽位；`theme.rs` 控制在 120 行以内，超过就说明在过度抽象。

---

## 6. v0.6 交付顺序（建议的落地批次）

| 批次 | 内容 | 依赖 |
|---|---|---|
| **b0 · 止血（不改行为）** | P5 / P6 / P7 / P8 / P9 / P13；G-1~G-5 的文档事实修正 | 无 |
| **b1 · i18n 地基** | `locales/*.toml` + `build.rs` 生成 + `src/tr.rs` + `Key` enum；T1/T2/T3/T4/T5/T6 | 无 |
| **b2 · i18n 落地** | 逐文件替换 152 处用户可见文案；D5 的错误类型化（含 3 个 audio 测试改造）；D7 的 `apply_language` + T9；`config.language` + T7/T8；P14 | b1、D5 |
| **b3 · 记忆** | P1（字体惰性 + 预热）；P2（壁纸共享缓存） | b2 的 `Lang` 就绪 |
| **b4 · 审美** | D9 的 `theme.rs`；§4.2 排版阶梯；`ui_zoom_pct`；F-5 搜索框；F-6 状态着色；F-8 工作区矩形 | b2 |
| **b5 · 唤醒（独立、可回滚）** | D6 的 P3 + P4；**单独提交，24 小时 soak 后才合并** | b3 |

**b5 单独成批的理由**：D6 是唯一有"可能让托盘/热键假死"风险且无法用测试覆盖的改动。它必须能被单独 revert，且必须在其他改动全部稳定后再上。

---

## 7. 验证清单（v0.6 合并前必须全绿）

```bat
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --locked          # 49 个原有 + 10 个新增 = 59
cargo build --release --locked
```

外加三项无法用 `cargo test` 覆盖的：

| 项 | 做法 |
|---|---|
| D6 的消息泵正确性 | 5 分钟手工脚本：反复开关托盘右键菜单、反复按 Ctrl+Shift+E、看 `%APPDATA%\eyeflow\eyeflow.log` 的时间戳是否与操作同步 |
| D6 的稳定性 | 24 小时 soak（不碰鼠标键盘），对比任务管理器 CPU 曲线与内存 |
| 字体路径 | 中/英两种语言各跑一次 `EYEFLOW_DEMO=1`，目视三块界面（设置窗 / 预告浮窗 / 休息面板）无豆腐块，且英文模式下任务管理器工作集显著低于中文模式 |

---

## 8. "这里我不确定"的清单

| # | 不确定的事 | 为什么 | 怎么定 |
|---|---|---|---|
| 1 | muda（tray-icon 0.24）在 Windows 上是否 100% 经主线程消息队列投递菜单事件 | 决定 D6 能否成立；本次环境无网络，读不到 muda 源码 | 查 muda 0.x 的 `windows.rs`；或直接跑 D6 的 5 分钟验证脚本 |
| 2 | egui 0.36 的 viewport 状态比对在"只改 `ViewportBuilder::with_title`"时是否下发 `ViewportCommand::Title` | 决定 D7/§2.5 步骤 3 要不要显式兜底 | 读 `egui-winit` 的 viewport state 同步代码；或切语言时目视设置窗标题栏 |
| 3 | `i18n-embed-fl` 是否是真实存在的、被广泛使用的 crate | 用户在需求里提到了它 | 查 crates.io；我未能确认 |
| 4 | `rust-i18n` 当前版本是否已摆脱 `serde_yaml` | 影响 B 方案的否决理由强度 | 查 `rust-i18n` 的 Cargo.toml 依赖树 |
| 5 | `[build-dependencies] toml` 重复编译的实际耗时 | 决定 D2 是否需要退路 | `cargo build --timings` 读一下 `build.rs` 的 `run` 行 |
| 6 | egui `default_fonts` 在 exe 里的实际占比（估 1.1~1.5 MB / 11~15%） | 决定要不要为体积做文章 | `cargo install cargo-bloat`（需要网络） |
| 7 | `msyh.ttc` 18.8 MB 在实际访问模式下有多少页真正常驻工作集 | 决定 P1 的收益是 18.8 MB 还是 8 MB | P1 落地后在任务管理器"内存"看会话前后差值 |
