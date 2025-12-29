// ui.rs - Relm4 UI components for RCraft with libadwaita

use adw::prelude::*;
use adw::{self, NavigationSplitView, NavigationPage, StatusPage, EntryRow, ComboRow, SpinRow};
use relm4::gtk;
use relm4::{ComponentParts, ComponentSender, SimpleComponent};
use std::collections::HashMap;
use tokio::runtime::Runtime;
use tokio::io::{AsyncBufReadExt, BufReader};
use gtk::prelude::*;

use crate::models::{MinecraftVersion, Profile};
use crate::settings::Settings;




use gtk::prelude::{AdjustmentExt, Cast, WidgetExt, ObjectExt};
use crate::models::{Section, Theme};
use crate::launcher::MinecraftLauncher;

// ============================================================================
// Application Model
// ============================================================================



// ============================================================================
// Application Model
// ============================================================================

pub struct AppModel {
    pub state: AppState,
    pub launcher: Option<MinecraftLauncher>,
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
    pub download_dots: u8,

    pub versions_updated: bool,
    pub version_list_model: Option<gtk::StringList>,

    // Component sender for UI updates
    pub sender: ComponentSender<AppModel>,
}



// ============================================================================
// Application State
// ============================================================================

#[derive(Debug, Clone)]
pub enum AppState {
    Loading,
    Ready { current_section: Section },
    Downloading { version: String },
    Launching { version: String },
    GameRunning { version: String },
    Error { message: String },
}

impl Default for AppState {
    fn default() -> Self {
        AppState::Loading
    }
}

// ============================================================================
// Messages
// ============================================================================

#[derive(Debug)]
pub enum AppMsg {
    LaunchProfile(String),
    DeleteProfile(String),
    UsernameChanged(String),
    VersionSelected(String),
    RamChanged(u32),
    ToggleFabric(bool),
    SaveProfile,
    CancelCreate,
    VersionsLoaded(Result<Vec<MinecraftVersion>, String>),
    ProfilesLoaded(Result<HashMap<String, Profile>, String>),
    DownloadCompleted,
    DownloadStarted(String),
    GameStarted,
    LaunchCompleted,
    NavigateToSection(Section),
    BackToMainMenu,
    UpdateDownloadDots,
    OpenMinecraftFolder,
    ShowToast(String),
    ShowAboutWindow,
    ThemeSelected(Theme),
    ToggleNerdMode(bool),
    Log(String),
    MinimizeWindow,
    CloseWindow,
    Error(String),
    RequestDeleteProfile(String),
    SettingsLoaded(Settings),
    SessionEnded(String, u64),
}

// ============================================================================
// Widgets
// ============================================================================

#[allow(dead_code)]
pub struct AppWidgets {
    window: adw::ApplicationWindow,
    header_bar: adw::HeaderBar,
    navigation_split_view: NavigationSplitView,
    navigation_page: NavigationPage,
    content_stack: gtk::Stack,

    // Pages
    home_page: gtk::Box,
    create_page: gtk::Box,
    settings_page: gtk::Box,
    logs_page: gtk::ScrolledWindow,
    loading_page: adw::StatusPage,

    // Home page widgets
    profile_list: gtk::ListBox,
    username_entry: adw::EntryRow,
    version_combo: adw::ComboRow,
    ram_scale: adw::SpinRow,
    fabric_switch: adw::SwitchRow,
    nerd_mode_switch: adw::SwitchRow,

    // Buttons
    launch_button: gtk::Button,
    create_button: gtk::Button,
    delete_button: gtk::Button,
    save_button: gtk::Button,
    cancel_button: gtk::Button,

    // Sidebar buttons
    home_button: gtk::Button,
    create_sidebar_button: gtk::Button,
    settings_button: gtk::Button,
    logs_button: gtk::Button,
    
    // Settings widgets
    theme_combo: adw::ComboRow,

    // Status/error labels
    status_label: gtk::Label,
    error_label: gtk::Label,

    // Loading widgets
    loading_spinner: gtk::Spinner,
    loading_label: gtk::Label,

    // Logs view
    logs_view: gtk::TextView,
}

// ============================================================================
// Relm4 Component Implementation
// ============================================================================

impl SimpleComponent for AppModel {
    type Input = AppMsg;
    type Output = ();
    type Init = ();
    type Root = adw::ApplicationWindow;
    type Widgets = AppWidgets;

