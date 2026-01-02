use adw::prelude::*;
use adw::{self, NavigationSplitView, NavigationPage, StatusPage, EntryRow, ComboRow, SpinRow};
use relm4::gtk;
use relm4::{ComponentParts, ComponentSender, SimpleComponent, RelmWidgetExt};
use std::collections::{HashMap, VecDeque};

use std::io::{Read}; // Added Read trait for zip
use zip::ZipArchive;
use std::fs::File;
use tokio::runtime::Runtime;
use gtk::prelude::{AdjustmentExt, Cast, WidgetExt, ObjectExt};

use crate::models::{MinecraftVersion, Profile};
use crate::settings::Settings;
use crate::models::{Section, Theme, ModSearchResult};
use crate::launcher::MinecraftLauncher;
use crate::modrinth_client::ModrinthClient;
use tokio::io::{AsyncBufReadExt, BufReader};
use crate::mods_ui::{create_mods_page, create_mod_search_result_row};

// ============================================================================
// Application Model
// ============================================================================

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
    pub download_dots: u8,
    pub sidebar_collapsed: bool,

    pub versions_updated: bool,
    pub version_list_model: Option<gtk::StringList>,
    pub is_searching: bool,

    // Mods UI State
    pub mod_search_results: Vec<ModSearchResult>,
    pub mod_search_entry: Option<gtk::SearchEntry>,
    pub mod_search_spinner: Option<gtk::Spinner>,
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

    // Component sender for UI updates
    pub sender: ComponentSender<AppModel>,
}

