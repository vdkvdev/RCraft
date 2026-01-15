use std::collections::{HashMap, VecDeque};
use relm4::{ComponentSender, gtk};
use adw::prelude::*;
use crate::models::{MinecraftVersion, Profile, Section, ModSearchResult};
use crate::settings::Settings;
use crate::launcher::MinecraftLauncher;
use crate::modrinth_client::ModrinthClient;

#[derive(Debug, Clone)]
pub enum AppState {
    Loading,
    Ready { current_section: Section },
    Downloading { version: String, progress: f64, status: String },
    Launching { version: String },
    GameRunning { #[allow(dead_code)] version: String },
    Error { message: String },
}

impl Default for AppState {
    fn default() -> Self {
        AppState::Loading
    }
}

pub struct AppModel {
    pub state: AppState,
    pub launcher: Option<MinecraftLauncher>,
    pub modrinth: ModrinthClient,
    pub window: Option<adw::ApplicationWindow>,

    // Data
    pub profiles: HashMap<String, Profile>,
    pub available_versions: Vec<MinecraftVersion>,
    pub sorted_versions: Vec<String>,

    // Inputs
    pub input_username: String,
    pub input_version: Option<String>,
    pub input_ram: u32,
    pub input_install_fabric: bool,
    pub fabric_switch_enabled: bool,

    // Settings & Logs
    pub settings: Settings,
    pub logs: gtk::TextBuffer,

    // UI State
    pub error_message: Option<String>,

    pub sidebar_collapsed: bool,

    pub versions_updated: bool,
    pub version_list_model: Option<gtk::StringList>,
    pub is_searching: bool,

    // Mods UI State
    pub mod_search_results: Vec<ModSearchResult>,
    pub mod_search_entry: Option<gtk::SearchEntry>,
    pub mod_browse_list: Option<gtk::ListBox>,
    pub mod_installed_list: Option<gtk::ListBox>,
    pub selected_mod_profile: Option<String>,
    pub mod_profile_list_model: Option<gtk::StringList>,

    // Track installed mods: ProjectID -> Filename
    pub installed_mods: HashMap<String, String>,

    pub toast_overlay: Option<adw::ToastOverlay>,

    // Icon Download Queue
    pub icon_download_queue: VecDeque<(String, String)>, // (ProjectID, URL)
    pub is_downloading_icon: bool,

    // Selection Sync
    pub pending_mod_selection: Option<u32>,
    pub pending_launch_profile: Option<String>,
    pub mod_profile_list_updated: bool,

    // Component sender for UI updates
    pub sender: ComponentSender<AppModel>,

    pub java_dialog_request: Option<u32>,

    // Shared Tokio Runtime
    pub rt: std::sync::Arc<tokio::runtime::Runtime>,
}

impl AppModel {
     // Helper to update button state based on installation status
    pub fn update_mod_button_state(&self, project_id: &str) {
         if let Some(list) = &self.mod_browse_list {
             let is_installed = self.installed_mods.contains_key(project_id);
             let icon_name = if is_installed { "user-trash-symbolic" } else { "folder-download-symbolic" };
             let tooltip = if is_installed { "Uninstall" } else { "Install" };
             let sensitive = true;

             let mut sibling = list.first_child();
             while let Some(child) = sibling {
                   if let Some(row) = child.downcast_ref::<gtk::ListBoxRow>() {
                        if let Some(box_widget) = row.child() {
                             if let Some(bx) = box_widget.downcast_ref::<gtk::Box>() {
                                  let mut box_child = bx.first_child();
                                  while let Some(b_child) = box_child {
                                       if let Some(button) = b_child.downcast_ref::<gtk::Button>() {
                                            if button.widget_name() == format!("btn_{}", project_id) {
                                                 button.set_icon_name(icon_name);
                                                 button.set_tooltip_text(Some(tooltip));
                                                 button.set_sensitive(sensitive);

                                                 // Update CSS class?
                                                 if is_installed {
                                                     button.add_css_class("destructive-action");
                                                 } else {
                                                     button.remove_css_class("destructive-action");
                                                 }
                                                 break;
                                            }
                                       }
                                       box_child = b_child.next_sibling();
                                  }
                             }
                        }
                   }
                   sibling = child.next_sibling();
             }
         }
    }
}