    fn init_root() -> Self::Root {
        let window = adw::ApplicationWindow::builder()
            .title("RCraft")
            .default_width(900)
            .default_height(540)
            .build();
        window.set_decorated(true);

        window
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // Initialize model
        let mut model = AppModel {
            state: AppState::Loading,
            launcher: match MinecraftLauncher::new() {
                Ok(launcher) => Some(launcher),
                Err(e) => {
                    sender.input(AppMsg::Error(e.to_string()));
                    None
                }
            },
            window: Some(root.clone()),
            profiles: HashMap::new(),
            available_versions: Vec::new(),
            sorted_versions: Vec::new(),
            input_username: String::new(),
            input_version: None,
            input_ram: 2048, // Default 2GB
            input_install_fabric: false,
            fabric_switch_enabled: false,
            error_message: None,
            download_dots: 0,

            // Initialize settings
            settings: {
                let _config_dir = if let Ok(launcher) = MinecraftLauncher::new() {
                    launcher.config.minecraft_dir
                } else {
                    std::path::PathBuf::from(".")
                };

                // Block on async load (since we are in sync init) - unsafe but simple for now
                // Or better: Load defaults, then spawn task to load.
                // For simplicity let's use a blocking load wrapper or just defaults then load.
                // Actually `Settings::load` is async. Let's spawn a task.
                Settings::default()
            },
            logs: gtk::TextBuffer::new(None),

            versions_updated: false,
            version_list_model: None,
            sender: sender.clone(),
        };

        // Set window title
        root.set_title(Some("RCraft"));

        // Create navigation split view for sidebar navigation
        let navigation_split_view = adw::NavigationSplitView::new();
        navigation_split_view.set_collapsed(false);
        navigation_split_view.set_vexpand(true);
        navigation_split_view.set_hexpand(true);

        // Ensure sidebar expands to fill available space
        navigation_split_view.set_max_sidebar_width(250.0);
        navigation_split_view.set_min_sidebar_width(180.0);





        // Create sidebar
        let (sidebar, home_button, create_sidebar_button, settings_button, logs_button) = create_sidebar(&sender);
        navigation_split_view.set_sidebar(Some(&sidebar));

        // Create content stack for different sections
        let content_stack = gtk::Stack::new();
        content_stack.set_transition_type(gtk::StackTransitionType::Crossfade);
        content_stack.set_transition_duration(200);

        // Create widget fields first
        let username_entry = adw::EntryRow::builder()
            .title("Username")
            .build();

        let version_list_model = gtk::StringList::new(&[]);
        let version_combo = {
            let combo = adw::ComboRow::builder()
                .title("Minecraft Version")
                .build();
            combo.set_model(Some(&version_list_model));
            combo
        };
        model.version_list_model = Some(version_list_model.clone());

        let ram_scale = adw::SpinRow::builder()
            .title("RAM (MB)")
            .adjustment(&gtk::Adjustment::new(2048.0, 1024.0, 32768.0, 256.0, 256.0, 0.0))
            .build();

        let fabric_switch = adw::SwitchRow::builder()
            .title("Install Fabric")
            .subtitle("Install Fabric Modloader for this version")
            .build();

        let nerd_mode_switch = adw::SwitchRow::builder()
            .title("Nerd Mode")
            .build();

        let profile_list = gtk::ListBox::new();
        let loading_widgets = create_loading_widgets();

        // Create pages for each section
        // Create pages for each section
        let home_page = create_home_page(&sender, &profile_list);
        let create_page = create_create_instance_page(&sender, &username_entry, &version_combo, &ram_scale, &fabric_switch);
        let (settings_page, theme_combo) = create_settings_page(&sender, &nerd_mode_switch);
        let (logs_page, logs_view) = create_logs_page(&sender, &model.logs);

        // Update settings page to use actual nerd mode switch from widgets or binding?
        // Actually create_settings_page created the switch inside itself.
        // We should probably rely on messages to update the model.
        // But we need ref to switch to update it if model changes?
        // Let's assume create_settings_page handles it.
        // Wait, I need to pass the switch to widgets to control its state?
        // Or I can query the settings page children? Too complex.
        // Let's modify create_settings_page to accept the switch or return it.
        // For now let's assume create_settings_page creates it locally and I missed capturing it.
        // Re-reading create_settings_page... it creates `nerd_mode_switch` locally.
        // I need to be able to set its state from model.settings.nerd_mode.
        // So I should pass it IN, or return it out.
        // Let's pass it IN like other inputs.

        content_stack.add_titled(&home_page, Some("home"), "Home");
        content_stack.add_titled(&create_page, Some("create"), "Create");
        content_stack.add_titled(&settings_page, Some("settings"), "Settings");
        content_stack.add_titled(&logs_page, Some("logs"), "Logs");
        content_stack.add_titled(&loading_widgets.0, Some("loading"), "Loading");

        // Initialize Nerd Mode State
        logs_button.set_visible(model.settings.nerd_mode);

        // ... (Error label creation)

        let error_label = gtk::Label::new(None);
        error_label.add_css_class("error-label");
        error_label.set_wrap(true);
        error_label.set_max_width_chars(50);

        let error_status_page = adw::StatusPage::builder()
            .title("Error")
            .icon_name("dialog-error-symbolic")
            .child(&error_label)
            .build();

        content_stack.add_titled(&error_status_page, Some("error"), "Error");

        // Wrap content_stack in a NavigationPage for NavigationSplitView
        let navigation_page = adw::NavigationPage::builder()
            .title("RCraft")
            .child(&content_stack)
            .build();

        // Set the main content
        navigation_split_view.set_content(Some(&navigation_page));

        // Create header bar
        let header_bar = adw::HeaderBar::new();
        header_bar.set_show_end_title_buttons(true);
        header_bar.set_title_widget(Some(&adw::WindowTitle::new("RCraft", "")));

        // Create vertical box to hold header bar and navigation split view
        let main_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        main_box.set_vexpand(true);
        main_box.set_hexpand(true);
        main_box.set_valign(gtk::Align::Fill);
        main_box.append(&header_bar);
        main_box.append(&navigation_split_view);

        root.set_content(Some(&main_box));

        // Create widgets struct
        let widgets = AppWidgets {
            window: root.clone(),
            header_bar,
            navigation_split_view,
            navigation_page,
            content_stack,
            home_page,
            create_page,
            settings_page,
            logs_page,
            loading_page: loading_widgets.0,
            profile_list,
            username_entry,
            version_combo,
            ram_scale,
            fabric_switch,
            nerd_mode_switch,
            launch_button: gtk::Button::with_label("Launch"),
            create_button: gtk::Button::with_label("Create"),
            delete_button: gtk::Button::with_label("Delete"),
            save_button: gtk::Button::with_label("Save"),
            cancel_button: gtk::Button::with_label("Cancel"),
            home_button,
            create_sidebar_button,
            settings_button,
            logs_button,
            theme_combo,
            status_label: gtk::Label::new(None),
            error_label,
            loading_spinner: loading_widgets.1,
            loading_label: loading_widgets.2,
            logs_view,
        };

        // Start loading data asynchronously
        sender.input(AppMsg::NavigateToSection(Section::Home));

        // Load versions asynchronously
        let sender_clone = sender.clone();
        if let Some(launcher) = &model.launcher {
            let launcher_clone = launcher.clone();
            std::thread::spawn(move || {
                let rt = Runtime::new().unwrap();
                rt.block_on(async {
                    match launcher_clone.get_available_versions().await {
                        Ok(versions) => sender_clone.input(AppMsg::VersionsLoaded(Ok(versions))),
                        Err(e) => sender_clone.input(AppMsg::VersionsLoaded(Err(e.to_string()))),
                    }
                });
            });
        }

        // Apply theme
        // Note: We can't apply theme easily to window here as it might be too early or we need to post it?
        // We can just send a message.
        sender.input(AppMsg::ThemeSelected(model.settings.theme.clone()));


        // Apply theme (initial default or system)
        sender.input(AppMsg::ThemeSelected(model.settings.theme.clone()));

        // Load settings asynchronously
        let sender_clone = sender.clone();
        let config_dir_clone = if let Some(l) = &model.launcher { l.config.minecraft_dir.clone() } else { std::path::PathBuf::from(".") };
        std::thread::spawn(move || {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
               let settings = Settings::load(&config_dir_clone).await;
               sender_clone.input(AppMsg::SettingsLoaded(settings));
            });
        });

        // Load profiles asynchronously
        let sender_clone = sender.clone();
        if let Some(launcher) = &model.launcher {
            let config_dir = launcher.config.minecraft_dir.clone();
            std::thread::spawn(move || {
                let rt = Runtime::new().unwrap();
                rt.block_on(async {
                    let path = config_dir.join("profiles.json");
                    let profiles = if tokio::fs::try_exists(&path).await.unwrap_or(false) {
                        match tokio::fs::read_to_string(&path).await {
                            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
                            Err(_) => HashMap::new(),
                        }
                    } else {
                        HashMap::new()
                    };
                    sender_clone.input(AppMsg::ProfilesLoaded(Ok(profiles)));
                });
            });
        }

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            AppMsg::NavigateToSection(section) => {
                self.state = AppState::Ready { current_section: section };
            }
            AppMsg::SettingsLoaded(settings) => {
                self.settings = settings.clone();
                // Apply loaded settings
                self.sender.input(AppMsg::ToggleNerdMode(settings.nerd_mode));
                self.sender.input(AppMsg::ThemeSelected(settings.theme));
            }
            AppMsg::ToggleNerdMode(enabled) => {
                self.settings.nerd_mode = enabled;

                // Save settings
                if let Some(launcher) = &self.launcher {
                     let config_dir = launcher.config.minecraft_dir.clone();
                     let settings_clone = self.settings.clone();
                     std::thread::spawn(move || {
                         let rt = tokio::runtime::Runtime::new().unwrap();
                         rt.block_on(async {
                             let _ = settings_clone.save(&config_dir).await;
                         });
                     });
                }
            }
            AppMsg::Log(log_line) => {
                 let mut end_iter = self.logs.end_iter();
                 self.logs.insert(&mut end_iter, &format!("{}\n", log_line));
            }
            AppMsg::VersionsLoaded(result) => {
                match result {
                    Ok(versions) => {
                        // Filter versions to only 1.8+
                        use crate::utils::{is_at_least_1_8, compare_versions};
                        let mut filtered: Vec<_> = versions
                            .into_iter()
                            .filter(|v| is_at_least_1_8(&v.id))
                            .collect();
                        filtered.sort_by(|a, b| compare_versions(&b.id, &a.id));

                        self.sorted_versions = filtered.iter().map(|v| v.id.clone()).collect();
                        self.available_versions = filtered;
                        self.versions_updated = true;

                        // Populate version combo box
                        if let Some(string_list) = &self.version_list_model {
                            while string_list.n_items() > 0 {
                                string_list.remove(0);
                            }
                            for version in &self.sorted_versions {
                                string_list.append(version);
                            }
                        }
                    }
                    Err(e) => {
                        self.error_message = Some(format!("Failed to load versions: {}", e));
                    }
                }
            }
            AppMsg::ProfilesLoaded(result) => {
                match result {
                    Ok(profiles) => {
                        self.profiles = profiles;
                    }
                    Err(e) => {
                        self.error_message = Some(format!("Failed to load profiles: {}", e));
                    }
                }
            }
            AppMsg::LaunchProfile(profile_name) => {
                if let Some(profile) = self.profiles.get(&profile_name) {
                    if let Some(launcher) = &self.launcher {
                        let launcher_clone = launcher.clone();
                        let profile_clone = profile.clone();
                        let sender_clone = sender.clone();

                        // Change to Launching state immediately to show logs if in nerd mode or just loading
                        // We use Launching state for "Launching..." screen which we want to keep.
                        self.state = AppState::Launching { version: profile_clone.version.clone() };

                        let profile_name_clone = profile_name.clone();

                        std::thread::spawn(move || {
                            let rt = tokio::runtime::Runtime::new().unwrap();
                            rt.block_on(async {
                                // Determine the actual version ID to launch and install fabric if needed
                                let mut version_to_launch = profile_clone.version.clone();

                                // Fabric installation logic
                                if profile_clone.is_fabric {
                                    // Check/Install Fabric logic...
                                     let fabric_installed = if let Ok(mut entries) = tokio::fs::read_dir(&launcher_clone.config.versions_dir).await {
                                        let mut found = None;
                                        while let Ok(Some(entry)) = entries.next_entry().await {
                                            if let Some(name) = entry.file_name().to_str() {
                                                if name.contains("fabric-loader") && name.contains(&profile_clone.version) {
                                                    // Found one
                                                    found = Some(name.to_string());
                                                    break;
                                                }
                                            }
                                        }
                                        found
                                    } else {
                                        None
                                    };

                                    if let Some(fabric_id) = fabric_installed {
                                        version_to_launch = fabric_id;
                                    } else {
                                        sender_clone.input(AppMsg::ShowToast(format!("First time setup: Installing Fabric for {}...", profile_clone.version)));
                                        match launcher_clone.install_fabric(&profile_clone.version).await {
                                            Ok(new_fabric_id) => {
                                                version_to_launch = new_fabric_id;
                                            }
                                            Err(e) => {
                                                sender_clone.input(AppMsg::Error(format!("Failed to install Fabric: {}", e)));
                                                return;
                                            }
                                        }
                                    }
                                }

                                // Check vanilla jar
                                let vanilla_version_id = &profile_clone.version;
                                let version_dir = launcher_clone.config.versions_dir.join(vanilla_version_id);
                                let jar_path = version_dir.join(format!("{}.jar", vanilla_version_id));

                                if !jar_path.exists() {
                                    sender_clone.input(AppMsg::ShowToast(format!("Downloading Minecraft {}...", vanilla_version_id)));
                                     match launcher_clone.get_available_versions().await {
                                        Ok(versions) => {
                                             if let Some(v) = versions.into_iter().find(|v| v.id == *vanilla_version_id) {
                                                  // Change state to downloading
                                                  sender_clone.input(AppMsg::DownloadStarted(vanilla_version_id.clone()));

                                                  if let Err(e) = launcher_clone.download_version(&v).await {
                                                      sender_clone.input(AppMsg::Error(format!("Failed to download vanilla version: {}", e)));
                                                      return;
                                                  }
                                             } else {
                                                 sender_clone.input(AppMsg::Error(format!("Version {} not found in manifest", vanilla_version_id)));
                                                 return;
                                             }
                                        }
                                        Err(e) => {
                                             sender_clone.input(AppMsg::Error(format!("Failed to fetch version manifest: {}", e)));
                                             return;
                                        }
                                    }
                                }

                                // Launch Minecraft
                                sender_clone.input(AppMsg::ShowToast(format!("Launching {}...", version_to_launch)));
                                match launcher_clone.launch_minecraft(
                                    &version_to_launch,
                                    &profile_clone.username,
                                    profile_clone.ram_mb
                                ).await {
                                    Ok(mut command) => {
                                        // Spawn the command to get a Child process
                                        match command.spawn() {
                                            Ok(mut child) => {
                                                // Game Started!
                                                sender_clone.input(AppMsg::GameStarted);

                                                let start_time = std::time::Instant::now();

                                                // Streaming logs
                                                let stdout = child.stdout.take();
                                                let stderr = child.stderr.take();

                                                // Spawn Log Reading Tasks
                                                if let Some(stdout) = stdout {
                                                    let sender_log = sender_clone.clone();
                                                    let mut reader = BufReader::new(stdout).lines();
                                                    tokio::spawn(async move {
                                                        while let Ok(Some(line)) = reader.next_line().await {
                                                            sender_log.input(AppMsg::Log(line));
                                                        }
                                                    });
                                                }

                                                if let Some(stderr) = stderr {
                                                    let sender_log = sender_clone.clone();
                                                    let mut reader = BufReader::new(stderr).lines();
                                                    tokio::spawn(async move {
                                                        while let Ok(Some(line)) = reader.next_line().await {
                                                            sender_log.input(AppMsg::Log(format!("[ERR] {}", line)));
                                                        }
                                                    });
                                                }

                                                // Wait for process to exit
                                                let _ = child.wait().await;
                                                let duration = start_time.elapsed().as_secs();

                                                sender_clone.input(AppMsg::SessionEnded(profile_name_clone, duration));
                                                sender_clone.input(AppMsg::LaunchCompleted);
                                            }
                                            Err(e) => {
                                                sender_clone.input(AppMsg::Error(format!("Failed to spawn Minecraft process: {}", e)));
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        sender_clone.input(AppMsg::Error(format!("Failed to prepare Minecraft launch: {}", e)));
                                    }
                                }
                            });
                        });
                    }
                }
            }
            AppMsg::GameStarted => {
                if let AppState::Launching { version } = &self.state {
                    self.state = AppState::GameRunning { version: version.clone() };
                }
            }
            AppMsg::DownloadStarted(version) => {
                self.state = AppState::Downloading { version };
            }
            AppMsg::LaunchCompleted => {
                 // Changed behavior: Only go back to home AFTER process exit
                self.state = AppState::Ready { current_section: Section::Home };
                sender.input(AppMsg::ShowToast("Minecraft session ended.".to_string()));
            }
            AppMsg::DownloadCompleted => {
                self.state = AppState::Ready { current_section: Section::Home };
                sender.input(AppMsg::ShowToast("Download completed!".to_string()));
            }
            AppMsg::UsernameChanged(username) => {
                self.input_username = username;
            }
            AppMsg::VersionSelected(version) => {
                // Check if version supports Fabric (1.14+)
                use crate::utils::is_at_least_1_14;
                if is_at_least_1_14(&version) {
                    self.fabric_switch_enabled = true;
                } else {
                    self.fabric_switch_enabled = false;
                    self.input_install_fabric = false;
                }
                self.input_version = Some(version);
            }
            AppMsg::RamChanged(ram) => {
                self.input_ram = ram;
            }
            AppMsg::ToggleFabric(install) => {
                self.input_install_fabric = install;
            }
            AppMsg::SaveProfile => {
                // Validate username
                if self.input_username.trim().is_empty() {
                    self.error_message = Some("Username cannot be empty".to_string());
                    return;
                }

                if self.input_version.is_none() {
                    self.error_message = Some("Please select a Minecraft version".to_string());
                    return;
                }

                let selected_version = self.input_version.clone().unwrap();

                // If Fabric installation is requested
                let is_fabric = self.input_install_fabric && self.fabric_switch_enabled;

                // Normal profile creation
                let profile = crate::models::Profile {
                    username: self.input_username.clone(),
                    version: selected_version.clone(),
                    ram_mb: self.input_ram,
                    playtime_seconds: 0,
                    last_launch: None,
                    is_fabric,
                };

                // Generate profile name (username_version_variant)
                // Use a different name for Fabric profile to avoid collision with vanilla of same version
                // but user might want same name?
                // Let's append _fabric if fabric is selected
                let profile_name = if is_fabric {
                    format!("{}_{}_fabric", profile.username, profile.version)
                } else {
                    format!("{}_{}", profile.username, profile.version)
                };

                // Add to profiles map
                self.profiles.insert(profile_name.clone(), profile);

                // Save to profiles.json
                if let Some(launcher) = &self.launcher {
                    let config_dir = launcher.config.minecraft_dir.clone();
                    let profiles_clone = self.profiles.clone();
                    let sender_clone = sender.clone();
                    std::thread::spawn(move || {
                        let rt = tokio::runtime::Runtime::new().unwrap();
                        rt.block_on(async {
                            let path = config_dir.join("profiles.json");
                            // Ensure directory exists
                            if let Err(e) = tokio::fs::create_dir_all(&config_dir).await {
                                sender_clone.input(AppMsg::Error(format!("Failed to create directory: {}", e)));
                                return;
                            }
                            let json = serde_json::to_string_pretty(&profiles_clone).unwrap_or_default();
                            if let Err(e) = tokio::fs::write(&path, json).await {
                                sender_clone.input(AppMsg::Error(format!("Failed to save profiles: {}", e)));
                            }
                        });
                    });
                }

                // Clear form
                self.input_username.clear();
                self.input_version = None;
                self.input_ram = 2048;
                self.input_install_fabric = false;
                self.fabric_switch_enabled = false;

                sender.input(AppMsg::ShowToast("Profile saved successfully!".to_string()));
                sender.input(AppMsg::NavigateToSection(Section::Home));
            }
            AppMsg::DeleteProfile(profile_name) => {
                // Remove profile from map
                self.profiles.remove(&profile_name);

                // Save to profiles.json
                if let Some(launcher) = &self.launcher {
                    let config_dir = launcher.config.minecraft_dir.clone();
                    let profiles_clone = self.profiles.clone();
                    let sender_clone = sender.clone();
                    std::thread::spawn(move || {
                        let rt = tokio::runtime::Runtime::new().unwrap();
                        rt.block_on(async {
                            let path = config_dir.join("profiles.json");
                            // Ensure directory exists
                            if let Err(e) = tokio::fs::create_dir_all(&config_dir).await {
                                sender_clone.input(AppMsg::Error(format!("Failed to create directory: {}", e)));
                                return;
                            }
                            let json = serde_json::to_string_pretty(&profiles_clone).unwrap_or_default();
                            if let Err(e) = tokio::fs::write(&path, json).await {
                                sender_clone.input(AppMsg::Error(format!("Failed to save profiles: {}", e)));
                            }
                        });
                    });
                }

                sender.input(AppMsg::ShowToast("Profile deleted successfully!".to_string()));
                sender.input(AppMsg::NavigateToSection(Section::Home));
            }
            AppMsg::CancelCreate => {
                sender.input(AppMsg::NavigateToSection(Section::Home));
            }
            AppMsg::BackToMainMenu => {
                sender.input(AppMsg::NavigateToSection(Section::Home));
            }
            AppMsg::CloseWindow => {
                if let Some(window) = &self.window {
                    window.close();
                }
            }
            AppMsg::MinimizeWindow => {
                if let Some(window) = &self.window {
                    window.minimize();
                }
            }
            AppMsg::Error(message) => {
                println!("Error: {}", message);
                self.state = AppState::Error { message };
            }
            AppMsg::ShowToast(message) => {
                if let Some(_window) = &self.window {
                    let _toast = adw::Toast::builder()
                        .title(&message)
                        .timeout(3)
                        .build();
                    println!("Toast: {}", message);
                }
            }
            AppMsg::ShowAboutWindow => {
                if let Some(window) = &self.window {
                    let about = adw::AboutWindow::builder()
                        .application_name("RCraft")
                        .version("v0.8")
                        .developer_name("vdkvdev")
                        .license_type(gtk::License::Gpl30)
                        .website("https://github.com/vdkvdev/rcraft")
                        .copyright("© 2025 vdkvdev")
                        .build();
                    about.set_transient_for(Some(window));
                    about.present();
                }
            }
            AppMsg::ThemeSelected(theme) => {
                self.settings.theme = theme;
                if let Some(_window) = &self.window {
                    let style_manager = adw::StyleManager::default();
                    match theme {
                        Theme::Dark => style_manager.set_color_scheme(adw::ColorScheme::ForceDark),
                        Theme::Light => style_manager.set_color_scheme(adw::ColorScheme::ForceLight),
                        Theme::System => style_manager.set_color_scheme(adw::ColorScheme::Default),
                    }
                }

                // Save settings? Ideally yes.
                let settings_clone = self.settings.clone();
                if let Some(launcher) = &self.launcher {
                     let config_dir = launcher.config.minecraft_dir.clone();
                     std::thread::spawn(move || {
                         let rt = tokio::runtime::Runtime::new().unwrap();
                         rt.block_on(async {
                             let _ = settings_clone.save(&config_dir).await;
                         });
                     });
                }

                sender.input(AppMsg::ShowToast(format!("Theme changed to {}", theme)));
            }
            AppMsg::OpenMinecraftFolder => {
                if let Some(launcher) = &self.launcher {
                    let minecraft_dir = launcher.config.minecraft_dir.clone();
                    std::thread::spawn(move || {
                        let _ = std::process::Command::new("xdg-open")
                            .arg(&minecraft_dir)
                            .spawn();
                    });
                    sender.input(AppMsg::ShowToast("Opening Minecraft folder...".to_string()));
                }
            }
            AppMsg::RequestDeleteProfile(profile_name) => {
                if let Some(window) = &self.window {
                    let dialog = adw::MessageDialog::builder()
                        .heading("Delete Profile?")
                        .body(format!("Are you sure you want to delete profile '{}'? This action cannot be undone.", profile_name))
                        .transient_for(window)
                        .modal(true)
                        .build();

                    dialog.add_response("cancel", "Cancel");
                    dialog.add_response("delete", "Delete");

                    dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
                    dialog.set_default_response(Some("cancel"));
                    dialog.set_close_response("cancel");

                    let sender_clone = sender.clone();
                    let profile_name_clone = profile_name.clone();

                    dialog.connect_response(None, move |d, response| {
                        if response == "delete" {
                            sender_clone.input(AppMsg::DeleteProfile(profile_name_clone.clone()));
                        }
                        d.close();
                    });

                    dialog.present();
                }
            }
            AppMsg::SessionEnded(profile_name, duration) => {
                if let Some(profile) = self.profiles.get_mut(&profile_name) {
                    profile.playtime_seconds += duration;
                    profile.last_launch = Some(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs());

                    // Save profiles
                     if let Some(launcher) = &self.launcher {
                        let config_dir = launcher.config.minecraft_dir.clone();
                        let profiles_clone = self.profiles.clone();
                        let sender_clone = sender.clone();
                        std::thread::spawn(move || {
                            let rt = tokio::runtime::Runtime::new().unwrap();
                            rt.block_on(async {
                                let path = config_dir.join("profiles.json");
                                let json = serde_json::to_string_pretty(&profiles_clone).unwrap_or_default();
                                if let Err(e) = tokio::fs::write(&path, json).await {
                                    sender_clone.input(AppMsg::Error(format!("Failed to save profiles: {}", e)));
                                }
                            });
                        });
                    }
                }
            }
            _ => {
                // Handle other messages
            }
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        // Update UI based on model state
        match &self.state {
            AppState::Loading => {
                widgets.content_stack.set_visible_child_name("loading");
                widgets.loading_spinner.start();
            }
            AppState::Ready { current_section } => {
                widgets.loading_spinner.stop();

                // Enable buttons
                widgets.home_button.set_sensitive(true);
                widgets.create_sidebar_button.set_sensitive(true);
                widgets.settings_button.set_sensitive(true);
                widgets.logs_button.set_sensitive(true);

                // Clear previous suggested-action classes first
                widgets.home_button.remove_css_class("suggested-action");
                widgets.create_sidebar_button.remove_css_class("suggested-action");
                widgets.settings_button.remove_css_class("suggested-action");
                widgets.logs_button.remove_css_class("suggested-action");

                // Update sidebar button styles based on current section
                match current_section {
                    Section::Home => {
                        widgets.home_button.add_css_class("suggested-action");
                    }
                    Section::CreateInstance => {
                        widgets.create_sidebar_button.add_css_class("suggested-action");
                    }
                    Section::Settings => {
                        widgets.settings_button.add_css_class("suggested-action");
                    }
                    Section::Logs => {
                         widgets.logs_button.add_css_class("suggested-action");
                    }
                }

                // Update content based on current section
                match current_section {
                    Section::Home => {
                        widgets.content_stack.set_visible_child_name("home");
                        update_profile_list(&widgets.profile_list, &self.profiles, &self.sender);
                    }
                    Section::CreateInstance => {
                        widgets.content_stack.set_visible_child_name("create");
                        widgets.fabric_switch.set_active(self.input_install_fabric);
                        widgets.fabric_switch.set_sensitive(self.fabric_switch_enabled);
                    }
                    Section::Settings => {
                        widgets.content_stack.set_visible_child_name("settings");
                    }
                    Section::Logs => {
                         widgets.content_stack.set_visible_child_name("logs");
                         // Scroll to bottom of logs?
                         // self.logs.end_iter(); // buffer scroll is auto if implemented or we can do it here via view.
                    }
                }
            }
            AppState::Downloading { .. } => {
                widgets.content_stack.set_visible_child_name("loading");
                widgets.loading_page.set_title("Downloading...");
                widgets.loading_page.set_description(Some("Downloading game files. This may take a few minutes."));
                widgets.loading_spinner.start();

                // Disable sidebar buttons
                widgets.home_button.set_sensitive(false);
                widgets.create_sidebar_button.set_sensitive(false);
                widgets.settings_button.set_sensitive(false);
                widgets.logs_button.set_sensitive(false);
            }
            AppState::Launching { .. } => {
                widgets.content_stack.set_visible_child_name("loading");
                widgets.loading_page.set_title("Launching...");
                widgets.loading_page.set_description(Some("If this is your first time launching, it may take longer as files are downloaded."));
                widgets.loading_spinner.start();

                // Disable sidebar buttons
                widgets.home_button.set_sensitive(false);
                widgets.create_sidebar_button.set_sensitive(false);
                widgets.settings_button.set_sensitive(false);
                widgets.logs_button.set_sensitive(false);
            }
            AppState::GameRunning { .. } => {
                widgets.content_stack.set_visible_child_name("loading");
                widgets.loading_page.set_title("Game Running");
                widgets.loading_page.set_description(Some("Minecraft is running."));
                widgets.loading_spinner.start();

                // Disable sidebar buttons
                widgets.home_button.set_sensitive(false);
                widgets.create_sidebar_button.set_sensitive(false);
                widgets.settings_button.set_sensitive(false);
                widgets.logs_button.set_sensitive(false);
            }
            AppState::Error { message } => {
                widgets.error_label.set_text(message);
                widgets.content_stack.set_visible_child_name("error");
            }
        }

        // Update common widgets
        widgets.logs_button.set_visible(self.settings.nerd_mode);
        widgets.nerd_mode_switch.set_active(self.settings.nerd_mode);

        let theme_index = match self.settings.theme {
            Theme::System => 0,
            Theme::Light => 1,
            Theme::Dark => 2,
        };
        if widgets.theme_combo.selected() != theme_index {
            widgets.theme_combo.set_selected(theme_index);
        }
    }
}

