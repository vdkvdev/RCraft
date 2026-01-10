use relm4::gtk;
use relm4::ComponentSender;

use crate::ui::model::AppModel;

pub fn create_logs_page(_sender: &ComponentSender<AppModel>, logs_buffer: &gtk::TextBuffer) -> (gtk::ScrolledWindow, gtk::TextView) {
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
