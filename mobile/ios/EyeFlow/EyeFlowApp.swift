import SwiftUI

@main
struct EyeFlowApp: App {
    @StateObject private var model = EyeFlowModel()
    @Environment(\.scenePhase) private var scenePhase

    init() {
        LegacyUsageCleanup.disable()
    }

    var body: some Scene {
        WindowGroup {
            ContentView(model: model)
                .onChange(of: scenePhase, initial: true) { _, phase in
                    if phase == .active { LegacyUsageCleanup.disable() }
                    model.setActive(phase == .active)
                }
        }
    }
}