// ============================================================================
// UI Helper Functions
// ============================================================================

fn create_sidebar(sender: &ComponentSender<AppModel>) -> (NavigationPage, gtk::Button, gtk::Button, gtk::Button, gtk::Button) {
    let sidebar_content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .width_request(200)
        .spacing(0)
        .vexpand(true)
        .hexpand(true)
        .halign(gtk::Align::Fill)
        .valign(gtk::Align::Fill)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();

    // Navigation buttons
    let home_button = gtk::Button::builder()
        .label("Home")
        .halign(gtk::Align::Fill)
        .hexpand(true)
        .height_request(40)
        .margin_top(6)
        .margin_bottom(6)
        .build();

    // Create New Profile button
    let create_button = gtk::Button::builder()
        .label("New Profile")
        .halign(gtk::Align::Fill)
        .hexpand(true)
        .height_request(40)
        .margin_top(6)
        .margin_bottom(6)
        .build();

    let settings_button = gtk::Button::builder()
        .label("Settings")
        .halign(gtk::Align::Fill)
        .hexpand(true)
        .height_request(40)
        .margin_top(6)
        .margin_bottom(6)
        .build();

    // Logs button (hidden by default)
    let logs_button = gtk::Button::builder()
        .label("Logs")
        .halign(gtk::Align::Fill)
        .hexpand(true)
        .height_request(40)
        .margin_top(6)
        .margin_bottom(6)
        .visible(false)
        .build();

    // Connect button signals
    let sender_clone = sender.clone();
    home_button.connect_clicked(move |_| {
        sender_clone.input(AppMsg::NavigateToSection(Section::Home));
    });

    let sender_clone = sender.clone();
    create_button.connect_clicked(move |_| {
        sender_clone.input(AppMsg::NavigateToSection(Section::CreateInstance));
    });

    let sender_clone = sender.clone();
    settings_button.connect_clicked(move |_| {
        sender_clone.input(AppMsg::NavigateToSection(Section::Settings));
    });

    let sender_clone = sender.clone();
    logs_button.connect_clicked(move |_| {
        sender_clone.input(AppMsg::NavigateToSection(Section::Logs));
    });

    // Add buttons to sidebar (Home > Create > Settings)
    sidebar_content.append(&home_button);
    sidebar_content.append(&create_button);
    sidebar_content.append(&settings_button);
    sidebar_content.append(&logs_button);

    // Add spacer to push content to top
    let spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    spacer.set_vexpand(true);
    sidebar_content.append(&spacer);

    // Version Label
    let version_label = gtk::Label::builder()
        .label("v0.8 (beta)")
        .css_classes(vec!["dim-label".to_string(), "subtitle".to_string()])
        .margin_bottom(12)
        .build();
    sidebar_content.append(&version_label);

    // Create navigation page
    let sidebar_page = adw::NavigationPage::builder()
        .title("Navigation")
        .child(&sidebar_content)
        .vexpand(true)
        .hexpand(true)
        .build();

    // Remove any default background from NavigationPage
    sidebar_page.set_css_classes(&["flat"]);

    (sidebar_page, home_button, create_button, settings_button, logs_button)
}

