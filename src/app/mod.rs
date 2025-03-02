pub mod tabs;

use crate::app::tabs::{show_config_tab, show_setup_tab, show_terminal_tab};
use crate::model::{AppConfig, Tab};
use eframe::egui::{self, Color32, ScrollArea, Vec2};
use keyring::Entry;
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
    sync::mpsc::{channel, Receiver},
    thread,
    time::Duration,
};

// Import WINDOWS_1252 for fallback decoding on Windows.
use encoding_rs::WINDOWS_1252;

/// The main application state and logic
pub struct ThetaApp {
    // -- Setup tab fields --
    pub username_input: String,
    pub password_input: String,
    pub credentials_saved: bool,

    // -- Terminal config --
    pub jar_path: String,
    pub auto_start: bool,

    // -- Child process & logging --
    pub process: Option<Child>,
    pub log_text: String,
    pub log_receiver: Option<Receiver<String>>,

    // -- Which tab is selected + the default tab --
    pub selected_tab: Tab,
    pub default_tab: Tab,

    // -- ThetaData config file management --
    pub thetadata_config_path: String, // user's chosen config file path
    pub thetadata_config_text: String, // the text we load/edit
    pub last_detected_config_path: Option<String>,
}

impl ThetaApp {
    /// Constructs the app by loading configuration and stored credentials.
    /// (Note: The terminal is not auto-started on launch.)
    pub fn new() -> Self {
        let cfg: AppConfig = confy::load("thetadata_terminal_manager", None).unwrap_or_default();

        // Always force the default tab to Setup.
        let default_tab = Tab::Setup;

        let username_entry = Entry::new("ThetaDataTerminal", "username");
        let password_entry = Entry::new("ThetaDataTerminal", "password");
        let (username_input, password_input, credentials_saved) =
            match (username_entry.get_password(), password_entry.get_password()) {
                (Ok(u), Ok(_p)) => (u, String::new(), true),
                _ => (String::new(), String::new(), false),
            };

        let jar_path = cfg.jar_path.unwrap_or_default();
        let auto_start = false; // Disable auto-start regardless of config.
        let thetadata_config_path = cfg.thetadata_config_path.unwrap_or_default();

        let mut thetadata_config_text = String::new();
        if !thetadata_config_path.is_empty() {
            thetadata_config_text =
                Self::read_thetadata_config_file(&thetadata_config_path).unwrap_or_default();
        }

        Self {
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
            thetadata_config_path,
            thetadata_config_text,
            last_detected_config_path: None,
        }
    }

    /// Start the Theta Terminal process if not already running.
    pub fn start_terminal(&mut self) {
        if self.process.is_none() && !self.jar_path.is_empty() {
            let username_entry = Entry::new("ThetaDataTerminal", "username");
            let password_entry = Entry::new("ThetaDataTerminal", "password");

            if let (Ok(username), Ok(password)) =
                (username_entry.get_password(), password_entry.get_password())
            {
                let mut command = if cfg!(target_os = "windows") {
                    // Use javaw on Windows so no console window is created.
                    Command::new("javaw")
                } else {
                    Command::new("java")
                };
                command
                    .arg("-jar")
                    .arg(&self.jar_path)
                    .arg(&username)
                    .arg(&password)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());
                #[cfg(target_os = "windows")]
                {
                    use std::os::windows::process::CommandExt;
                    const CREATE_NO_WINDOW: u32 = 0x08000000;
                    // Optionally, you could also add DETACHED_PROCESS: 0x00000008
                    command.creation_flags(CREATE_NO_WINDOW);
                }
                match command.spawn() {
                    Ok(mut child) => {
                        let (tx, rx) = channel();
                        if let Some(stdout) = child.stdout.take() {
                            let tx_stdout = tx.clone();
                            thread::spawn(move || {
                                let reader = BufReader::new(stdout);
                                for line in reader.lines().flatten() {
                                    let _ = tx_stdout.send(line);
                                }
                            });
                        }
                        if let Some(stderr) = child.stderr.take() {
                            thread::spawn(move || {
                                let reader = BufReader::new(stderr);
                                for line in reader.lines().flatten() {
                                    let _ = tx.send(line);
                                }
                            });
                        }
                        self.log_receiver = Some(rx);
                        self.process = Some(child);
                        self.append_log("Terminal started.\n");
                    }
                    Err(e) => self.append_log(&format!("Failed to start terminal: {e}\n")),
                }
            } else {
                self.append_log("No valid credentials found. Cannot start.\n");
            }
        }
    }

    /// Forcefully quit the terminal process.
    pub fn force_quit_process(&mut self) {
        if let Some(mut child) = self.process.take() {
            let _ = child.kill();
            let _ = child.wait();
            self.append_log("Terminal forcibly quit.\n");
        }
    }

    pub fn stop_terminal(&mut self) {
        self.force_quit_process();
    }

    pub fn reset_terminal(&mut self) {
        self.force_quit_process();
        thread::sleep(Duration::from_millis(250));
        self.start_terminal();
    }

    pub fn save_credentials(&mut self) {
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

    pub fn remove_credentials(&mut self) {
        let username_entry = Entry::new("ThetaDataTerminal", "username");
        let password_entry = Entry::new("ThetaDataTerminal", "password");

        let _ = username_entry.delete_password();
        let _ = password_entry.delete_password();
        self.username_input.clear();
        self.password_input.clear();
        self.credentials_saved = false;
        self.append_log("Credentials removed.\n");
    }

    pub fn append_log(&mut self, text: &str) {
        self.log_text.push_str(text);
    }

    /// Detect and capture a config file path from a log line.
    pub fn detect_config_file_path_in_line(&mut self, line: &str) {
        let prefix = "Using ";
        let suffix = " as the config file";
        if let (Some(start_idx), Some(end_idx)) = (line.find(prefix), line.find(suffix)) {
            let path_start = start_idx + prefix.len();
            if path_start < end_idx {
                let raw_path = &line[path_start..end_idx].trim();
                self.last_detected_config_path = Some(raw_path.to_string());
                self.append_log(&format!(
                    "Detected config file path from terminal: {raw_path}\n"
                ));
            }
        }
    }

    /// Read the ThetaData config file.
    /// If the file isn’t valid UTF‑8, decode it as Windows‑1252.
    pub fn read_thetadata_config_file(path: &str) -> std::io::Result<String> {
        let bytes = fs::read(path)?;
        match String::from_utf8(bytes.clone()) {
            Ok(s) => Ok(s),
            Err(_) => {
                let (cow, _, _) = WINDOWS_1252.decode(&bytes);
                Ok(cow.into_owned())
            }
        }
    }

    /// Write the ThetaData config file.
    pub fn write_thetadata_config_file(path: &str, contents: &str) -> std::io::Result<()> {
        let mut file = fs::File::create(path)?;
        file.write_all(contents.as_bytes())?;
        Ok(())
    }

    /// Save the current config file text.
    pub fn save_current_config_file(&mut self) {
        if self.thetadata_config_path.is_empty() {
            self.append_log("No config file path set.\n");
            return;
        }
        match Self::write_thetadata_config_file(
            &self.thetadata_config_path,
            &self.thetadata_config_text,
        ) {
            Ok(_) => self.append_log("Config file saved.\n"),
            Err(e) => self.append_log(&format!("Failed to write config file: {e}\n")),
        }
    }
}

