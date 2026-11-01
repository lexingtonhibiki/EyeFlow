import Foundation
import Combine
import UserNotifications

struct DailyStats: Codable {
    var completed: Int = 0
    var postponed: Int = 0
    var skipped: Int = 0
    var activeSeconds: Double = 0
}

/// iOS intentionally does not claim access to global screen use or lock state.
/// All scheduler time advances only while our scene is active.
@MainActor
final class EyeFlowModel: ObservableObject {
    @Published private(set) var phase: Int32 = 0
    @Published private(set) var remaining: UInt64 = 0
    @Published private(set) var active = false
    @Published private(set) var paused = false
    @Published private(set) var today = DailyStats()
    @Published var focus = false {
        didSet { refresh(); UserDefaults.standard.set(focus, forKey: "eyeflow.focus") }
    }
    private var engine: OpaquePointer?
    private var timer: Timer?
    private var elapsed: Double = 0
    private var previousUptime: TimeInterval?
    private var pauseUntil: Double?
    private var lastCompleted: UInt32 = 0
    private var lastPostponed: UInt32 = 0
    private var stats: [String: DailyStats] = [:]
    private var day = ""
    private var saveCounter = 0

    init() {
        engine = eyeflow_create()
        if let data = UserDefaults.standard.data(forKey: "eyeflow.stats"),
           let stored = try? JSONDecoder().decode([String: DailyStats].self, from: data) {
            stats = stored
        }
        focus = UserDefaults.standard.bool(forKey: "eyeflow.focus")
        if let snapshot = UserDefaults.standard.data(forKey: "eyeflow.engine"), let engine {
            let success = snapshot.withUnsafeBytes { bytes in
                eyeflow_import_state(engine, bytes.bindMemory(to: UInt8.self).baseAddress, bytes.count)
            }
            if success == 0 {
                lastCompleted = eyeflow_completed_count(engine)
                lastPostponed = eyeflow_postponed_count(engine)
                let pause = UserDefaults.standard.double(forKey: "eyeflow.pauseRemaining")
                if pause > 0 { paused = true; pauseUntil = pause }
            }
        }
        rotateDay()
        refresh()
    }

    deinit {
        timer?.invalidate()
        if let engine { eyeflow_destroy(engine) }
    }

    var clock: String {
        let seconds = paused ? UInt64(max(0, (pauseUntil ?? elapsed) - elapsed).rounded(.up)) : remaining
        return String(format: "%02llu:%02llu", seconds / 60, seconds % 60)
    }
    var title: String {
        if paused { return "已暂停，慢慢来" }
        if focus { return "专注中，休息不会被丢掉" }
        switch phase {
        case 1: return "准备给眼睛放个假"
        case 2: return "看向远方，轻轻呼吸"
        default: return "留一点时间给眼睛"
        }
    }
    var subtitle: String {
        if paused { return "暂停仅按应用前台时间计时，可随时恢复" }
        if focus { return "关闭专注后，仍会安排到期的休息" }
        switch phase {
        case 1: return "收好手边的事，即将开始休息"
        case 2: return "望向窗外，放松肩膀。你不需要一直盯着倒计时。"
        default: return "距下次休息 · 只累计 EyeFlow 前台使用时间"
        }
    }

    func setActive(_ value: Bool) {
        guard value != active else { return }
        if !value { advance() }
        active = value
        timer?.invalidate()
        timer = nil
        // Never leave a background or lock-screen reminder behind.
        UNUserNotificationCenter.current().removeAllPendingNotificationRequests()
        UNUserNotificationCenter.current().removeAllDeliveredNotifications()
        previousUptime = value ? ProcessInfo.processInfo.systemUptime : nil
        if value {
            rotateDay()
            refresh()
            timer = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] _ in
                Task { @MainActor in self?.advance() }
            }
        } else {
            persist()
        }
    }

    private func advance() {
        guard active else { return }
        let now = ProcessInfo.processInfo.systemUptime
        let delta = max(0, now - (previousUptime ?? now))
        previousUptime = now
        rotateDay()
        elapsed += delta
        today.activeSeconds += delta
        if let until = pauseUntil, elapsed >= until {
            paused = false
            pauseUntil = nil
        }
        refresh()
        saveCounter += 1
        if saveCounter >= 5 { persist(); saveCounter = 0 }
    }

    private func refresh() {
        guard let engine else { return }
        phase = eyeflow_tick(engine, UInt64(max(0, elapsed) * 1000), 0, focus ? 1 : 0)
        remaining = eyeflow_remaining_seconds(engine)
        let completed = eyeflow_completed_count(engine)
        let postponed = eyeflow_postponed_count(engine)
        if completed >= lastCompleted { today.completed += Int(completed - lastCompleted) }
        if postponed >= lastPostponed { today.postponed += Int(postponed - lastPostponed) }
        lastCompleted = completed
        lastPostponed = postponed
    }

    func command(_ command: UInt32) {
        guard active, let engine else { return }
        advance() // C ABI commands use the most recently supplied tick timestamp.
        eyeflow_command(engine, command)
        if command == 3 { today.skipped += 1 }
        if command == 4 { paused = true; pauseUntil = elapsed + 3600 }
        if command == 5 { paused = false; pauseUntil = nil }
        refresh()
        persist()
    }

    private func rotateDay() {
        let parts = Calendar.current.dateComponents([.year, .month, .day], from: Date())
        let key = String(format: "%04d-%02d-%02d", parts.year ?? 0, parts.month ?? 0, parts.day ?? 0)
        guard key != day else { return }
        if !day.isEmpty { stats[day] = today }
        day = key
        today = stats[key] ?? DailyStats()
    }

    private func persist() {
        if let engine {
            let size = eyeflow_export_state(engine, nil, 0)
            if size > 0 && size < 1_048_576 {
                var snapshot = Data(count: size)
                let written = snapshot.withUnsafeMutableBytes { bytes in
                    eyeflow_export_state(engine, bytes.bindMemory(to: UInt8.self).baseAddress, size)
                }
                if written == size { UserDefaults.standard.set(snapshot, forKey: "eyeflow.engine") }
            }
        }
        UserDefaults.standard.set(max(0, (pauseUntil ?? elapsed) - elapsed), forKey: "eyeflow.pauseRemaining")
        stats[day] = today
        if let data = try? JSONEncoder().encode(stats) {
            UserDefaults.standard.set(data, forKey: "eyeflow.stats")
        }
    }
}