impl AppModel {
     // Helper to update button state based on installation status
    fn update_mod_button_state(&self, project_id: &str) {
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
    ShowAboutWindow,
    ThemeSelected(Theme),
    ToggleNerdMode(bool),
    ToggleHideMods(bool),
    ToggleSidebar,
    Log(String),
    MinimizeWindow,
    CloseWindow,
    Error(String),
    RequestDeleteProfile(String),
    SettingsLoaded(Settings),
    SessionEnded(String, u64),
    ColorsLoaded(Vec<String>), // Placeholder example
    RefreshInstalledMods,
    SelectModProfile(String),
    // Modrinth Messages
    SearchMods(String),
    ModsSearched(Result<Vec<ModSearchResult>, String>),
    InstallMod(String), // Project ID
    UninstallMod(String), // Filename
    DownloadModIcon(String, String), // Project ID, URL
    ModIconDownloaded(String, String), // project_id, path
    ProcessIconQueue,
    ModActionButtonClicked(String), // project_id (Toggle Install/Uninstall)
    ModInstallFinished(String, bool), // project_id, success
    ModUninstallFinished(String), // project_id
    RegisterInstalledMod(String, String), // project_id, filename
    ShowToast(String),
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
    mods_page: gtk::Box,
    logs_page: gtk::ScrolledWindow,
    loading_page: adw::StatusPage,

    // Home page widgets
    profile_list: gtk::ListBox,
    username_entry: adw::EntryRow,
    version_combo: adw::ComboRow,
    ram_scale: adw::SpinRow,
    fabric_switch: adw::SwitchRow,
    nerd_mode_switch: adw::SwitchRow,
    hide_mods_switch: adw::SwitchRow,

    // Buttons
    launch_button: gtk::Button,
    create_button: gtk::Button,
    delete_button: gtk::Button,
    save_button: gtk::Button,
    cancel_button: gtk::Button,

    // Sidebar buttons
    home_button: gtk::Button,
    create_sidebar_button: gtk::Button,
    mods_button: gtk::Button,
    settings_button: gtk::Button,
    logs_button: gtk::Button,

    // Mods widgets
    mod_profile_dropdown: gtk::DropDown,
    mod_search_stack: gtk::Stack,

    // Sidebar button labels (for visibility)
    home_label: gtk::Label,
    create_label: gtk::Label,
    mods_label: gtk::Label,
    settings_label: gtk::Label,
    logs_label: gtk::Label,

    // Sidebar button boxes (for alignment)
    home_box: gtk::Box,
    create_box: gtk::Box,
    mods_box: gtk::Box,
    settings_box: gtk::Box,
    logs_box: gtk::Box,

    // Sidebar Toggle
    sidebar_toggle_button: gtk::Button,

    // Settings widgets
    theme_combo: adw::ComboRow,

    // Status/error labels
    status_label: gtk::Label,
    error_label: gtk::Label,

    // Loading widgets
    loading_spinner: gtk::Spinner,
    loading_label: gtk::Label,

    // Toast Overlay
    toast_overlay: adw::ToastOverlay,

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
            modrinth: ModrinthClient::new(),
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
            sidebar_collapsed: false,
            is_searching: false,

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

            mod_search_results: Vec::new(),
            mod_search_entry: None,
            mod_search_spinner: None, // We don't need this stored in model anymore? Or we keep it?
            // Wait, we need to toggle the stack, not the spinner directly.
            // So we can remove this or keep it as None.
            // But update_view needs to know IF we are searching.
            // We'll add is_searching state.
            mod_browse_list: None,
            mod_installed_list: None,
            selected_mod_profile: None,
            mod_profile_list_model: None,

            // Map ProjectID -> Filename
            // This tracks mods installed IN THIS SESSION (or known).
            // Persistence requires saving this mapping or scanning metadata.
            installed_mods: HashMap::new(),

            toast_overlay: None,



            icon_download_queue: VecDeque::new(),
            is_downloading_icon: false,

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
        navigation_split_view.set_min_sidebar_width(60.0);





        // Create sidebar
        let (sidebar, home_button, create_sidebar_button, mods_button, settings_button, logs_button, home_label, create_label, mods_label, settings_label, logs_label, home_box, create_box, mods_box, settings_box, logs_box) = create_sidebar(&sender);
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

        let hide_mods_switch = adw::SwitchRow::builder()
            .title("Hide Mods Button")
            .subtitle("Hide the Mods button in the sidebar")
            .build();

        let profile_list = gtk::ListBox::new();
        let loading_widgets = create_loading_widgets();

        // Create pages for each section
        // Create pages for each section
        let home_page = create_home_page(&sender, &profile_list);
        let create_page = create_create_instance_page(&sender, &username_entry, &version_combo, &ram_scale, &fabric_switch);
        let (settings_page, theme_combo) = create_settings_page(&sender, &nerd_mode_switch, &hide_mods_switch);
        let (logs_page, logs_view) = create_logs_page(&sender, &model.logs);
        let (mods_page, mod_search_entry, mod_search_button, mod_search_stack, mod_installed_list, mod_browse_list, mod_profile_dropdown) = create_mods_page(&sender);

        // Store references to separate widgets for logic
        model.mod_search_entry = Some(mod_search_entry.clone());
        // model.mod_search_spinner = Some(mod_search_spinner.clone()); // Removed
        model.mod_browse_list = Some(mod_browse_list.clone());
        model.mod_installed_list = Some(mod_installed_list.clone());

        // Connect Search Logic
        // Connect Search Logic
        let sender_clone = sender.clone();

        mod_search_entry.connect_activate(move |entry| {
             let text = entry.text().to_string();
             if !text.is_empty() {
                 sender_clone.input(AppMsg::SearchMods(text));
             }
        });

        let sender_clone = sender.clone();
        let search_entry_clone = mod_search_entry.clone();
        mod_search_button.connect_clicked(move |_| {
             let text = search_entry_clone.text().to_string();
             if !text.is_empty() {
                 sender_clone.input(AppMsg::SearchMods(text));
             }
        });

        content_stack.add_titled(&home_page, Some("home"), "Home");
        content_stack.add_titled(&create_page, Some("create"), "Create");
        content_stack.add_titled(&mods_page, Some("mods"), "Mods");
        content_stack.add_titled(&settings_page, Some("settings"), "Settings");
        content_stack.add_titled(&logs_page, Some("logs"), "Logs");
        content_stack.add_titled(&loading_widgets.0, Some("loading"), "Loading");

        // Initialize Nerd Mode State
        logs_button.set_visible(model.settings.nerd_mode);

        // Initialize Hide Mods State
        // If hidden is true, visible is false
        mods_button.set_visible(!model.settings.hide_mods_button);

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


        // Toast Overlay
        let toast_overlay = adw::ToastOverlay::new();
        toast_overlay.set_child(Some(&navigation_split_view));
        model.toast_overlay = Some(toast_overlay.clone());

        // Create header bar
        let header_bar = adw::HeaderBar::new();
        header_bar.set_show_end_title_buttons(true);
        header_bar.set_title_widget(Some(&adw::WindowTitle::new("RCraft", "")));

        // Sidebar toggle button
        let sidebar_toggle_button = gtk::Button::builder()
            .icon_name("sidebar-show-symbolic")
            .tooltip_text("Toggle Sidebar")
            .build();

        let sender_clone = sender.clone();
        sidebar_toggle_button.connect_clicked(move |_| {
            sender_clone.input(AppMsg::ToggleSidebar);
        });

        header_bar.pack_start(&sidebar_toggle_button);

        // Create vertical box to hold header bar and navigation split view
        let main_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        main_box.set_vexpand(true);
        main_box.set_hexpand(true);
        main_box.set_valign(gtk::Align::Fill);
        main_box.append(&header_bar);
        main_box.append(&toast_overlay);

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
            mods_page,
            logs_page,
            loading_page: loading_widgets.0,

            mod_profile_dropdown,
            mod_search_stack,

            profile_list,
            username_entry,
            version_combo,
            ram_scale,
            fabric_switch,

            nerd_mode_switch,
            hide_mods_switch,
            launch_button: gtk::Button::with_label("Launch"),
            create_button: gtk::Button::with_label("Create"),
            delete_button: gtk::Button::with_label("Delete"),
            save_button: gtk::Button::with_label("Save"),
            cancel_button: gtk::Button::with_label("Cancel"),
            home_button,
            create_sidebar_button,
            mods_button,
            settings_button,
            logs_button,
            home_label,
            create_label,
            mods_label,
            settings_label,
            logs_label,
            home_box,
            create_box,
            mods_box,
            settings_box,
            logs_box,
            sidebar_toggle_button,
            theme_combo,
            status_label: gtk::Label::new(None),
            error_label,
            loading_spinner: loading_widgets.1,
            loading_label: loading_widgets.2,
            toast_overlay,
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
                self.sidebar_collapsed = settings.sidebar_collapsed;
                self.sender.input(AppMsg::ToggleNerdMode(settings.nerd_mode));
                self.sender.input(AppMsg::ToggleHideMods(settings.hide_mods_button));
                self.sender.input(AppMsg::ThemeSelected(settings.theme));
            }
            AppMsg::ToggleHideMods(hide) => {
                self.settings.hide_mods_button = hide;

                // Update UI (sender not needed as update loop handles widgets update?
                // Wait, relm4 simple component update loop updates logic, then view updates widgets?
                // No, we need to update widgets here or in view.
                // In simple component, we update model state, then `update_view` runs.
                // So we just update model state. But we also need to save settings.

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
            AppMsg::ToggleSidebar => {
                self.sidebar_collapsed = !self.sidebar_collapsed;
                self.settings.sidebar_collapsed = self.sidebar_collapsed;

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

                        // Populate Mod Profile Dropdown
                        let mut display_strings = Vec::new();
                        // Filter for Fabric profiles only and use "Username - Version" format
                        let mut sorted_keys: Vec<&String> = self.profiles.keys().collect();
                        sorted_keys.sort();

                        for key in sorted_keys {
                             if let Some(profile) = self.profiles.get(key) {
                                 if profile.is_fabric {
                                     display_strings.push(format!("{} - {}", profile.username, profile.version));
                                 }
                             }
                        }

                        let display_strs: Vec<&str> = display_strings.iter().map(|s| s.as_str()).collect();
                        let model = gtk::StringList::new(&display_strs);
                        self.mod_profile_list_model = Some(model);

                        // Select first if available
                        if let Some(first_str) = display_strings.first() {
                             if self.selected_mod_profile.is_none() {
                                 if let Some((name, version)) = first_str.rsplit_once(" - ") {
                                     // Reconstruct key: username_version_fabric
                                     let key = format!("{}_{}_fabric", name, version);
                                     // Verify it exists (it should)
                                     if self.profiles.contains_key(&key) {
                                          self.selected_mod_profile = Some(key);
                                          sender.input(AppMsg::RefreshInstalledMods);
                                     }
                                 }
                             }
                        }
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
                                    // Check/Install Fabric logic...
                                     let fabric_installed = if let Ok(mut entries) = tokio::fs::read_dir(&launcher_clone.config.versions_dir).await {
                                        let mut found = None;
                                        while let Ok(Some(entry)) = entries.next_entry().await {
                                            if let Some(name) = entry.file_name().to_str() {
                                                if name.contains("fabric-loader") && name.ends_with(&format!("-{}", profile_clone.version)) {
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

                                println!("[Debug] Checking Vanilla JAR at: {:?}", jar_path);

                                if !jar_path.exists() {
                                    println!("[Debug] Vanilla JAR not found. Attempting download for version: {}", vanilla_version_id);
                                     match launcher_clone.get_available_versions().await {
                                        Ok(versions) => {
                                             if let Some(v) = versions.into_iter().find(|v| v.id == *vanilla_version_id) {
                                                  // Change state to downloading
                                                  sender_clone.input(AppMsg::DownloadStarted(vanilla_version_id.clone()));
                                                  println!("[Debug] Found version in manifest. Starting download...");

                                                  if let Err(e) = launcher_clone.download_version(&v).await {
                                                      let err_msg = format!("Failed to download vanilla version: {}", e);
                                                      println!("[Error] {}", err_msg);
                                                      sender_clone.input(AppMsg::Error(err_msg));
                                                      return;
                                                  }
                                                  println!("[Debug] Download completed successfully.");
                                             } else {
                                                 let err_msg = format!("Version {} not found in manifest", vanilla_version_id);
                                                 println!("[Error] {}", err_msg);
                                                 sender_clone.input(AppMsg::Error(err_msg));
                                                 return;
                                             }
                                        }
                                        Err(e) => {
                                             let err_msg = format!("Failed to fetch version manifest: {}", e);
                                             println!("[Error] {}", err_msg);
                                             sender_clone.input(AppMsg::Error(err_msg));
                                             return;
                                        }
                                    }
                                } else {
                                    println!("[Debug] Vanilla JAR exists.");
                                }

                                // Determine Game Directory
                                let game_dir = if let Some(dir) = &profile_clone.game_dir {
                                    std::path::PathBuf::from(dir)
                                } else {
                                    // Default to isolated directory: instances/<profile_name>
                                    launcher_clone.config.minecraft_dir.join("instances").join(&profile_name_clone)
                                };

                                // Ensure game directory exists
                                if !game_dir.exists() {
                                    if let Err(e) = std::fs::create_dir_all(&game_dir) {
                                         sender_clone.input(AppMsg::Error(format!("Failed to create instance directory: {}", e)));
                                         return;
                                    }
                                }

                                // Launch Minecraft
                                // Launching... (Toast removed)
                                println!("[Debug] Invoking launch_minecraft...");
                                match launcher_clone.launch_minecraft(
                                    &version_to_launch,
                                    &profile_clone.username,
                                    profile_clone.ram_mb,
                                    &game_dir
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

            }
            AppMsg::DownloadCompleted => {
                self.state = AppState::Ready { current_section: Section::Home };

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
                    game_dir: None,
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

                // Update Mod Profile Dropdown
                let mut display_strings = Vec::new();
                let mut sorted_keys: Vec<&String> = self.profiles.keys().collect();
                sorted_keys.sort();

                for key in sorted_keys {
                     if let Some(profile) = self.profiles.get(key) {
                         display_strings.push(format!("{} - {}", key, profile.version));
                     }
                }
                let display_strs: Vec<&str> = display_strings.iter().map(|s| s.as_str()).collect();
                let model = gtk::StringList::new(&display_strs);
                self.mod_profile_list_model = Some(model);

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


                sender.input(AppMsg::NavigateToSection(Section::Home));
            }
            AppMsg::DeleteProfile(profile_name) => {
                // Remove profile from map
                self.profiles.remove(&profile_name);

                // Update Mod Profile Dropdown
                let mut display_strings = Vec::new();
                let mut sorted_keys: Vec<&String> = self.profiles.keys().collect();
                sorted_keys.sort();

                for key in sorted_keys {
                     if let Some(profile) = self.profiles.get(key) {
                         display_strings.push(format!("{} - {}", key, profile.version));
                     }
                }
                let display_strs: Vec<&str> = display_strings.iter().map(|s| s.as_str()).collect();
                let model = gtk::StringList::new(&display_strs);
                self.mod_profile_list_model = Some(model);

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


            }
            AppMsg::OpenMinecraftFolder => {
                if let Some(launcher) = &self.launcher {
                    let minecraft_dir = launcher.config.minecraft_dir.clone();
                    std::thread::spawn(move || {
                        let _ = std::process::Command::new("xdg-open")
                            .arg(&minecraft_dir)
                            .spawn();
                    });

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
            AppMsg::RefreshInstalledMods => {
                 if let Some(list) = &self.mod_installed_list {
                      while let Some(child) = list.first_child() {
                          list.remove(&child);
                      }

                      // Determine mods directory
                      let mods_dir = if let Some(profile_name) = &self.selected_mod_profile {
                          if let Some(profile) = self.profiles.get(profile_name) {
                              if let Some(dir) = &profile.game_dir {
                                  std::path::PathBuf::from(dir).join("mods")
                              } else if let Some(launcher) = &self.launcher {
                                  launcher.config.minecraft_dir.join("instances").join(profile_name).join("mods")
                              } else {
                                  return;
                              }
                          } else {
                              return;
                          }
                      } else {
                          // Default global mods dir if no profile selected (or "default" fallback?)
                          // Or empty if no profile selected.
                          return;
                      };

                      if mods_dir.exists() {
                           if let Ok(mut entries) = std::fs::read_dir(&mods_dir) {
                                while let Some(Ok(entry)) = entries.next() {
                                    if let Some(name) = entry.file_name().to_str() {
                                        if name.ends_with(".jar") {
                                            // Create row with delete button
                                            let row = gtk::ListBoxRow::new();
                                            let box_container = gtk::Box::new(gtk::Orientation::Horizontal, 12);
                                            box_container.set_margin_all(12);

                                            // Mod Icon
                                            let icon_image = gtk::Image::builder()
                                                .icon_name("application-x-addon-symbolic") // Default fallback
                                                .pixel_size(32)
                                                .build();

                                            // Try to extract icon from jar
                                            let jar_path = mods_dir.join(name);
                                            let cache_dir = std::env::temp_dir().join("rcraft").join("cache").join("installed_icons");
                                            std::fs::create_dir_all(&cache_dir).unwrap_or_default();

                                            // Unique cache key: mod_filename.png (simple but effective for now)
                                            let icon_path = cache_dir.join(format!("{}.png", name));


                                            if icon_path.exists() {
                                                icon_image.set_from_file(Some(icon_path.to_str().unwrap_or_default()));
                                            } else {
                                                // Extract
                                                let mut icon_path_inside_jar: Option<String> = None;

                                                if let Ok(file) = File::open(&jar_path) {
                                                    if let Ok(mut archive) = ZipArchive::new(file) {
                                                        // 1. Read fabric.mod.json to find icon path
                                                        if let Ok(mut json_file) = archive.by_name("fabric.mod.json") {
                                                            let mut json_str = String::new();
                                                            if json_file.read_to_string(&mut json_str).is_ok() {
                                                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&json_str) {
                                                                    if let Some(icon_val) = json.get("icon") {
                                                                        if let Some(s) = icon_val.as_str() {
                                                                             icon_path_inside_jar = Some(s.to_string());
                                                                        } else if let Some(obj) = icon_val.as_object() {
                                                                             // Sometimes icon is a map { "16x16": "assets/..." }
                                                                             // Pick largest or first?
                                                                             if let Some(s) = obj.values().last().and_then(|v| v.as_str()) {
                                                                                 icon_path_inside_jar = Some(s.to_string());
                                                                             }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }

                                                        // 2. If found, extract icon file (borrow archive again)
                                                        if let Some(mut ip) = icon_path_inside_jar {
                                                            // Remove leading ./ if present
                                                            if ip.starts_with("./") { ip = ip[2..].to_string(); }

                                                            if let Ok(mut icon_zip_file) = archive.by_name(&ip) {
                                                                let mut buffer = Vec::new();
                                                                if icon_zip_file.read_to_end(&mut buffer).is_ok() {
                                                                      // Convert/Save using image crate
                                                                      match image::load_from_memory(&buffer) {
                                                                          Ok(img) => {
                                                                              if img.save_with_format(&icon_path, image::ImageFormat::Png).is_ok() {
                                                                                  icon_image.set_from_file(Some(icon_path.to_str().unwrap_or_default()));
                                                                              }
                                                                          },
                                                                          Err(_) => {
                                                                              // Ignore
                                                                          }
                                                                      }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }

                                            let label = gtk::Label::builder()
                                                .label(name)
                                                .halign(gtk::Align::Start)
                                                .hexpand(true)
                                                .build();

                                            let delete_button = gtk::Button::builder()
                                                .icon_name("user-trash-symbolic")
                                                .css_classes(vec!["destructive-action"])
                                                .tooltip_text("Uninstall")
                                                .build();

                                            let sender_clone = sender.clone();
                                            let filename = name.to_string();
                                            delete_button.connect_clicked(move |_| {
                                                 sender_clone.input(AppMsg::UninstallMod(filename.clone()));
                                             });

                                            box_container.append(&icon_image);
                                            box_container.append(&label);
                                            box_container.append(&delete_button);
                                            row.set_child(Some(&box_container));
                                            list.append(&row);
                                        }
                                    }
                                }
                           }
                      }
                 }
            }
            AppMsg::SelectModProfile(profile_name) => {
                self.selected_mod_profile = Some(profile_name);
                // Refresh list for the new profile
                sender.input(AppMsg::RefreshInstalledMods);
            }
            AppMsg::SearchMods(query) => {
                self.is_searching = true;

                let modrinth = self.modrinth.clone();
                let sender_clone = sender.clone();
                // Removed Toast: Searching...

                // Get profile version for filtering
                let (version_filter, loader_filter) = if let Some(profile_name) = &self.selected_mod_profile {
                    if let Some(profile) = self.profiles.get(profile_name) {
                        (Some(profile.version.clone()), Some("fabric".to_string()))
                    } else {
                        (None, None)
                    }
                } else {
                    (None, None)
                };

                std::thread::spawn(move || {
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(async {
                         let v_ref = version_filter.as_deref();
                         let l_ref = loader_filter.as_deref();
                         match modrinth.search_mods(&query, 20, v_ref, l_ref).await {
                             Ok(results) => sender_clone.input(AppMsg::ModsSearched(Ok(results))),
                             Err(e) => sender_clone.input(AppMsg::ModsSearched(Err(e.to_string()))),
                         }
                    });
                });
            }
            AppMsg::ModsSearched(result) => {
                self.is_searching = false;

                match result {
                    Ok(results) => {
                        self.mod_search_results = results.clone();

                        // Populate list directly since we have the reference
                        if let Some(list) = &self.mod_browse_list {
                             // Clear existing
                             while let Some(child) = list.first_child() {
                                 list.remove(&child);
                             }

                             // Add new items
                             for mod_data in results {
                                 let row = create_mod_search_result_row(&mod_data, &sender);
                                 list.append(&row);

                                 if let Some(url) = &mod_data.icon_url {
                                      sender.input(AppMsg::DownloadModIcon(mod_data.project_id.clone(), url.clone()));
                                 }
                             }
                        }
                    }
                    Err(e) => {
                        println!("Search failed: {}", e);
                        println!("Search error: {}", e);
                    }
                }
            }
            AppMsg::InstallMod(project_id) => {
                let modrinth = self.modrinth.clone();
                let sender_clone = sender.clone();
                let _project_id_clone = project_id.clone();

                // Determine mods directory
                let mods_dir = if let Some(profile_name) = &self.selected_mod_profile {
                    if let Some(profile) = self.profiles.get(profile_name) {
                        if let Some(dir) = &profile.game_dir {
                            std::path::PathBuf::from(dir).join("mods")
                        } else if let Some(launcher) = &self.launcher {
                            launcher.config.minecraft_dir.join("instances").join(profile_name).join("mods")
                        } else {
                            sender.input(AppMsg::Error("Could not determine mods directory.".to_string()));
                            return;
                        }
                    } else {
                         sender.input(AppMsg::Error("Profile not found.".to_string()));
                         return;
                    }
                } else {
                    sender.input(AppMsg::Error("No profile selected for installation.".to_string()));
                    return;
                };

                // Create mods dir if needed
                if !mods_dir.exists() {
                     if let Err(e) = std::fs::create_dir_all(&mods_dir) {
                         sender.input(AppMsg::Error(format!("Failed to create mods directory: {}", e)));
                         return;
                     }
                }

                // Set button to loading state
                if let Some(list) = &self.mod_browse_list {
                     // Find button with id "btn_<project_id>"
                     // ... (omitted iteration for brevity, assuming helper or inline)
                     let mut sibling = list.first_child();
                     while let Some(child) = sibling {
                           if let Some(row) = child.downcast_ref::<gtk::ListBoxRow>() {
                                if let Some(box_widget) = row.child() {
                                     if let Some(bx) = box_widget.downcast_ref::<gtk::Box>() {
                                          let mut box_child = bx.first_child();
                                          while let Some(b_child) = box_child {
                                               if let Some(button) = b_child.downcast_ref::<gtk::Button>() {
                                                    if button.widget_name() == format!("btn_{}", project_id) {
                                                         button.set_icon_name("process-working-symbolic");
                                                         button.set_sensitive(false);
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

                 // Get profile version for filtering
                let (version_filter, loader_filter) = if let Some(profile_name) = &self.selected_mod_profile {
                    if let Some(profile) = self.profiles.get(profile_name) {
                        (Some(profile.version.clone()), Some("fabric".to_string()))
                    } else {
                        (None, None)
                    }
                } else {
                    (None, None)
                };

                 std::thread::spawn(move || {
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(async {
                         // 1. Get versions with filtering
                         let v_ref = version_filter.as_deref();
                         let l_ref = loader_filter.as_deref();
                         match modrinth.get_versions(&project_id, l_ref, v_ref).await {
                             Ok(versions) => {
                                 if let Some(version) = versions.first() {
                                     if let Some(file) = version.files.iter().find(|f| f.primary).or(version.files.first()) {
                                          let path = mods_dir.join(&file.filename);
                                          // Removed Toast: Downloading...
                                          match modrinth.download_mod(&file.url, &path).await {
                                              Ok(_) => {
                                                  sender_clone.input(AppMsg::ShowToast("Mod installed!".to_string()));
                                                  sender_clone.input(AppMsg::RefreshInstalledMods);

                                                  // Register FIRST so map is updated before UI refresh
                                                  sender_clone.input(AppMsg::RegisterInstalledMod(project_id.clone(), file.filename.clone()));
                                                  sender_clone.input(AppMsg::ModInstallFinished(project_id.clone(), true));
                                              },
                                              Err(e) => {
                                                  sender_clone.input(AppMsg::Error(format!("Download failed: {}", e)));
                                                  sender_clone.input(AppMsg::ModInstallFinished(project_id.clone(), false));
                                              }
                                          }
                                     } else {
                                          sender_clone.input(AppMsg::Error("No files found for this version".to_string()));
                                          sender_clone.input(AppMsg::ModInstallFinished(project_id.clone(), false));
                                     }
                                 } else {
                                      sender_clone.input(AppMsg::Error("No versions found for this mod".to_string()));
                                      sender_clone.input(AppMsg::ModInstallFinished(project_id.clone(), false));
                                 }
                             }
                             Err(e) => {
                                 sender_clone.input(AppMsg::Error(format!("Failed to get mod versions: {}", e)));
                                 sender_clone.input(AppMsg::ModInstallFinished(project_id.clone(), false));
                             }
                         }
                    });
                });
            }

            AppMsg::DownloadModIcon(project_id, url) => {
                self.icon_download_queue.push_back((project_id, url));
                if !self.is_downloading_icon {
                    sender.input(AppMsg::ProcessIconQueue);
                }
            }

            AppMsg::ProcessIconQueue => {
                if self.is_downloading_icon {
                    return;
                }

                if let Some((project_id, url)) = self.icon_download_queue.pop_front() {
                    self.is_downloading_icon = true;
                    // let modrinth = self.modrinth.clone(); // Modrinth client has download helper, but we need bytes now.
                    // ModrinthClient::download_icon saves to file directly. We need to fetch bytes first.
                    // So we should probably use the reqwest client directly or add a helper to ModrinthClient.
                    // For simplicity, let's just use reqwest here or modify ModrinthClient?
                    // Accessing modrinth.client is not possible if it's private.
                    // Let's modify ModrinthClient to have `fetch_bytes` or just create a new client/request here?
                    // Creating a new client for every icon is bad.
                    // Let's add `get_icon_bytes` to ModrinthClient?
                    // Or since we are inside `ui.rs` and `ModrinthClient` is in `modrinth_client.rs`, we can't easily change it without editing that file.
                    // Let's edit `modrinth_client.rs` to expose a method to get bytes.

                    // Actually, let's just check `modrinth_client.rs`. It has `client` field. Is it pub?
                    // No.

                    // Okay, let's pause editing `ui.rs` and update `modrinth_client.rs` first to add `download_icon_bytes`.
                    // But wait, I'm already in a tool call.
                    // I'll assume I can add it next.
                    // For now, let's write the code assuming `modrinth.download_icon_bytes(&url)` exists.

                    let modrinth = self.modrinth.clone();
                    let sender_clone = sender.clone();
                    let project_id_clone = project_id.clone();

                    std::thread::spawn(move || {
                        let rt = tokio::runtime::Runtime::new().unwrap();
                        rt.block_on(async {
                            // Cache dir
                            let cache_dir = std::env::temp_dir().join("rcraft").join("cache").join("icons");
                            if let Err(e) = std::fs::create_dir_all(&cache_dir) {
                                println!("Failed to create icon cache dir: {}", e);
                            }

                            // Check PNG first
                            let png_path = cache_dir.join(format!("{}.png", project_id_clone));
                            // Check SVG second
                            let svg_path = cache_dir.join(format!("{}.svg", project_id_clone));

                            // Helper to validate and return path if valid
                            let validate_cache = |path: &std::path::PathBuf| -> bool {
                                if !path.exists() { return false; }
                                if let Ok(metadata) = std::fs::metadata(path) {
                                    if metadata.len() < 100 { // Arbitrary small size check
                                        let _ = std::fs::remove_file(path);
                                        return false;
                                    }
                                }
                                true
                            };

                            if validate_cache(&png_path) {
                                sender_clone.input(AppMsg::ModIconDownloaded(project_id_clone, png_path.to_string_lossy().to_string()));
                            } else if validate_cache(&svg_path) {
                                sender_clone.input(AppMsg::ModIconDownloaded(project_id_clone, svg_path.to_string_lossy().to_string()));
                            } else {
                                if !url.starts_with("http") {
                                      sender_clone.input(AppMsg::ModIconDownloaded(project_id_clone, "".to_string()));
                                     return;
                                }

                                match modrinth.download_icon_bytes(&url).await {
                                    Ok(bytes) => {
                                        // Try converting to PNG first using image crate
                                        match image::load_from_memory(&bytes) {
                                            Ok(img) => {
                                                if let Err(e) = img.save_with_format(&png_path, image::ImageFormat::Png) {
                                                    println!("Failed to save converted icon for '{}': {}", project_id_clone, e);
                                                    sender_clone.input(AppMsg::ModIconDownloaded(project_id_clone, "".to_string()));
                                                } else {
                                                    sender_clone.input(AppMsg::ModIconDownloaded(project_id_clone, png_path.to_string_lossy().to_string()));
                                                }
                                            },
                                            Err(_) => {
                                                // Failed to load as image -> check if it's SVG
                                                // Simple heuristic: check for <svg or <?xml ... <svg
                                                let s = String::from_utf8_lossy(&bytes);
                                                if s.contains("<svg") {
                                                     // Save as SVG
                                                     if std::fs::write(&svg_path, &bytes).is_ok() {
                                                          sender_clone.input(AppMsg::ModIconDownloaded(project_id_clone, svg_path.to_string_lossy().to_string()));
                                                     } else {
                                                          sender_clone.input(AppMsg::ModIconDownloaded(project_id_clone, "".to_string()));
                                                     }
                                                } else {
                                                     // Unknown format or corrupted
                                                     sender_clone.input(AppMsg::ModIconDownloaded(project_id_clone, "".to_string()));
                                                }
                                            }
                                        }
                                    },
                                    Err(_) => {
                                        sender_clone.input(AppMsg::ModIconDownloaded(project_id_clone, "".to_string()));
                                    }
                                }
                            }
                        });
                    });
                }
            }
            AppMsg::ModIconDownloaded(project_id, path) => {
                self.is_downloading_icon = false;
                sender.input(AppMsg::ProcessIconQueue); // Process next

                if path.is_empty() { return; }

                if let Some(list) = &self.mod_browse_list {
                      let mut sibling = list.first_child();
                      while let Some(child) = sibling {
                           // This is the ListBoxRow
                           if let Some(row) = child.downcast_ref::<gtk::ListBoxRow>() {
                                if let Some(box_widget) = row.child() {
                                     // Box
                                     if let Some(bx) = box_widget.downcast_ref::<gtk::Box>() {
                                          let mut box_child = bx.first_child();
                                          while let Some(b_child) = box_child {
                                               if let Some(image) = b_child.downcast_ref::<gtk::Image>() {
                                                    if image.widget_name() == project_id {
                                                         image.set_from_file(Some(&path));
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

            AppMsg::ModInstallFinished(project_id, success) => {
                 if success {
                     // We need the filename to store in installed_mods
                     // But InstallMod logic handles the download separately.
                     // The InstallMod logic we saw earlier finds the version and downloads it.
                     // It DID NOT pass the filename back in ModInstallFinished.
                     // I need to update InstallMod logic to find the filename and pass it to ModInstallFinished?
                     // Or just store it?
                     // In InstallMod handler (below), we can see it finds the file.
                     // I should modify InstallMod handler to Insert into installed_mods map.
                 }

                 // Reset button state
                 self.update_mod_button_state(&project_id);
            }
            AppMsg::ModUninstallFinished(project_id) => {
                self.installed_mods.remove(&project_id);
                self.update_mod_button_state(&project_id);
            }
            AppMsg::ModActionButtonClicked(project_id) => {
                if let Some(filename) = self.installed_mods.get(&project_id) {
                    sender.input(AppMsg::UninstallMod(filename.clone()));
                    // We also need to know the project_id to update UI after uninstall
                    // UninstallMod just takes filename.
                    // We can track "uninstalling_project_id" in state?
                    // Or change UninstallMod to take (filename, Option<project_id>).
                    // Or just look up who owns the filename? (Slow).
                    // Better: Change UninstallMod to take project_id if we have it?
                    // But installed mods list doesn't know project id.

                    // Let's carry project_id in UninstallMod?
                    // Or better: ModUninstallFinished is called by UninstallMod handler.
                    // But UninstallMod handler needs to know project_id to send ModUninstallFinished.
                    // So UninstallMod needs to take `(String, Option<String>)` -> (filename, project_id).
                } else {
                    sender.input(AppMsg::InstallMod(project_id));
                }
            }
            AppMsg::ShowToast(message) => {
                if let Some(overlay) = &self.toast_overlay {
                    overlay.add_toast(adw::Toast::new(&message));
                }
            }
            AppMsg::RegisterInstalledMod(project_id, filename) => {
                self.installed_mods.insert(project_id, filename);
            }
            AppMsg::UninstallMod(filename) => {
                // Determine mods directory (duplicate logic, should be helper but ok for now)
                let mods_dir = if let Some(profile_name) = &self.selected_mod_profile {
                    if let Some(profile) = self.profiles.get(profile_name) {
                        if let Some(dir) = &profile.game_dir {
                            std::path::PathBuf::from(dir).join("mods")
                        } else if let Some(launcher) = &self.launcher {
                            launcher.config.minecraft_dir.join("instances").join(profile_name).join("mods")
                        } else {
                            sender.input(AppMsg::Error("Could not determine mods directory.".to_string()));
                            return;
                        }
                    } else {
                         sender.input(AppMsg::Error("Profile not found.".to_string()));
                         return;
                    }
                } else {
                    sender.input(AppMsg::Error("No profile selected.".to_string()));
                    return;
                };

                let file_path = mods_dir.join(&filename);
                if file_path.exists() {
                     match std::fs::remove_file(&file_path) {
                         Ok(_) => {
                             // Removed Toast: Uninstalled...
                             sender.input(AppMsg::RefreshInstalledMods);

                             // Find if this was a tracked mod and update UI
                             let mut project_id_to_remove = None;
                             for (pid, fname) in &self.installed_mods {
                                 if fname == &filename {
                                     project_id_to_remove = Some(pid.clone());
                                     break;
                                 }
                             }

                             if let Some(pid) = project_id_to_remove {
                                 sender.input(AppMsg::ModUninstallFinished(pid));
                             }
                         }
                         Err(e) => sender.input(AppMsg::Error(format!("Failed to uninstall mod: {}", e))),
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
                widgets.mods_button.set_sensitive(true);
                widgets.logs_button.set_sensitive(true);

                // Clear previous suggested-action classes first
                widgets.home_button.remove_css_class("suggested-action");
                widgets.create_sidebar_button.remove_css_class("suggested-action");
                widgets.mods_button.remove_css_class("suggested-action");
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
                    Section::Mods => {
                         widgets.mods_button.add_css_class("suggested-action");
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
                    Section::Mods => {
                        widgets.content_stack.set_visible_child_name("mods");
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
                widgets.mods_button.set_sensitive(false);
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
                widgets.mods_button.set_sensitive(false);
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
                widgets.mods_button.set_sensitive(false);
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
        widgets.logs_button.set_visible(self.settings.nerd_mode);
        widgets.nerd_mode_switch.set_active(self.settings.nerd_mode);

        widgets.mods_button.set_visible(!self.settings.hide_mods_button);
        widgets.hide_mods_switch.set_active(self.settings.hide_mods_button);

        let theme_index = match self.settings.theme {
            Theme::System => 0,
            Theme::Light => 1,
            Theme::Dark => 2,
        };
        if widgets.theme_combo.selected() != theme_index {
            widgets.theme_combo.set_selected(theme_index);
        }

        // Update sidebar constraints to force resize
        if self.sidebar_collapsed {
            widgets.navigation_split_view.set_min_sidebar_width(60.0);
            widgets.navigation_split_view.set_max_sidebar_width(60.0);

            // Center icons when collapsed
            widgets.home_box.set_halign(gtk::Align::Center);
            widgets.create_box.set_halign(gtk::Align::Center);
            widgets.settings_box.set_halign(gtk::Align::Center);
            widgets.mods_box.set_halign(gtk::Align::Center);
            widgets.logs_box.set_halign(gtk::Align::Center);
        } else {
            // Restore standard width
            widgets.navigation_split_view.set_min_sidebar_width(180.0);
            widgets.navigation_split_view.set_max_sidebar_width(250.0);

            // Left align when expanded
            // Left align when expanded
            widgets.home_box.set_halign(gtk::Align::Start);
            widgets.create_box.set_halign(gtk::Align::Start);
            widgets.settings_box.set_halign(gtk::Align::Start);
            widgets.mods_box.set_halign(gtk::Align::Start);
            widgets.logs_box.set_halign(gtk::Align::Start);
        }

        if let Some(model) = &self.mod_profile_list_model {
             widgets.mod_profile_dropdown.set_model(Some(model));
        }

        // Update sidebar visibility (no animation)
        widgets.home_label.set_visible(!self.sidebar_collapsed);
        widgets.create_label.set_visible(!self.sidebar_collapsed);
        widgets.create_label.set_visible(!self.sidebar_collapsed);
        widgets.settings_label.set_visible(!self.sidebar_collapsed);
        // Only show mods label if button is visible (which is handled by set_visible above but label is separate in box)
        // Wait, the sidebar button contains the label.
        // `create_nav_button` returns (button, label, box).
        // The `mods_button` visibility controls the whole button (icon+label).
        // BUT `widgets.mods_label` visibility controls the LABEL specifically (for sidebar collapse).
        // So we need: if sidebar collapsed -> label hidden.
        // If mods button hidden -> whole button hidden (so label state doesn't matter).
        // But if mods button visible -> check sidebar state.
        if self.is_searching {
             widgets.mod_search_stack.set_visible_child_name("spinner");
        } else {
             widgets.mod_search_stack.set_visible_child_name("button");
        }

        widgets.mods_label.set_visible(!self.sidebar_collapsed);
        widgets.logs_label.set_visible(!self.sidebar_collapsed);
    }
}

// ============================================================================
// UI Helper Functions
// ============================================================================

fn create_sidebar(sender: &ComponentSender<AppModel>) -> (NavigationPage, gtk::Button, gtk::Button, gtk::Button, gtk::Button, gtk::Button, gtk::Label, gtk::Label, gtk::Label, gtk::Label, gtk::Label, gtk::Box, gtk::Box, gtk::Box, gtk::Box, gtk::Box) {
    let sidebar_content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
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

    // Helper to create a styled button with icon and label
    let create_nav_button = |label_text: &str, icon_name: &str| -> (gtk::Button, gtk::Label, gtk::Box) {
        let button = gtk::Button::builder()
            .halign(gtk::Align::Fill)
            .hexpand(true)
            .height_request(40)
            .margin_top(6)
            .margin_bottom(6)
            .build();

        let box_container = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .halign(gtk::Align::Start) // Default left align
            .build();

        let icon = gtk::Image::builder()
            .icon_name(icon_name)
            .build();

        let label = gtk::Label::builder()
            .label(label_text)
            .visible(true)
            .build();

        box_container.append(&icon);
        box_container.append(&label);

        button.set_child(Some(&box_container));
        (button, label, box_container)
    };

    // Navigation buttons
    let (home_button, home_label, home_box) = create_nav_button("Home", "user-home-symbolic");
    let (create_button, create_label, create_box) = create_nav_button("New Profile", "list-add-symbolic");
    let (mods_button, mods_label, mods_box) = create_nav_button("Mods", "application-x-addon-symbolic");
    let (settings_button, settings_label, settings_box) = create_nav_button("Settings", "emblem-system-symbolic");
    let (logs_button, logs_label, logs_box) = create_nav_button("Logs", "utilities-terminal-symbolic");

    // Logs button (hidden by default)
    logs_button.set_visible(false);

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
    mods_button.connect_clicked(move |_| {
        sender_clone.input(AppMsg::NavigateToSection(Section::Mods));
    });

    let sender_clone = sender.clone();
    logs_button.connect_clicked(move |_| {
        sender_clone.input(AppMsg::NavigateToSection(Section::Logs));
    });

    // Add buttons to sidebar (Home > Create > Settings)
    sidebar_content.append(&home_button);
    sidebar_content.append(&create_button);
    sidebar_content.append(&mods_button);
    sidebar_content.append(&settings_button);
    sidebar_content.append(&logs_button);

    // Add spacer to push content to top
    let spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    spacer.set_vexpand(true);
    sidebar_content.append(&spacer);

    // Version Label
    let version_label = gtk::Label::builder()
        .label("v0.9")
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

    (sidebar_page, home_button, create_button, mods_button, settings_button, logs_button, home_label, create_label, mods_label, settings_label, logs_label, home_box, create_box, mods_box, settings_box, logs_box)
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

fn create_settings_page(sender: &ComponentSender<AppModel>, nerd_mode_switch: &adw::SwitchRow, hide_mods_switch: &adw::SwitchRow) -> (gtk::Box, adw::ComboRow) {
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

    // Hide Mods switch configuration
    let sender_clone = sender.clone();
    hide_mods_switch.connect_active_notify(move |switch| {
        sender_clone.input(AppMsg::ToggleHideMods(switch.is_active()));
    });
    hide_mods_switch.set_hexpand(true);
    hide_mods_switch.set_halign(gtk::Align::Fill);

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
    settings_list.append(hide_mods_switch);

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
        .subtitle("v0.9")
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
