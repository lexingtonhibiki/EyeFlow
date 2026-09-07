---
status: accepted
date: 2026-09-08
revised: 2026-09-08（实测内存后修订：由“常驻”改为“按需会话”）
---

# UI 宿主采用按需启动的 eframe 会话（隐藏锚点根视口 + 子视口），空闲时不持有任何 GL 上下文

v0.1 在主循环里按需调用 `eframe::run_native` 打开设置窗，但核心逻辑不在 eframe 内运行，`run_native` 一阻塞，托盘、提醒、状态机全部停摆；主线程还跑着一个自写的 `PeekMessageW` 泵与 winit 抢消息。最初的 v0.2 方案是让 eframe（0.36，glow）常驻，根视口隐藏、逻辑在 `App::logic` 中按 1 Hz 推进。**实测推翻了它**：在 NVIDIA 显卡的 Windows 机器上，一个什么都不画的空 eframe 窗口就占 163 MB 私有内存（GL 上下文 + 驱动堆），EyeFlow 常驻达 203 MB——与被用户诟病的 Electron 竞品同一量级，直接摧毁“Rust 轻量”这个差异化卖点。

最终方案：
- 主线程平时跑一个**轻量循环**（`PeekMessageW` 泵 + 200 ms 轮询），没有任何窗口和 GL 上下文；托盘（tray-icon）、全局热键（global-hotkey）在启动时于主线程创建，两种模式下都能收到消息。
- 只有当需要显示窗口（预告浮窗 / 休息面板 / 设置窗）时才调用 `eframe::run_native`，进入 **UI 会话**；eframe 的 `run_and_return`（默认开启）把 winit 事件循环缓存在线程局部变量里并用 `run_app_on_demand` 运行，因此可以在同一线程反复进入、退出。
- 会话期间根视口是一个 1×1、放在屏幕外的锚点窗口，只为让 egui pass 每帧运行；三种窗口都是子视口。浮窗每次出现都换新的 `ViewportId`（新的原生窗口），因为 winit 只在窗口**首次**显示时尊重 `with_active(false)`；再叠加 `WS_EX_NOACTIVATE`，做到“看得见、点得动、不抢焦点”。
- 所有窗口关闭后会话结束，GL 上下文释放，回到轻量循环。
- 核心逻辑（`Runtime::step`）在两种模式下调用同一份代码，因此无论窗口开不开，提醒计时、托盘与热键都不会停摆。

## 实测（Windows 11，NVIDIA 独显，2026-09-08）

| 状态 | 工作集 | 私有内存 |
|---|---|---|
| 空闲，从未打开过窗口 | 15.6 MB | 2.5 MB |
| UI 会话期间（休息面板 + 设置窗） | ~175 MB | ~200 MB |
| 会话结束后空闲（驱动堆残留） | ~87 MB | ~60 MB |

spec N1 维持“<30 MB 常驻（未打开窗口时）”，并如实标注会话后的残留。

## Considered Options

- 常驻 eframe（最初方案）：架构最简单，但 160~200 MB 常驻不可接受。
- 浮窗脱离 eframe 用 Win32 自绘（GDI/Direct2D）：内存可到 ~20 MB 且无会话切换，但要维护两套渲染与输入路径；列为 Roadmap，若驱动残留成为用户投诉点再做。
- wgpu 渲染器：不会更省，且显著增加二进制体积与编译时间。

## Sources

- docs/research/05-rust-stack-upgrade.md（tray-icon “创建线程 = 消息泵线程”、eframe 0.34 默认 wgpu、多视口模式）
- eframe `NativeOptions::run_and_return` / `run_app_on_demand`: https://docs.rs/eframe/latest/eframe/struct.NativeOptions.html
- winit `WindowAttributes::with_active` 语义（仅首次显示）: https://docs.rs/winit/latest/winit/window/struct.WindowAttributes.html#method.with_active
- Extended Window Styles（WS_EX_NOACTIVATE）: https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles
