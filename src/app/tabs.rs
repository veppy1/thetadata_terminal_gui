use super::ThetaApp;
use crate::model::Tab;
use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{self, Color32, FontId, Galley, ScrollArea, TextEdit, Ui, Vec2};
use rfd::FileDialog;
use std::sync::Arc;

//
// ────────────────────────────────────────────────────────────────────────────
//   :: Tab 1: Setup
// ────────────────────────────────────────────────────────────────────────────
//
pub fn show_setup_tab(app: &mut ThetaApp, ui: &mut Ui) {
    egui::CollapsingHeader::new("⌨ Login Credentials")
        .default_open(true)
        .show(ui, |ui| {
            if app.credentials_saved {
                ui.horizontal(|ui| {
                    ui.label("Username (saved):");
                    ui.monospace(&app.username_input);
                });
                ui.label("Password stored in keychain.");
                if ui.button("Remove all credentials").clicked() {
                    app.remove_credentials();
                }
            } else {
                ui.horizontal(|ui| {
                    ui.label("Username:");
                    ui.add(
                        TextEdit::singleline(&mut app.username_input)
                            .desired_width(ui.available_width() - 8.0),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Password:");
                    ui.add(
                        TextEdit::singleline(&mut app.password_input)
                            .password(true)
                            .desired_width(ui.available_width() - 8.0),
                    );
                });
                if ui.button("Save Credentials").clicked() {
                    app.save_credentials();
                }
            }
        });

    ui.add_space(16.0);

    egui::CollapsingHeader::new("⚙ ThetaTerminal Configuration")
        .default_open(true)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("ThetaTerminal.jar Path:");
            });
            ui.horizontal(|ui| {
                ui.add(
                    TextEdit::singleline(&mut app.jar_path)
                        .desired_width(ui.available_width() - 60.0),
                );
                if ui.button("Browse").clicked() {
                    if let Some(file) = FileDialog::new()
                        .add_filter("JAR Files", &["jar"])
                        .pick_file()
                    {
                        app.jar_path = file.to_string_lossy().to_string();
                    }
                }
            });
            ui.checkbox(
                &mut app.auto_start,
                "Start ThetaData Terminal on app launch",
            );
        });

    ui.add_space(8.0);

    egui::CollapsingHeader::new("☑ Terminal Controls")
        .default_open(true)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Start").clicked() {
                    app.start_terminal();
                }
                if ui.button("Stop").clicked() {
                    app.stop_terminal();
                }
                if ui.button("Reset").clicked() {
                    app.reset_terminal();
                }
            });
            ui.horizontal(|ui| {
                ui.label("Status:");
                if app.process.is_some() {
                    ui.strong("Running");
                } else {
                    ui.strong("Stopped");
                }
            });
        });

    ui.add_space(8.0);

    egui::CollapsingHeader::new("⚡ App Configuration")
        .default_open(true)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Default Tab:");
                egui::ComboBox::from_id_source("default_tab")
                    .selected_text(match app.default_tab {
                        Tab::Setup => "Setup",
                        Tab::Terminal => "Terminal",
                        Tab::Config => "Config",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut app.default_tab, Tab::Setup, "Setup");
                        ui.selectable_value(&mut app.default_tab, Tab::Terminal, "Terminal");
                        ui.selectable_value(&mut app.default_tab, Tab::Config, "Config");
                    });
            });
        });

    ui.add_space(16.0);
}

//
// ────────────────────────────────────────────────────────────────────────────
//   :: Tab 2: Terminal
// ────────────────────────────────────────────────────────────────────────────
//
pub fn show_terminal_tab(app: &mut ThetaApp, ui: &mut Ui) {
    if ui.button("Copy Output").clicked() {
        ui.output_mut(|o| o.copied_text = app.log_text.clone());
    }
    ui.add_space(4.0);

    // Make the terminal output fill all remaining height
    let available = ui.available_size();
    // Auto-scroll region
    ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
        let mut display_buffer = app.log_text.clone();
        ui.add_sized(
            available,
            TextEdit::multiline(&mut display_buffer)
                .font(egui::TextStyle::Monospace)
                .lock_focus(true)
                .desired_rows(10)
                .desired_width(f32::INFINITY)
                .margin(Vec2::new(0.0, 4.0))
                .interactive(true),
        );
    });
}

