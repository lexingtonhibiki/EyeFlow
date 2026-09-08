# 跨平台移植与内存/维护成本评估

> 2026-09-08 · 回答三个问题:①移植其他平台,性能/内存能不能更小?②会不会更容易维护?③手头的 Qt 6.9 与 exe 压缩工具该不该用上?

## 1. 现状:这套代码其实已经"大半跨平台"

| 模块 | 占比 | 跨平台情况 |
|---|---|---|
| `core.rs` 调度核心 / `stats.rs` / `tips.rs` / `config.rs` | ~45% | **纯 Rust,零平台依赖**,直接复用 |
| `audio.rs`(rodio) | ~8% | rodio 后端 = WASAPI / CoreAudio / ALSA,自动适配 |
| `tray.rs`(tray-icon) / 热键(global-hotkey) | ~10% | 两库都官方支持 Win / macOS / Linux(Linux 托盘需 libappindicator 运行库) |
| `app.rs` + `ui.rs`(eframe/egui) | ~25% | eframe 支持 Win / macOS / Linux(X11);`apply_noactivate` 等焦点打磨需要 `#[cfg(windows)]` |
| `detector.rs` + `autostart.rs` + 单实例 | ~12% | **唯一的硬平台耦合**(Win32),需要逐平台重写 |

所以移植的正确姿势不是换框架,而是把 `detector.rs` 抽象成 `PlatformSensors` trait(空闲秒数 / 前台全屏 / 可打扰性三个方法),再逐平台实现:

| 平台 | 空闲 | 全屏/可打扰 | 预计工作量 |
|---|---|---|---|
| macOS | `CGEventSource.secondsSinceLastEventType` | `CGWindowListCopyWindowInfo`(层级 + 边界)+ NSWorkspace | 1~2 天 |
| Linux/X11 | XScreenSaver 扩展 / `org.freedesktop.ScreenSaver` | EWMH `_NET_WM_STATE_FULLSCREEN` | 2~3 天 |
| Linux/Wayland | 组合器相关,需 portal | 无统一协议,逐合成器适配 | 数天起,最难 |

**结论:继续用 Rust/egui 这条线移植,是工作量最小、维护面最小的路径**——一套语言、一套 UI 工具包、一份 27 个测试的核心逻辑;换框架等于把已经验证的东西全部重写一遍。

## 2. 内存还能不能更小?

先看清 200 MB UI 会话的构成(本机 NVIDIA 实测):

| 成分 | 量级 | 能否省 |
|---|---|---|
| OpenGL 上下文 + 显卡驱动堆 | ~120~150 MB | **换掉 GL 才能省**(见下) |
| 中文字体(msyh.ttc 整包读入) | ~20 MB | 可子集化到常用字形(~1~2 MB),省 ~18 MB |
| egui 默认字体 + 图集 | ~5~10 MB | 可关 default_fonts / 精简 |
| 常驻部分(托盘 + 音频 + 线程) | **2.5 MB 私有 / 16 MB 工作集** | 已接近 Rust 进程下限 |

按收益排序的做法:

1. **浮窗 / 休息面板改 Win32 自绘(GDI/Direct2D)**——已列入 README Roadmap。面板只有文字 + 进度 + 三个按钮,自绘约 300~400 行,可把 UI 会话从 ~200 MB 压到 ~30 MB 且无驱动残留;代价是 Windows 专有、少一套声明式 UI。
2. **字体子集化**(fonttools/pyftsubset 预生成 `assets/font-subset.ttf` 提交进仓库):省 ~18 MB,半小时工作量,跨平台同样受益。
3. 保持"按需会话"架构不变——这是空闲 16 MB 的根本,别动。

跨平台角度:macOS/Linux 的 GL 驱动堆通常比 NVIDIA/Windows 小(UI 会话约 60~100 MB),而常驻部分一样是 16 MB 级——**"按需会话"这个架构本身移植过去,就已经是各平台的省内存形态**。

## 3. Qt 6.9 值不值得换?

诚实对比(针对 EyeFlow 这种"后台常驻 + 罕见小窗口"形态):

| | 现方案(Rust + egui,按需会话) | Qt 6.9 重写(C++/Widgets 或 QML) |
|---|---|---|
| 空闲常驻 | **16 MB**(无窗口时零 GUI 依赖) | Qt 库从进程启动就得加载(QGuiApplication/QSystemTrayIcon),**~40~80 MB 起步**,做不出"无 UI 时零 GUI" |
| UI 会话 | ~200 MB(GL)/ 自绘后 ~30 MB | Widgets ~50~90 MB;QML 走 RHI→D3D,同样是 GPU 上下文 |
| 包体 | exe 7 MB | exe 20~40 MB(动态链接还需带 Qt DLL 或静态链增大) |
| 跨平台集成 | tray-icon/global-hotkey 已覆盖,Wayland 是短板 | **Qt 的强项**:QSystemTrayIcon、QScreen、D-Bus/portal 绑定成熟,Linux 桌面体验更稳 |
| 维护 | 单语言单工具链,27 个测试护住核心逻辑;代码量 ~3.5k 行 | 你熟 Qt 是加分项,但 C++ 内存/生命周期手工管理,且全部逻辑要重写移植 |
| 迁移成本 | —(已是现状) | 全量重写;核心逻辑可移植但 UI 全部重来 |

**结论:不建议为"省内存/易维护"换 Qt**——对这个形态 Qt 只会让空闲内存更高、包体更大。Qt 真正值得上的时机是:产品要长出"完整的桌面主窗口"(统计图表、列表、复杂表单),或你明确想统一到 C++/Qt 技术栈、且愿意接受常驻内存换开发效率。折中方案(Rust 核心 + cxx-qt 绑定 Qt UI)会同时引入两套工具链的复杂度,对这个体量是负资产。

## 4. exe 压缩工具(UPX 等):不建议

- **杀软误报**是主要代价:加壳的未签名 exe 是 Defender / SmartScreen 启发式的重点关照对象, winget 提交也会受影响;一旦误报,清理成本远高于 4 MB。
- **不省内存**:UPX 是自解压,运行时解包反而抬高私有内存。
- 安装包已经由 NSIS LZMA 压到 2.8 MB(源 exe 7.2 MB),收益只剩便携版那几 MB。
- 若坚持要更小的便携 exe:先做字体子集化 + 关 `default_fonts`(预计 ~5 MB),这比加壳安全得多。

## 5. 建议的路线

1. 短期(Windows 打磨):字体子集化 → 浮窗 Win32 自绘(把 UI 会话压到 ~30 MB)。
2. 跨平台:抽 `PlatformSensors` trait → macOS 实现 → Linux/X11;发布以 GitHub Actions matrix 一并出包。
3. Qt 不引入;UPX 不使用。
