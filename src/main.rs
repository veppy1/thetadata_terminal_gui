#![windows_subsystem = "windows"] // Just makes the app open without a terminal on Windows, ignored on macOS

use eframe::egui::{self, Color32, ScrollArea, Vec2};
use keyring::Entry;
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::thread;

#[derive(Serialize, Deserialize, Default)]
struct AppConfig {
    jar_path: Option<String>,
    auto_start: bool,
    default_tab: Tab,
}

#[derive(PartialEq, Serialize, Deserialize, Clone, Copy)]
enum Tab {
    Setup,
    Terminal,
    Config,
}

impl Default for Tab {
    fn default() -> Self {
        Self::Setup
    }
}

struct ThetaApp {
    username_input: String,
    password_input: String,
    credentials_saved: bool,
    jar_path: String,
    auto_start: bool,
    process: Option<Child>,
    log_text: String,
    log_receiver: Option<Receiver<String>>,
    selected_tab: Tab,
    default_tab: Tab,
}

impl ThetaApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let cfg: AppConfig = confy::load("thetadata_terminal_manager", None).unwrap_or_default();
        let jar_path = cfg.jar_path.unwrap_or_default();
        let auto_start = cfg.auto_start;
        let default_tab = cfg.default_tab;

        // Attempt to load stored credentials:
        let username_entry = Entry::new("ThetaDataTerminal", "username");
        let password_entry = Entry::new("ThetaDataTerminal", "password");
        let (username_input, password_input, credentials_saved) =
            match (username_entry.get_password(), password_entry.get_password()) {
                (Ok(u), Ok(_p)) => (u, String::new(), true),
                _ => (String::new(), String::new(), false),
            };

        let mut app = Self {
            username_input,
            password_input,
            credentials_saved,
            jar_path,
            auto_start,
            process: None,
            log_text: String::new(),
            log_receiver: None,
            selected_tab: default_tab,
            default_tab,
        };

        // Auto-start if configured:
        if app.auto_start && app.credentials_saved && !app.jar_path.is_empty() {
            app.start_terminal();
        }

        app
    }

    fn start_terminal(&mut self) {
        if self.process.is_none() && !self.jar_path.is_empty() {
            let username_entry = Entry::new("ThetaDataTerminal", "username");
            let password_entry = Entry::new("ThetaDataTerminal", "password");
            if let (Ok(username), Ok(password)) =
                (username_entry.get_password(), password_entry.get_password())
            {
                match Command::new("java")
                    .arg("-jar")
                    .arg(&self.jar_path)
                    .arg(&username)
                    .arg(&password)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                {
                    Ok(child) => {
                        self.process = Some(child);
                        self.spawn_log_reader();
                        self.append_log("Terminal started.\n");
                    }
                    Err(e) => self.append_log(&format!("Failed to start terminal: {}\n", e)),
                }
            }
        }
    }

    fn stop_terminal(&mut self) {
        if let Some(mut child) = self.process.take() {
            let _ = child.kill();
            let _ = child.wait();
            self.append_log("Terminal stopped.\n");
        }
    }

    fn reset_terminal(&mut self) {
        self.stop_terminal();
        self.start_terminal();
    }

    fn save_credentials(&mut self) {
        let username_entry = Entry::new("ThetaDataTerminal", "username");
        let password_entry = Entry::new("ThetaDataTerminal", "password");
        if let (Ok(()), Ok(())) = (
            username_entry.set_password(&self.username_input),
            password_entry.set_password(&self.password_input),
        ) {
            self.credentials_saved = true;
            self.append_log("Credentials saved.\n");
        } else {
            self.append_log("Failed to save credentials.\n");
        }
    }

    fn remove_credentials(&mut self) {
        let username_entry = Entry::new("ThetaDataTerminal", "username");
        let password_entry = Entry::new("ThetaDataTerminal", "password");
        let _ = username_entry.delete_password();
        let _ = password_entry.delete_password();
        self.username_input.clear();
        self.password_input.clear();
        self.credentials_saved = false;
        self.append_log("Credentials removed.\n");
    }

    fn spawn_log_reader(&mut self) {
        if let Some(child) = &mut self.process {
            let (tx, rx) = channel();
            self.log_receiver = Some(rx);

            if let Some(stdout) = child.stdout.take() {
                let tx_stdout = tx.clone();
                thread::spawn(move || {
                    let reader = BufReader::new(stdout);
                    for line in reader.lines() {
                        if let Ok(line) = line {
                            let _ = tx_stdout.send(line);
                        }
                    }
                });
            }
            if let Some(stderr) = child.stderr.take() {
                thread::spawn(move || {
                    let reader = BufReader::new(stderr);
                    for line in reader.lines() {
                        if let Ok(line) = line {
                            let _ = tx.send(line);
                        }
                    }
                });
            }
        }
    }

    fn append_log(&mut self, text: &str) {
        self.log_text.push_str(text);
    }
}