fn create_loading_widgets() -> (StatusPage, gtk::Spinner, gtk::Label) {
    let status_page = adw::StatusPage::builder()
        .title("Loading RCraft")
        .description("Please wait while the launcher initializes...")
        .build();

    let spinner = gtk::Spinner::new();
    spinner.start();
    spinner.set_size_request(48, 48);

    let label = gtk::Label::new(Some("Initializing..."));

    // Add spinner to status page
    status_page.set_child(Some(&spinner));

    (status_page, spinner, label)
}

fn create_home_page(_sender: &ComponentSender<AppModel>, profile_list: &gtk::ListBox) -> gtk::Box {
    let main_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .hexpand(true)
        .vexpand(true)
        .build();

    let content_container = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(24)
        .hexpand(true)
        .halign(gtk::Align::Fill)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();

    // Title label
    let title_label = gtk::Label::builder()
        .label("Home")
        .halign(gtk::Align::Start)
        .css_classes(vec!["title-1".to_string()])
        .build();

    content_container.append(&title_label);

    // Use the provided profile list
    profile_list.set_selection_mode(gtk::SelectionMode::None);
    profile_list.add_css_class("boxed-list");

    content_container.append(profile_list);

    main_box.append(&content_container);
    main_box
}

fn create_create_instance_page(
    sender: &ComponentSender<AppModel>,
    username_entry: &EntryRow,
    version_combo: &ComboRow,
    ram_scale: &SpinRow,
    fabric_switch: &adw::SwitchRow,
) -> gtk::Box {
    let main_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .hexpand(true)
        .vexpand(true)
        .build();

    let content_container = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(24)
        .hexpand(true)
        .halign(gtk::Align::Fill)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();

    // Title label
    let title_label = gtk::Label::builder()
        .label("New Profile")
        .halign(gtk::Align::Start)
        .css_classes(vec!["title-1".to_string()])
        .build();

    content_container.append(&title_label);

    // List box for inputs
    let input_list = gtk::ListBox::new();
    input_list.add_css_class("boxed-list");
    input_list.set_selection_mode(gtk::SelectionMode::None);
    input_list.set_hexpand(true);
    input_list.set_halign(gtk::Align::Fill);

    // Username entry
    let sender_clone = sender.clone();
    username_entry.connect_changed(move |entry: &adw::EntryRow| {
        let text = entry.text();
        sender_clone.input(AppMsg::UsernameChanged(text.to_string()));
    });

    // Wrap username entry in row if needed? No, EntryRow is a widget.
    // Actually, EntryRow, ComboRow, SpinRow are PreferencesRow which are ListBoxRow.
    // So we can append them directly to ListBox? Yes.

    // Version combo
    let sender_clone = sender.clone();
    version_combo.connect_notify(Some("selected"), move |combo: &adw::ComboRow, _| {
        if let Some(item) = combo.selected_item() {
            if let Some(string_obj) = item.downcast_ref::<gtk::StringObject>() {
                let version = string_obj.string().to_string();
                sender_clone.input(AppMsg::VersionSelected(version));
            }
        }
    });

    // RAM adjustment
    let sender_clone = sender.clone();
    ram_scale.adjustment().connect_value_changed(move |adjustment: &gtk::Adjustment| {
        sender_clone.input(AppMsg::RamChanged(adjustment.value() as u32));
    });

    let sender_clone = sender.clone();
    fabric_switch.connect_active_notify(move |switch| {
        sender_clone.input(AppMsg::ToggleFabric(switch.is_active()));
    });

    // Configure rows
    username_entry.set_hexpand(true);
    version_combo.set_hexpand(true);
    ram_scale.set_hexpand(true);
    fabric_switch.set_hexpand(true);

    // Ensure they don't have hover effects if they are inputs?
    // Usually input rows in boxed list are fine as they are.
    // But let's check if we need to set activatable(false) for interaction?
    // EntryRow needs interaction. ComboRow needs interaction.
    // Default behavior for PreferencesRow is usually fine.

    input_list.append(username_entry);
    input_list.append(version_combo);
    input_list.append(ram_scale);
    input_list.append(fabric_switch);

    content_container.append(&input_list);

    // Buttons
    let button_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .halign(gtk::Align::Fill)
        .margin_top(12)
        .build();

    let save_button = gtk::Button::builder()
        .label("Save Profile")
        .css_classes(vec!["suggested-action".to_string()])
        .height_request(40)
        .hexpand(true)
        .build();

    let sender_clone = sender.clone();
    save_button.connect_clicked(move |_| {
        sender_clone.input(AppMsg::SaveProfile);
    });



    button_box.append(&save_button);

    content_container.append(&button_box);

    main_box.append(&content_container);
    main_box
}

