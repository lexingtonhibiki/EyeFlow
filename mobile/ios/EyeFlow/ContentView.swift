import SwiftUI

private let ink = Color(red: 0.18, green: 0.27, blue: 0.23)
private let moss = Color(red: 0.36, green: 0.48, blue: 0.35)
private let paper = Color(red: 0.97, green: 0.96, blue: 0.92)

struct ContentView: View {
    @ObservedObject var model: EyeFlowModel
    @State private var showAbout = false

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 25) {
                    HStack {
                        Label("EyeFlow", systemImage: "leaf.fill")
                            .font(.title2.bold())
                        Spacer()
                        Button { showAbout = true } label: {
                            Image(systemName: "info.circle").font(.title3)
                        }
                        .accessibilityLabel("使用说明与平台限制")
                    }
                    HStack(spacing: 6) {
                        Circle().fill(model.active ? moss : .gray).frame(width: 7, height: 7)
                        Text("一段专注 · 一次远望").font(.caption.weight(.medium))
                    }
                    .padding(.horizontal, 15).padding(.vertical, 8)
                    .background(moss.opacity(0.09), in: Capsule())
                    VStack(spacing: 10) {
                        Text(model.title).font(.title2.bold()).multilineTextAlignment(.center)
                        Text(model.subtitle).font(.subheadline).foregroundStyle(.secondary)
                            .multilineTextAlignment(.center).frame(maxWidth: 320)
                    }
                    ZStack {
                        Circle().stroke(moss.opacity(0.12), lineWidth: 2)
                        Circle().stroke(moss.opacity(0.12), lineWidth: 16).padding(16)
                        VStack(spacing: 10) {
                            Image(systemName: model.phase == 2 ? "sun.horizon" : "leaf")
                                .font(.system(size: 32, weight: .light))
                            Text(model.clock)
                                .font(.system(size: 49, weight: .light, design: .rounded))
                                .monospacedDigit().minimumScaleFactor(0.7).lineLimit(1)
                            Text(model.paused ? "暂停剩余" : model.phase == 2 ? "休息剩余" : "下一次远望")
                                .font(.caption).tracking(3)
                        }
                        .padding(30)
                    }
                    .frame(maxWidth: 280).aspectRatio(1, contentMode: .fit)
                    .accessibilityElement(children: .combine)
                    VStack(spacing: 12) {
                        if model.paused {
                            primaryButton("恢复提醒", icon: "play.fill") { model.command(5) }
                        } else if model.phase == 2 {
                            primaryButton("跳过这次休息", icon: "forward.end") { model.command(3) }
                        } else {
                            primaryButton("现在休息一下", icon: "sun.max") { model.command(1) }
                        }
                        if !model.paused {
                            HStack {
                                Button("推迟 5 分钟") { model.command(2) }
                                Spacer()
                                Button("暂停 1 小时") { model.command(4) }
                            }
                            .font(.subheadline).padding(.horizontal, 10)
                        }
                    }
                    .frame(maxWidth: 360)
                    NavigationLink { AppUsageView() } label: {
                        Label("跨应用连续提醒暂不可用 · 了解原因", systemImage: "info.circle")
                            .font(.footnote).frame(maxWidth: .infinity).padding()
                    }
                    Toggle(isOn: $model.focus) {
                        VStack(alignment: .leading, spacing: 4) {
                            Text("先让我专注").font(.subheadline.weight(.semibold))
                            Text("延后到期提醒，不悄悄取消")
                                .font(.caption).foregroundStyle(.secondary)
                        }
                    }
                    .tint(moss).padding(18)
                    .background(.white.opacity(0.65), in: RoundedRectangle(cornerRadius: 20))
                    VStack(alignment: .leading, spacing: 15) {
                        Text("今天的小小积累").font(.subheadline.bold())
                        HStack {
                            metric("完成休息", "\(model.today.completed)")
                            Spacer()
                            metric("主动推迟", "\(model.today.postponed)")
                            Spacer()
                            metric("前台分钟", "\(Int(model.today.activeSeconds / 60))")
                        }
                        Text("保存在这台设备上，无需账号或网络。")
                            .font(.caption2).foregroundStyle(.secondary)
                    }
                    .padding(20).background(.white.opacity(0.65), in: RoundedRectangle(cornerRadius: 20))
                    Label("此页倒计时在离开应用或锁屏时暂停。", systemImage: "lock.shield")
                        .font(.caption).foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                }
                .padding(24).frame(maxWidth: 560).frame(maxWidth: .infinity)
            }
            .background(paper.ignoresSafeArea()).foregroundStyle(ink)
            .toolbar(.hidden, for: .navigationBar)
            .sheet(isPresented: $showAbout) { about }
        }
        .tint(moss)
    }

    private func primaryButton(_ text: String, icon: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Label(text, systemImage: icon).font(.headline)
                .frame(maxWidth: .infinity).padding(.vertical, 17)
        }
        .buttonStyle(.plain).foregroundStyle(.white)
        .background(moss, in: Capsule())
    }

    private func metric(_ title: String, _ value: String) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(value).font(.title2.weight(.medium)).monospacedDigit()
            Text(title).font(.caption).foregroundStyle(.secondary)
        }
    }

    private var about: some View {
        NavigationStack {
            List {
                Section("温和的节奏") {
                    Text("短休息：使用约 15–25 分钟后休息 30 秒。")
                    Text("长休息：累计使用约 2 小时后休息 15 分钟。")
                    Text("你可以随时推迟、跳过或暂停。专注模式只延后提醒，不丢掉到期的休息。")
                }
                Section("iPhone 与 iPad 的边界") {
                    Text("此页倒计时仅用于 EyeFlow 内主动休息，不代表全手机或其他 App 的连续使用时间。跨应用连续使用及真实锁屏休息重置，目前无法可靠实现，因此不可用。")
                    Text("此页在离开应用、锁屏或系统打断时暂停计时，回来后继续。不会把离开应用当成实际锁屏休息，不用每日累计阈值替代，不自动遮罩其他应用，也不发送锁屏通知。")
                    Text("休息倒计时同样只在前台推进。离开应用不会被自动记作完成休息。")
                }
                Section("数据与你") {
                    Text("计时状态和按日统计保存在本机。无账号、无云同步、无健康数据采集。统计只描述应用内的计时，不代表实际屏幕使用量或医学效果。")
                    Text("完成休息指应用前台倒计时结束；推迟和跳过分别记录。")
                }
            }
            .navigationTitle("关于 EyeFlow")
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("完成") { showAbout = false } } }
        }
    }
}
