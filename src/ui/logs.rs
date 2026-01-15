use relm4::gtk;
use relm4::ComponentSender;
use gtk::prelude::*;

use crate::ui::model::AppModel;

pub fn create_logs_page(_sender: &ComponentSender<AppModel>, logs_buffer: &gtk::TextBuffer) -> (gtk::Box, gtk::TextView) {
    let container = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(24)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();

    // Title label
    let title_label = gtk::Label::builder()
        .label("Logs")
        .halign(gtk::Align::Start)
        .css_classes(vec!["title-1".to_string()])
        .build();

    container.append(&title_label);

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
        // Margins removed here as they are now handled by the container
        .build();

    // Add CSS class "boxed-list"-like visual if needed, or just plain frame.
    // For now, let's keep it simple as a text view in a scroller.
    // Maybe wrap scroller in a frame for better visuals?
    // Let's stick to the request: padding and title.

    scrolled_window.set_child(Some(&text_view));
    container.append(&scrolled_window);

    (container, text_view)
}
