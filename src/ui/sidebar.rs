use relm4::gtk;
use relm4::ComponentSender;
use gtk::prelude::*;
use crate::ui::model::AppModel;
use crate::ui::msg::AppMsg;
use crate::models::{Section};

use adw::NavigationPage;

pub fn create_sidebar(sender: &ComponentSender<AppModel>) -> (NavigationPage, gtk::Button, gtk::Button, gtk::Button, gtk::Button, gtk::Button, gtk::Label, gtk::Label, gtk::Label, gtk::Label, gtk::Label, gtk::Box, gtk::Box, gtk::Box, gtk::Box, gtk::Box) {
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
    sidebar_content.append(&logs_button);
    sidebar_content.append(&settings_button);

    // Add spacer to push content to top
    let spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    spacer.set_vexpand(true);
    sidebar_content.append(&spacer);

    // Version Label
    let version_label = gtk::Label::builder()
        .label("v1.1")
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
