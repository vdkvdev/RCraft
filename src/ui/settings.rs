use relm4::gtk;
use relm4::ComponentSender;
use gtk::prelude::*;
use adw::prelude::*;
use crate::ui::model::AppModel;
use crate::ui::msg::AppMsg;
use crate::models::Theme;

pub fn create_settings_page(sender: &ComponentSender<AppModel>, hide_logs_switch: &adw::SwitchRow, hide_mods_switch: &adw::SwitchRow) -> (gtk::ScrolledWindow, adw::ComboRow) {
    let scrolled_window = gtk::ScrolledWindow::builder()
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

    // Hide Logs switch configuration
    hide_logs_switch.set_title("Hide Console");
    hide_logs_switch.set_subtitle("Hide the logs tab");
    hide_logs_switch.set_hexpand(true);
    hide_logs_switch.set_halign(gtk::Align::Fill);

    let sender_clone = sender.clone();
    hide_logs_switch.connect_active_notify(move |switch| {
        sender_clone.input(AppMsg::ToggleHideLogs(switch.is_active()));
    });

    // Hide Mods switch configuration
    let sender_clone = sender.clone();
    hide_mods_switch.connect_active_notify(move |switch| {
        sender_clone.input(AppMsg::ToggleHideMods(switch.is_active()));
    });
    hide_mods_switch.set_hexpand(true);
    hide_mods_switch.set_halign(gtk::Align::Fill);
    hide_mods_switch.set_title("Hide Mods");
    hide_mods_switch.set_subtitle("Hide the Mods button in the sidebar");


    // Theme selection
    let theme_row = adw::ComboRow::builder()
        .title("Theme")
        .hexpand(true)
        .halign(gtk::Align::Fill)
        .build();

    let theme_model = gtk::StringList::new(&["System", "Light", "Dark", "Transparent"]);
    theme_row.set_model(Some(&theme_model));

    let sender_clone = sender.clone();
    theme_row.connect_notify(Some("selected"), move |combo, _| {
        let theme = match combo.selected() {
            3 => Theme::Transparent,
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
    settings_list.append(hide_logs_switch);
    settings_list.append(hide_mods_switch);

    // Add list box to main content
    content_container.append(&settings_list);

    // About Section
    let about_list = gtk::ListBox::new();
    about_list.add_css_class("boxed-list");
    about_list.set_selection_mode(gtk::SelectionMode::None);
    about_list.set_hexpand(true);
    about_list.set_halign(gtk::Align::Fill);

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
         let _ = open::that("https://github.com/vdkvdev/rcraft");
    });

    repo_row.add_suffix(&repo_button);
    repo_row.set_activatable(false); // only button is interactive

    about_list.append(&repo_row);

    content_container.append(&about_list);

    scrolled_window.set_child(Some(&content_container));
    (scrolled_window, theme_row)
}
