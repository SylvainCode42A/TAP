use eframe::egui;

/// State of the GUI window.
struct TapApp;

impl eframe::App for TapApp {
    /// Called on every frame: draws the whole window from the current state.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("TAP");
            ui.label("Not connected");
        });
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions::default();
    eframe::run_native("TAP", options, Box::new(|_cc| Ok(Box::new(TapApp))))
}