impl eframe::App for ThetaApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Only show the bottom panel (with Save button) when on the Config tab.
        if self.selected_tab == Tab::Config {
            eframe::egui::TopBottomPanel::bottom("global_bottom_panel").show(ctx, |ui| {
                ui.add_space(6.0);
                if ui.button("Save").clicked() {
                    self.save_current_config_file();
                }
                ui.add_space(6.0);
            });
        }

        eframe::egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(4.0);
            ui.heading(egui::RichText::new("ThetaData GUI Wrapper").strong());
            ui.add_space(8.0);

            ui.with_layout(
                egui::Layout::top_down_justified(egui::Align::Center),
                |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        let button_size = Vec2::new(50.0, 18.0);

                        let setup_btn = if self.selected_tab == Tab::Setup {
                            ui.add_sized(button_size, egui::Button::new("Setup"))
                        } else {
                            ui.add_sized(
                                button_size,
                                egui::Button::new("Setup")
                                    .fill(Color32::TRANSPARENT)
                                    .stroke(egui::Stroke::new(1.0, ui.visuals().text_color())),
                            )
                        };
                        if setup_btn.clicked() {
                            self.selected_tab = Tab::Setup;
                        }

                        let terminal_btn = if self.selected_tab == Tab::Terminal {
                            ui.add_sized(button_size, egui::Button::new("Terminal"))
                        } else {
                            ui.add_sized(
                                button_size,
                                egui::Button::new("Terminal")
                                    .fill(Color32::TRANSPARENT)
                                    .stroke(egui::Stroke::new(1.0, ui.visuals().text_color())),
                            )
                        };
                        if terminal_btn.clicked() {
                            self.selected_tab = Tab::Terminal;
                        }

                        let config_btn = if self.selected_tab == Tab::Config {
                            ui.add_sized(button_size, egui::Button::new("Config"))
                        } else {
                            ui.add_sized(
                                button_size,
                                egui::Button::new("Config")
                                    .fill(Color32::TRANSPARENT)
                                    .stroke(egui::Stroke::new(1.0, ui.visuals().text_color())),
                            )
                        };
                        if config_btn.clicked() {
                            self.selected_tab = Tab::Config;
                        }
                    });
                },
            );

            ui.add_space(8.0);

            ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| match self.selected_tab {
                    Tab::Setup => show_setup_tab(self, ui),
                    Tab::Terminal => show_terminal_tab(self, ui),
                    Tab::Config => show_config_tab(self, ui),
                });
        });

        let new_lines: Vec<String> = if let Some(rx) = &self.log_receiver {
            rx.try_iter().collect()
        } else {
            Vec::new()
        };
        for line in new_lines {
            self.append_log(&line);
            self.append_log("\n");
            self.detect_config_file_path_in_line(&line);
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
            default_tab: self.selected_tab,
            thetadata_config_path: if self.thetadata_config_path.is_empty() {
                None
            } else {
                Some(self.thetadata_config_path.clone())
            },
        };
        if let Err(e) = confy::store("thetadata_terminal_manager", None, new_cfg) {
            self.append_log(&format!("Failed saving app config: {e}\n"));
        }

        ctx.request_repaint();
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.force_quit_process();
    }
}
