
use adw::{self, NavigationSplitView, NavigationPage};
use relm4::gtk;

#[allow(dead_code)]
pub struct AppWidgets {
    pub window: adw::ApplicationWindow,
    pub header_bar: adw::HeaderBar,
    pub navigation_split_view: NavigationSplitView,
    pub navigation_page: NavigationPage,
    pub content_stack: gtk::Stack,

    // Pages
    pub home_page: gtk::Box,
    pub create_page: gtk::Box,
    pub settings_page: gtk::ScrolledWindow,
    pub mods_page: gtk::Box,
    pub logs_page: gtk::ScrolledWindow,
    pub loading_page: adw::StatusPage,

    // Home page widgets
    pub profile_list: gtk::ListBox,
    pub username_entry: adw::EntryRow,
    pub version_combo: adw::ComboRow,
    pub ram_scale: adw::SpinRow,
    pub fabric_switch: adw::SwitchRow,
    pub nerd_mode_switch: adw::SwitchRow,
    pub hide_mods_switch: adw::SwitchRow,

    // Buttons
    pub launch_button: gtk::Button,
    pub create_button: gtk::Button,
    pub delete_button: gtk::Button,
    pub save_button: gtk::Button,
    pub cancel_button: gtk::Button,

    // Sidebar buttons
    pub home_button: gtk::Button,
    pub create_sidebar_button: gtk::Button,
    pub mods_button: gtk::Button,
    pub settings_button: gtk::Button,
    pub logs_button: gtk::Button,

    // Mods widgets
    pub mod_profile_dropdown: gtk::DropDown,
    pub mod_search_stack: gtk::Stack,

    // Sidebar button labels (for visibility)
    pub home_label: gtk::Label,
    pub create_label: gtk::Label,
    pub mods_label: gtk::Label,
    pub settings_label: gtk::Label,
    pub logs_label: gtk::Label,

    // Sidebar button boxes (for alignment)
    pub home_box: gtk::Box,
    pub create_box: gtk::Box,
    pub mods_box: gtk::Box,
    pub settings_box: gtk::Box,
    pub logs_box: gtk::Box,

    // Sidebar Toggle
    pub sidebar_toggle_button: gtk::Button,

    // Settings widgets
    pub theme_combo: adw::ComboRow,

    // Status/error labels
    pub status_label: gtk::Label,
    pub error_label: gtk::Label,

    // Loading widgets
    pub loading_spinner: gtk::Spinner,
    pub loading_progress: gtk::ProgressBar,
    pub loading_label: gtk::Label,

    // Toast Overlay
    pub toast_overlay: adw::ToastOverlay,
    
    // Java Confirmation Dialog
    pub java_dialog: adw::MessageDialog,

    // Logs view
    pub logs_view: gtk::TextView,
}
