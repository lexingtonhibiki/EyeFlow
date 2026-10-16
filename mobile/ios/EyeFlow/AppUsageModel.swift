import SwiftUI

/// Informational only: no picker, authorization request, monitor or shield toggle.
struct AppUsageView: View {
    var body: some View {
        List {
            Section("跨应用连续使用提醒：暂不支持") {
                Text("你要的是连续使用目标 App 后提醒，真正锁屏休息足够久再重置，而不是每天累计使用时长。")
                Text("本版尚无经过验证、受支持的跨应用连续使用与实际锁屏时间线实现，因此不启用这项功能，也不以每日累计阈值代替。")
                Text("实际锁屏休息至少 30 秒重置短休息、至少 15 分钟重置长休息的规则，目前无法在 iOS 上可靠落实。离开 EyeFlow 不能视为已经锁屏休息。")
            }
            Section("仍可主动休息") {
                Text("你可以在 EyeFlow 内主动开始休息、推迟、跳过或暂停。此页前台计时只是应用内辅助工具，不是跨应用连续使用检测。")
                Text("锁屏、切换应用或系统打断时，应用内倒计时暂停；回来后继续。不发送锁屏通知，不自动遮住其他应用。")
            }
        }
        .navigationTitle("iOS 功能边界")
    }
}
