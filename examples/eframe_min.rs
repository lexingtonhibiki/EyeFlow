//! 内存基线对照：一个什么都不做的 eframe(glow) 窗口。
//! `cargo run --release --example eframe_min`，然后用 Get-Process 看 PrivateMB。
struct Min;
impl eframe::App for Min {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.label("eframe baseline");
    }
}
fn main() -> eframe::Result {
    eframe::run_native(
        "eframe_min",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(Min))),
    )
}
