use relm4::gtk;
use relm4::{ComponentSender, RelmWidgetExt};
use gtk::prelude::*;
use crate::ui::model::AppModel;
use crate::ui::msg::AppMsg;
use crate::models::ModSearchResult;

pub fn create_mods_page(sender: &ComponentSender<AppModel>) -> (gtk::Box, gtk::SearchEntry, gtk::Button, gtk::Stack, gtk::ListBox, gtk::ListBox, gtk::DropDown) {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let stack = gtk::Stack::new();
    stack.set_transition_type(gtk::StackTransitionType::SlideLeftRight);

    let top_bar = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    top_bar.set_margin_all(12);

    let profile_model = gtk::StringList::new(&[]);
    let profile_dropdown = gtk::DropDown::builder()
        .model(&profile_model)
        .hexpand(true)
        .build();
    profile_dropdown.set_hexpand(true);

    let sender_clone = sender.clone();
    profile_dropdown.connect_selected_item_notify(move |dropdown| {
        if let Some(item) = dropdown.selected_item() {
            if let Some(string_obj) = item.downcast_ref::<gtk::StringObject>() {
                let full_string = string_obj.string().to_string();
                if let Some((name, version)) = full_string.rsplit_once(" - ") {
                     let key = format!("{}_{}_fabric", name, version);
                     sender_clone.input(AppMsg::SelectModProfile(key));
                }
            }
        }
    });

    let installed_btn_content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    installed_btn_content.append(&gtk::Image::from_icon_name("folder-download-symbolic"));
    installed_btn_content.append(&gtk::Label::new(Some("Installed")));

    let installed_button = gtk::Button::builder().child(&installed_btn_content).build();
    installed_button.add_css_class("suggested-action");

    let browse_btn_content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    browse_btn_content.append(&gtk::Image::from_icon_name("system-search-symbolic"));
    browse_btn_content.append(&gtk::Label::new(Some("Browse")));

    let browse_button = gtk::Button::builder().child(&browse_btn_content).build();

    let stack_clone = stack.clone();
    let _installed_btn_clone = installed_button.clone();
    let browse_btn_clone = browse_button.clone();

    installed_button.connect_clicked(move |btn| {
        stack_clone.set_visible_child_name("installed");
        btn.add_css_class("suggested-action");
        browse_btn_clone.remove_css_class("suggested-action");
    });

    let stack_clone = stack.clone();
    let installed_btn_clone = installed_button.clone();
    let _browse_btn_clone = browse_button.clone();

    browse_button.connect_clicked(move |btn| {
        stack_clone.set_visible_child_name("browse");
        btn.add_css_class("suggested-action");
        installed_btn_clone.remove_css_class("suggested-action");
    });

    top_bar.append(&profile_dropdown);
    top_bar.append(&installed_button);
    top_bar.append(&browse_button);

    let installed_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
    installed_box.set_margin_all(12);

    let installed_list = gtk::ListBox::new();
    installed_list.add_css_class("boxed-list");

    let installed_scroll = gtk::ScrolledWindow::new();
    installed_scroll.set_vexpand(true);
    installed_scroll.set_child(Some(&installed_list));

    let installed_toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let refresh_button = gtk::Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text("Refresh List")
        .build();

    let sender_clone = sender.clone();
    refresh_button.connect_clicked(move |_| {
        sender_clone.input(AppMsg::RefreshInstalledMods);
    });
    installed_toolbar.append(&gtk::Label::new(Some("My Mods")));
    installed_toolbar.append(&gtk::Box::new(gtk::Orientation::Horizontal, 0)); // Spacer?

    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    installed_toolbar.append(&spacer);
    installed_toolbar.append(&refresh_button);

    installed_box.append(&installed_toolbar);
    installed_box.append(&installed_scroll);

    stack.add_named(&installed_box, Some("installed"));

    let browse_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
    browse_box.set_margin_all(12);

    let search_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);

    let search_bar = gtk::SearchEntry::new();
    search_bar.set_placeholder_text(Some("Search Modrinth..."));
    search_bar.set_hexpand(true);

    let search_spinner = gtk::Spinner::new();

    let search_button = gtk::Button::builder()
        .icon_name("system-search-symbolic")
        .tooltip_text("Search")
        .build();

    let _search_spinner = gtk::Spinner::builder()
        .spinning(true)
        .build();

    let search_stack = gtk::Stack::new();
    search_stack.add_named(&search_button, Some("button"));
    search_stack.add_named(&search_spinner, Some("spinner"));
    search_stack.set_visible_child_name("button");
    search_stack.set_transition_type(gtk::StackTransitionType::Crossfade);

    search_box.append(&search_bar);
    search_box.append(&search_stack);

    let browse_list = gtk::ListBox::new();
    browse_list.add_css_class("boxed-list");
    browse_list.set_selection_mode(gtk::SelectionMode::None);

    let browse_scroll = gtk::ScrolledWindow::new();
    browse_scroll.set_vexpand(true);
    browse_scroll.set_child(Some(&browse_list));

    browse_box.append(&search_box);
    browse_box.append(&browse_scroll);

    stack.add_named(&browse_box, Some("browse"));

    container.append(&top_bar);
    container.append(&stack);

    stack.set_visible_child_name("installed");

    (container, search_bar, search_button, search_stack, installed_list, browse_list, profile_dropdown)
}

pub fn create_mod_search_result_row(mod_data: &ModSearchResult, sender: &ComponentSender<AppModel>) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    let box_container = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    box_container.set_margin_all(12);

    let icon = gtk::Image::from_icon_name("extension");
    icon.set_pixel_size(48);
    icon.set_widget_name(&mod_data.project_id);

    let info_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let title = gtk::Label::builder()
        .label(&mod_data.title)
        .halign(gtk::Align::Start)
        .css_classes(vec!["title-4"])
        .build();

    let desc_text = mod_data.description.clone().unwrap_or_default();
    let description = gtk::Label::builder()
        .label(&desc_text)
        .halign(gtk::Align::Start)
        .wrap(true)
        .max_width_chars(60)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .lines(2)
        .build();

    info_box.append(&title);
    info_box.append(&description);

    let download_button = gtk::Button::builder()
        .icon_name("folder-download-symbolic")
        .tooltip_text("Install")
        .valign(gtk::Align::Center)
        .name(&format!("btn_{}", mod_data.project_id))
        .build();

    let project_id = mod_data.project_id.clone();
    let sender_clone = sender.clone();
    download_button.connect_clicked(move |_| {
        sender_clone.input(AppMsg::ModActionButtonClicked(project_id.clone()));
    });

    let view_button = gtk::Button::builder()
        .icon_name("web-browser-symbolic")
        .tooltip_text("View on Modrinth")
        .valign(gtk::Align::Center)
        .build();

    let project_id_clone_2 = mod_data.project_id.clone();
    let sender_clone_2 = sender.clone();
    view_button.connect_clicked(move |_| {
        sender_clone_2.input(AppMsg::OpenModrinthPage(project_id_clone_2.clone()));
    });

    box_container.append(&icon);
    box_container.append(&info_box);

    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    box_container.append(&spacer);
    box_container.append(&view_button);
    box_container.append(&download_button);

    row.set_child(Some(&box_container));
    row
}
