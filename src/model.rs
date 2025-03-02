use serde::{Deserialize, Serialize};

/// Stored app configuration, loaded/saved with confy.
#[derive(Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub jar_path: Option<String>,
    pub auto_start: bool,
    pub default_tab: Tab,
    pub thetadata_config_path: Option<String>,
}

/// Which tab is selected
#[derive(PartialEq, Serialize, Deserialize, Clone, Copy)]
pub enum Tab {
    Setup,
    Terminal,
    Config,
}

impl Default for Tab {
    fn default() -> Self {
        Self::Setup
    }
}