//
// ────────────────────────────────────────────────────────────────────────────
//   :: Tab 3: Config (Syntax-Highlighted Editor with Refresh)
// ────────────────────────────────────────────────────────────────────────────
//
pub fn show_config_tab(app: &mut ThetaApp, ui: &mut Ui) {
    egui::CollapsingHeader::new("ThetaData Config File")
        .default_open(true)
        .show(ui, |ui| {
            // 1) The path input
            ui.horizontal(|ui| {
                ui.label("ThetaData Config File Path:");
            });
            ui.horizontal(|ui| {
                ui.add(
                    TextEdit::singleline(&mut app.thetadata_config_path)
                        .desired_width(ui.available_width() - 60.0),
                );
                if ui.button("Browse").clicked() {
                    if let Some(file) = FileDialog::new().pick_file() {
                        app.thetadata_config_path = file.to_string_lossy().to_string();
                        if let Ok(text) =
                            super::ThetaApp::read_thetadata_config_file(&app.thetadata_config_path)
                        {
                            app.thetadata_config_text = text;
                            app.append_log("Config file loaded from browse.\n");
                        } else {
                            app.append_log("Failed to load config from browse.\n");
                        }
                    }
                }
            });

            // 2) "Get from Terminal" button
            ui.horizontal(|ui| {
                if ui.button("Get from Terminal").clicked() {
                    if let Some(detected) = &app.last_detected_config_path {
                        app.thetadata_config_path = detected.clone();
                        match super::ThetaApp::read_thetadata_config_file(detected) {
                            Ok(contents) => {
                                app.thetadata_config_text = contents;
                                app.append_log("Config file loaded from terminal detection.\n");
                            }
                            Err(e) => {
                                app.append_log(&format!("Failed to load config file: {e}\n"));
                            }
                        }
                    } else {
                        app.append_log("No config path detected yet. Launch the terminal first.\n");
                    }
                }

                // 3) NEW: Refresh button
                if ui.button("Refresh").clicked() {
                    if app.thetadata_config_path.is_empty() {
                        app.append_log("No config path set to refresh.\n");
                    } else {
                        match super::ThetaApp::read_thetadata_config_file(
                            &app.thetadata_config_path,
                        ) {
                            Ok(text) => {
                                app.thetadata_config_text = text;
                                app.append_log("Config file refreshed from disk.\n");
                            }
                            Err(e) => app.append_log(&format!("Failed to refresh config: {e}\n")),
                        }
                    }
                }
            });

            ui.add_space(8.0);
            ui.label("Edit your config file below (with minimal syntax highlighting):");

            // Show the config file in a syntax-highlighted code editor
            syntax_highlight_editor(ui, &mut app.thetadata_config_text);

            ui.add_space(16.0);
            ui.label("Remember to click 'Save' at the bottom to persist changes.");
        });
}

/// A code editor that highlights lines starting with '#' as comments, and everything else in green.
/// Using `split_inclusive('\n')` so edits occur at the correct position.
fn syntax_highlight_editor(ui: &mut Ui, text: &mut String) {
    let mut layouter_fn =
        move |ui: &egui::Ui, code: &str, _wrap_width: f32| highlight_config_text(ui, code);

    ui.add(
        TextEdit::multiline(text)
            .font(egui::TextStyle::Monospace)
            .desired_rows(15)
            .desired_width(ui.available_width())
            .lock_focus(false)
            .layouter(&mut layouter_fn),
    );
}

/// Minimal syntax highlighter:
/// - Lines starting with '#' -> gray comment
/// - Everything else -> pale green
fn highlight_config_text(ui: &egui::Ui, code: &str) -> Arc<Galley> {
    let mut job = LayoutJob::default();

    for chunk in code.split_inclusive('\n') {
        let is_comment = chunk.trim_start().starts_with('#');
        let color = if is_comment {
            Color32::LIGHT_GRAY
        } else {
            Color32::from_rgb(150, 255, 150)
        };

        let format = TextFormat {
            font_id: FontId::monospace(14.0),
            color,
            ..Default::default()
        };
        job.append(chunk, 0.0, format);
    }

    ui.fonts(|fonts| fonts.layout_job(job))
}
