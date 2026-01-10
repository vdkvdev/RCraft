use relm4::gtk;
use gtk::prelude::*;

use adw::StatusPage;

pub fn create_loading_widgets() -> (StatusPage, gtk::Spinner, gtk::ProgressBar, gtk::Label) {
    let status_page = adw::StatusPage::builder()
        .title("Loading RCraft")
        .description("Please wait while the launcher initializes...")
        .build();

    let spinner = gtk::Spinner::new();
    spinner.start();
    spinner.set_size_request(48, 48);

    let progress_bar = gtk::ProgressBar::new();
    progress_bar.set_show_text(true);

    progress_bar.set_margin_top(12);
    progress_bar.set_size_request(300, -1);

    let label = gtk::Label::new(Some("Initializing..."));

    // Add spinner to status page
    status_page.set_child(Some(&spinner));

    (status_page, spinner, progress_bar, label)
}
