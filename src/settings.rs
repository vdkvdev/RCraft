use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

use crate::models::Theme;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub theme: Theme,
    pub hide_logs: bool,
    pub sidebar_collapsed: bool,
    pub hide_mods_button: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            hide_logs: false,
            sidebar_collapsed: false,
            hide_mods_button: false,
        }
    }
}

impl Settings {
    pub async fn load(config_dir: &PathBuf) -> Self {
        let path = config_dir.join("settings.json");
        if let Ok(content) = fs::read_to_string(&path).await {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub async fn save(&self, config_dir: &PathBuf) -> Result<(), std::io::Error> {
        let path = config_dir.join("settings.json");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let json = serde_json::to_string_pretty(self).unwrap_or_default();
        fs::write(path, json).await
    }
}
