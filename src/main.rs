// В релизной сборке под Windows не показываем чёрное окно консоли.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod analysis;
mod app;
mod clips;
mod library;
mod model;
mod parse;
mod report;

fn main() -> eframe::Result<()> {
    // Если демку перетащили на exe, откроем её сразу.
    let start = std::env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .filter(|p| p.exists());

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 760.0])
            .with_min_inner_size([820.0, 560.0])
            .with_title("CS2 Helper"),
        ..Default::default()
    };

    eframe::run_native(
        "CS2 Helper",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, start)))),
    )
}
