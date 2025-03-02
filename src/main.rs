#![windows_subsystem = "windows"] // Hide console window on Windows; ignored on macOS

mod app;
mod model;

use crate::app::ThetaApp;
use eframe::egui::Vec2;

fn main() {
    // Optional: load an icon
    let icon_bytes = {
        #[cfg(target_os = "windows")]
        {
            include_bytes!("../resources/Win_App_Icon.png")
        }
        #[cfg(not(target_os = "windows"))]
        {
            include_bytes!("../resources/Mac_App_Icon.png")
        }
    };

    // Attempt to decode the icon
    let image = image::load_from_memory(icon_bytes).unwrap_or_else(|_| {
        image::DynamicImage::new_rgba8(16, 16) // fallback if loading fails
    });
    let image = image.to_rgba8();
    let (width, height) = image.dimensions();
    let icon_data = eframe::IconData {
        rgba: image.into_raw(),
        width,
        height,
    };

    // Configure eframe
    let native_options = eframe::NativeOptions {
        // 1) Start at 300×300
        initial_window_size: Some(Vec2::new(300.0, 300.0)),
        // 2) Minimum window size also 300×300
        min_window_size: Some(Vec2::new(300.0, 300.0)),
        // Allow resizing
        resizable: true,
        // Set icon
        icon_data: Some(icon_data),
        ..Default::default()
    };

    // Launch the GUI
    eframe::run_native(
        "ThetaData Terminal GUI",
        native_options,
        Box::new(|_cc| Box::new(ThetaApp::new())),
    )
    .unwrap();
}