impl eframe::App for ThetaApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(egui::Visuals::dark());

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(4.0);
            ui.heading(egui::RichText::new("ThetaData GUI Wrapper").strong());
            ui.add_space(8.0);
            ui.with_layout(
                egui::Layout::top_down_justified(egui::Align::Center),
                |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        let button_size = Vec2::new(50.0, 18.0);

                        // Setup tab button
                        let setup_response = if self.selected_tab == Tab::Setup {
                            ui.add_sized(button_size, egui::Button::new("Setup"))
                        } else {
                            ui.add_sized(
                                button_size,
                                egui::Button::new("Setup")
                                    .fill(Color32::TRANSPARENT)
                                    .stroke(egui::Stroke::new(1.0, ui.visuals().text_color())),
                            )
                        };
                        if setup_response.clicked() {
                            self.selected_tab = Tab::Setup;
                        }

                        // Terminal tab button
                        let terminal_response = if self.selected_tab == Tab::Terminal {
                            ui.add_sized(button_size, egui::Button::new("Terminal"))
                        } else {
                            ui.add_sized(
                                button_size,
                                egui::Button::new("Terminal")
                                    .fill(Color32::TRANSPARENT)
                                    .stroke(egui::Stroke::new(1.0, ui.visuals().text_color())),
                            )
                        };
                        if terminal_response.clicked() {
                            self.selected_tab = Tab::Terminal;
                        }

                        // Config tab button
                        let config_response = if self.selected_tab == Tab::Config {
                            ui.add_sized(button_size, egui::Button::new("Config"))
                        } else {
                            ui.add_sized(
                                button_size,
                                egui::Button::new("Config")
                                    .fill(Color32::TRANSPARENT)
                                    .stroke(egui::Stroke::new(1.0, ui.visuals().text_color())),
                            )
                        };
                        if config_response.clicked() {
                            self.selected_tab = Tab::Config;
                        }
                    });
                },
            );
            ui.add_space(8.0);

            ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| match self.selected_tab {
                    Tab::Setup => {
                        egui::CollapsingHeader::new("⌨ Login Credentials")
                            .default_open(true)
                            .show(ui, |ui| {
                                if self.credentials_saved {
                                    ui.horizontal(|ui| {
                                        ui.label("Username (saved):");
                                        ui.monospace(&self.username_input);
                                    });
                                    ui.label("Password stored on keychain.");
                                    if ui.button("Remove all credentials").clicked() {
                                        self.remove_credentials();
                                    }
                                } else {
                                    ui.horizontal(|ui| {
                                        ui.label("Username:");
                                        ui.add(
                                            egui::TextEdit::singleline(&mut self.username_input)
                                                .desired_width(ui.available_width() - 8.0),
                                        );
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("Password: ");
                                        ui.add(
                                            egui::TextEdit::singleline(&mut self.password_input)
                                                .password(true)
                                                .desired_width(ui.available_width() - 8.0),
                                        );
                                    });
                                    if ui.button("Save Credentials").clicked() {
                                        self.save_credentials();
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
                                        egui::TextEdit::singleline(&mut self.jar_path)
                                            .desired_width(ui.available_width() - 60.0),
                                    );
                                    if ui.button("Browse").clicked() {
                                        if let Some(file) = FileDialog::new()
                                            .add_filter("JAR Files", &["jar"])
                                            .pick_file()
                                        {
                                            self.jar_path = file.to_string_lossy().to_string();
                                        }
                                    }
                                });
                                ui.checkbox(
                                    &mut self.auto_start,
                                    "Start ThetaData Terminal on app launch",
                                );
                            });

                        ui.add_space(8.0);

                        egui::CollapsingHeader::new("☑ Terminal Controls")
                            .default_open(true)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    if ui.button("Start").clicked() {
                                        self.start_terminal();
                                    }
                                    if ui.button("Stop").clicked() {
                                        self.stop_terminal();
                                    }
                                    if ui.button("Reset").clicked() {
                                        self.reset_terminal();
                                    }
                                });
                                ui.horizontal(|ui| {
                                    ui.label("Status: ");
                                    if self.process.is_some() {
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
                                        .selected_text(match self.default_tab {
                                            Tab::Setup => "Setup",
                                            Tab::Terminal => "Terminal",
                                            Tab::Config => "Config",
                                        })
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(
                                                &mut self.default_tab,
                                                Tab::Setup,
                                                "Setup",
                                            );
                                            ui.selectable_value(
                                                &mut self.default_tab,
                                                Tab::Terminal,
                                                "Terminal",
                                            );
                                            ui.selectable_value(
                                                &mut self.default_tab,
                                                Tab::Config,
                                                "Config",
                                            );
                                        });
                                });
                            });

                        ui.add_space(16.0);
                    }
                    Tab::Terminal => {
                        if ui.button("Copy Output").clicked() {
                            ui.output_mut(|o| o.copied_text = self.log_text.clone());
                        }
                        ui.add_space(4.0);
                        ScrollArea::vertical()
                            .stick_to_bottom(true)
                            .max_height(300.0)
                            .id_source("terminal_logs")
                            .show(ui, |ui| {
                                ui.add(
                                    egui::TextEdit::multiline(&mut self.log_text)
                                        .font(egui::TextStyle::Monospace)
                                        .desired_rows(10)
                                        .lock_focus(true)
                                        .desired_width(f32::INFINITY)
                                        .margin(Vec2::new(0.0, 4.0))
                                        .interactive(false),
                                );
                            });
                    }
                    Tab::Config => {
                        ui.label("Configuration options coming soon...");
                    }
                });
        });

        let new_lines: Vec<String> = if let Some(rx) = self.log_receiver.as_ref() {
            rx.try_iter().collect()
        } else {
            Vec::new()
        };
        for line in new_lines {
            self.append_log(&line);
            self.append_log("\n");
        }

        if let Some(child) = &mut self.process {
            if let Ok(Some(_status)) = child.try_wait() {
                self.append_log("Terminal process exited.\n");
                self.process = None;
            }
        }

        let new_cfg = AppConfig {
            jar_path: if self.jar_path.is_empty() {
                None
            } else {
                Some(self.jar_path.clone())
            },
            auto_start: self.auto_start,
            default_tab: self.default_tab,
        };
        if let Err(e) = confy::store("thetadata_terminal_manager", None, new_cfg) {
            self.append_log(&format!("Failed saving app config: {e}\n"));
        }

        ctx.request_repaint();
    }
}

fn main() {
    // Load the custom icon using platform-specific paths.
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

    let image = image::load_from_memory(icon_bytes)
        .expect("Failed to load icon")
        .to_rgba8();
    let (width, height) = image.dimensions();
    let icon_data = eframe::IconData {
        rgba: image.into_raw(),
        width,
        height,
    };

    let native_options = eframe::NativeOptions {
        initial_window_size: Some(Vec2::new(300.0, 275.0)),
        resizable: false,
        min_window_size: Some(Vec2::new(300.0, 275.0)),
        icon_data: Some(icon_data), // Custom window icon set here
        ..Default::default()
    };

    eframe::run_native(
        "ThetaData Terminal GUI",
        native_options,
        Box::new(|cc| Box::new(ThetaApp::new(cc))),
    )
    .unwrap();
}
