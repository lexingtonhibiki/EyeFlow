# Android 0.3.0 后续：运行中投递许可撤回

本次从已交付 `EyeFlow-Android-0.3.0-review-bundle.zip` 的完整源码恢复，不回退到 0.2.x，不重写 Windows 或 iOS。这个增量仍是源码修复，不是已经重建/签名的新版 APK。

## 问题与边界

旧 0.3.0 在显示中央小卡时检查勿扰，但展示期间未把勿扰纳入撤回条件。用户在小卡展示的十秒内打开勿扰，卡片可能继续留到超时。旧通知许可判断始终检查到点通道，而非当前预告通道；当前预告通道被关闭时，应用自身的展示状态也应撤回。

这次只统一当前阶段的投递许可和小卡许可：检查系统通知许可、使用记录许可、当前通道、锁屏/前台证据与核心待提醒状态；小卡另需用户开关、悬浮授权及勿扰关闭。许可变化只影响展示，不记休息，不清除用屏负担，不升级成全屏。

## 调度与权限边界

不申请“勿扰访问”权限。Android Q 起，`ACTION_INTERRUPTION_FILTER_CHANGED` 广播的投递受到该权限约束，因此只把可收到的广播作为加速信号，仍须依赖当前会话的一秒检查与最终显示前检查。系统挂起或厂商调度可能造成延迟，不能称为零竞态或保证实时。

系统通知继续遵守用户的通知通道与勿扰策略；勿扰开启会撤下自绘小卡，不强行替系统决定通知是否可保留。恢复许可时，同轮通知只能静默恢复，不能重复震动或再弹小卡。

官方依据：
- https://developer.android.com/reference/android/app/NotificationManager#ACTION_INTERRUPTION_FILTER_CHANGED
- https://developer.android.com/reference/android/app/NotificationManager#ACTION_NOTIFICATION_CHANNEL_BLOCK_STATE_CHANGED

## 验证记录

最终执行结果见本次交付验证记录。历史 0.3.0 的 APK 编译、53 项 Rust 测试与模拟器截图仅证明历史版本，不证明这个增量已经通过 Android SDK 编译或运行。

当前云环境不含 Android SDK/NDK、Gradle 或模拟器；官方 Google Maven 返回 `403 blocked-by-allowlist`。不绕过访问限制，不关闭 TLS 检查。恢复源码的锁定 Rust 依赖也有未缓存项，不能把不同锁定版本的离线补充验证冒充正式锁定构建。

本轮没有连接用户电脑，没有推送 GitHub，没有创建远端发布，也没有生成新的签名密钥。

## 本次最终结果

- 通过：生产 Kotlin 的 UsageClock、ReminderDelivery、新 ReminderPresentationPolicy 编译及行为回归。
- 通过：53 项 Rust 测试与严格 Clippy、C/C++ ABI、Kotlin→JNI→Rust（均为独立副本使用缓存兼容 patch 依赖的补充验证）。新增测试确认勿扰不会清除使用负担或伪造休息完成。
- 通过：当前服务门禁接线静态检查、iOS 禁用累计监测静态检查、shell 语法与 diff 空白检查。
- 未通过验证：原始 Cargo.lock 完整构建因未缓存依赖下载受限；本次完整 Android SDK 编译/lint、设备运行未执行，不能用作发布完成证明。
- 未产出新 APK；原调试签名密钥不在此环境，没有生成替代密钥。旧 APK 不包含这次修复。

详细日志、源文件摘要、依赖差异、工具来源与当时环境的复跑脚本保存在单独交付的 `EyeFlow-live-guards-verification-evidence.zip`，不作为源码依赖。完整依赖正常可用时，应先运行项目原有 `mobile/android/tests/run-host-tests.sh`，再执行 APK 构建/lint 与真机验收，不应将补充 lockfile 替换项目 lockfile。