fn create_settings_page(sender: &ComponentSender<AppModel>, nerd_mode_switch: &adw::SwitchRow) -> (gtk::Box, adw::ComboRow) {
    let main_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .hexpand(true)
        .vexpand(true)
        .build();

    let content_container = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(24)
        .hexpand(true)
        .halign(gtk::Align::Fill)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();

    // Title label
    let title_label = gtk::Label::builder()
        .label("Settings")
        .halign(gtk::Align::Start)
        .css_classes(vec!["title-1".to_string()])
        .build();

    content_container.append(&title_label);

    // Create a list box for settings rows
    let settings_list = gtk::ListBox::new();
    settings_list.add_css_class("boxed-list");
    settings_list.set_selection_mode(gtk::SelectionMode::None);
    settings_list.set_hexpand(true);
    settings_list.set_halign(gtk::Align::Fill);

    // Nerd Mode switch configuration
    nerd_mode_switch.set_title("Nerd Mode");
    nerd_mode_switch.set_subtitle("Enable advanced features and logs");
    nerd_mode_switch.set_hexpand(true);
    nerd_mode_switch.set_halign(gtk::Align::Fill);

    let sender_clone = sender.clone();
    nerd_mode_switch.connect_active_notify(move |switch| {
        sender_clone.input(AppMsg::ToggleNerdMode(switch.is_active()));
    });

    // Theme selection
    let theme_row = adw::ComboRow::builder()
        .title("Theme")
        .hexpand(true)
        .halign(gtk::Align::Fill)
        .build();

    let theme_model = gtk::StringList::new(&["System", "Light", "Dark"]);
    theme_row.set_model(Some(&theme_model));

    let sender_clone = sender.clone();
    theme_row.connect_notify(Some("selected"), move |combo, _| {
        let theme = match combo.selected() {
            2 => Theme::Dark,
            1 => Theme::Light,
            _ => Theme::System,
        };
        sender_clone.input(AppMsg::ThemeSelected(theme));
    });

    // Open Minecraft folder button
    let folder_row = adw::ActionRow::builder()
        .title("Open Minecraft Folder")
        .hexpand(true)
        .halign(gtk::Align::Fill)
        .build();

    let folder_button = gtk::Button::builder()
        .label("Open")
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .build();

    let sender_clone = sender.clone();
    folder_button.connect_clicked(move |_| {
        sender_clone.input(AppMsg::OpenMinecraftFolder);
    });

    folder_row.add_suffix(&folder_button);

    folder_row.set_activatable(false);

    // Add rows to list box
    settings_list.append(&theme_row);
    settings_list.append(&folder_row);
    settings_list.append(nerd_mode_switch);

    // Add list box to main content
    content_container.append(&settings_list);

    // About Section
    // Title removed

    let about_list = gtk::ListBox::new();
    about_list.add_css_class("boxed-list");
    about_list.set_selection_mode(gtk::SelectionMode::None);
    about_list.set_hexpand(true);
    about_list.set_halign(gtk::Align::Fill);

    // Version Row
    let version_row = adw::ActionRow::builder()
        .title("Version")
        .subtitle("v0.8")
        .build();

    // Developer Row
    let dev_row = adw::ActionRow::builder()
        .title("Developer")
        .subtitle("vdkvdev")
        .build();

     // Source Code Row
    let repo_row = adw::ActionRow::builder()
        .title("Source Code")
        .subtitle("https://github.com/vdkvdev/rcraft")
        .build();
    
    // Make repo row clickable or have a button
    // Let's add a button to open it
    let repo_button = gtk::Button::builder()
        .label("View")
        .valign(gtk::Align::Center)
        .build();
    
    // Add logic to open link
    repo_button.connect_clicked(move |_| {
         let _ = std::process::Command::new("xdg-open")
             .arg("https://github.com/vdkvdev/rcraft")
             .spawn();
    });

    repo_row.add_suffix(&repo_button);
    repo_row.set_activatable(false); // only button is interactive

    about_list.append(&version_row);
    about_list.append(&dev_row);
    about_list.append(&repo_row);

    content_container.append(&about_list);

    main_box.append(&content_container);
    (main_box, theme_row)
}

