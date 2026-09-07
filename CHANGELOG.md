# Changelog

本项目的所有显著变更都将记录在本文件中。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/),版本号遵循 [SemVer](https://semver.org/)。

## [0.2.0] - Unreleased

v0.2 是一次整体重写:从"一声提示音的空壳"补全为完整的"预告 → 休息 → 结算"闭环,并确立"提醒只顺延、不消失"的上下文规则(实现计划见 `docs/plan-v0.2.md`)。

### Added

- 完整提醒链路:预告浮窗(右下角、不抢焦点,可选 现在开始 / 延后 5 分钟 / 跳过)→ 休息界面(居中倒计时 + 一条 AOA/AAO 权威护眼贴士,倒计时结束自动完成并播结束音)
- 长休息:连续用屏累计 2 小时后安排 15 分钟长休息,默认开启(ADR-0001)
- 坚持统计:今日完成(短/长/自然休息)、跳过、延后次数与连续坚持天数,持久化于 `%APPDATA%\eyeflow\stats.toml`
- 全局热键 Ctrl+Shift+E = 立即休息(固定,不可配置)
- 上下文感知补全:心流中提醒顺延到输入停歇 8 秒后完整投递;游戏/系统不可打扰(SHQueryUserNotificationState)时仅响提示音,退出后补发预告(ADR-0002/0004)
- 严格模式(可选,默认关):休息界面改为全屏暗色遮罩
- 5 种合成提示音预设:gentle_chime / soft_tap / water_drop / digital_drop / triple_beep
- 开机自启开关:安装默认开启,设置界面可开关(读写 HKCU\Run;便携运行默认不写)(ADR-0005)
- 单实例互斥(二次启动直接退出);托盘 tooltip 显示当前状态与下次休息时间;设置界面恢复默认与参数校验
- v0.1 配置自动迁移:用户改过的间隔 / 免打扰时段带过来,旧默认值换成新默认值,原文件备份为 `config.toml.bak-v0.1-<时间戳>`
- `EYEFLOW_DEMO=1` 演示模式(启动即打开设置窗,60 秒后触发一次提醒),便于验收与截图
- 日志写入 `%APPDATA%\eyeflow\eyeflow.log`;27 个单元测试覆盖调度核心、配置迁移、统计与音频合成

### Changed

- 默认提醒节奏改为 AOA 20-20-20:短休息在 15~25 分钟三角随机(峰值 20 分钟)、持续 30 秒,取代 v0.1 的 10~18 分钟 / 25 秒(ADR-0001)
- UI 宿主改为按需启动的 eframe(glow)会话:平时是无窗口、无 GL 上下文的轻量循环,托盘与热键在两种模式下都由主线程消息泵驱动;空闲常驻实测约 16 MB(ADR-0006)
- 离开判断改用 GetLastInputInfo,同时计入鼠标与键盘;离开改为暂停计时而非反复重置,进入离开状态即视为一次自然休息
- 配置解析失败时改名备份为 `config.toml.bak-corrupt-<时间戳>` 后以默认值重建,不再直接删除
- exe 嵌入图标与版本资源,任务管理器启动项显示友好名称
- 依赖升级:eframe/egui 0.36(glow)、toml 1.x;新增 global-hotkey 0.8 与 winresource;rodio 裁剪解码器;移除死依赖 directories

### Fixed

- 心流状态下到点提醒被静默吞掉并重置计时——现顺延到停歇 8 秒后完整投递(P0-1)
- 游戏全屏时提醒完全静默——现到点立即播提示音,视觉部分退出全屏后补发(P0-2)
- "系统通知"只是日志占位、无倒计时、无完成/跳过——替换为预告浮窗与休息界面(P0-3)
- 全局热键 Ctrl+Shift+E 从未注册——现已通过 global-hotkey 实现(P0-4)
- 无音频设备时直接崩溃——音频初始化失败降级为静音并告警(P0-5)
- 只用键盘判断"离开",鼠标用户 3 分钟即被判离开——改用 GetLastInputInfo(含鼠标键盘)

## [0.1.0] - 2026-05-17

初版:

- 系统托盘常驻与右键菜单、加权随机定时提醒、rodio 合成提示音
- 四状态上下文机(桌面/心流/游戏/离开)雏形、全屏与空闲检测
- egui 设置窗口与 `%APPDATA%\eyeflow\config.toml` 配置持久化
- NSIS 安装包(HKCU\Run 自启 + 卸载清理)

[0.2.0]: https://github.com/lexingtonhibiki/eyeflow/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/lexingtonhibiki/eyeflow/releases/tag/v0.1.0