// ============================================================================
// Profile List Management
// ============================================================================

fn update_profile_list(profile_list: &gtk::ListBox, profiles: &std::collections::HashMap<String, crate::models::Profile>, sender: &relm4::ComponentSender<AppModel>) {
    // Clear existing children
    while let Some(child) = profile_list.first_child() {
        profile_list.remove(&child);
    }

    if profiles.is_empty() {
        let no_profiles_label = gtk::Label::builder()
            .label("No profiles yet. Create one to get started!")
            .halign(gtk::Align::Center)
            .margin_top(24)
            .margin_bottom(24)
            .build();
        profile_list.append(&no_profiles_label);
    } else {
        for (name, profile) in profiles {
            let row = create_profile_row(name, profile, sender);
            profile_list.append(&row);
        }
    }
}

fn create_profile_row(name: &str, profile: &crate::models::Profile, sender: &relm4::ComponentSender<AppModel>) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();

    let box_container = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .margin_start(12)
        .margin_end(12)
        .margin_top(6)
        .margin_bottom(6)
        .build();

    // Profile info
    let info_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .hexpand(true)
        .halign(gtk::Align::Start)
        .build();

    let name_label = gtk::Label::builder()
        .label(&profile.username)
        .halign(gtk::Align::Start)
        .css_classes(vec!["title-4".to_string()])
        .build();

    // Format playtime
    let hours = profile.playtime_seconds / 3600;
    let minutes = (profile.playtime_seconds % 3600) / 60;
    let playtime_str = if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else if minutes > 0 {
        format!("{}m", minutes)
    } else {
        "No playtime".to_string()
    };

    let version_display = if profile.is_fabric {
        format!("{} (Fabric)", profile.version)
    } else {
        profile.version.clone()
    };

    let details_label = gtk::Label::builder()
        .label(format!("{} • {} MB • {}", version_display, profile.ram_mb, playtime_str))
        .halign(gtk::Align::Start)
        .css_classes(vec!["dim-label".to_string()])
        .build();

    info_box.append(&name_label);
    info_box.append(&details_label);

    // Buttons
    let button_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .build();

    let launch_button = gtk::Button::builder()
        .label("Launch")
        .css_classes(vec!["suggested-action".to_string()])
        .valign(gtk::Align::Center)
        .build();

    let sender_clone = sender.clone();
    let name_clone = name.to_string();
    launch_button.connect_clicked(move |_| {
        sender_clone.input(AppMsg::LaunchProfile(name_clone.clone()));
    });

    let delete_button = gtk::Button::builder()
        .icon_name("user-trash-symbolic")
        .css_classes(vec!["destructive-action".to_string()])
        .valign(gtk::Align::Center)
        .build();

    let sender_clone = sender.clone();
    let name_clone = name.to_string();
    delete_button.connect_clicked(move |_| {
        sender_clone.input(AppMsg::RequestDeleteProfile(name_clone.clone()));
    });

    button_box.append(&launch_button);
    button_box.append(&delete_button);

    box_container.append(&info_box);
    box_container.append(&button_box);

    row.set_child(Some(&box_container));
    row.set_activatable(false);
    row
}

// ============================================================================

fn create_logs_page(_sender: &ComponentSender<AppModel>, logs_buffer: &gtk::TextBuffer) -> (gtk::ScrolledWindow, gtk::TextView) {
    let scrolled_window = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .hexpand(true)
        .build();

    let text_view = gtk::TextView::builder()
        .buffer(logs_buffer)
        .editable(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::Word)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();

    scrolled_window.set_child(Some(&text_view));

    (scrolled_window, text_view)
}
