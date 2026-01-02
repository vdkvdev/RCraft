mod models;
mod config;
mod utils;
mod launcher;
mod ui;
mod settings;
mod modrinth_client;
mod mods_ui;

use adw::Application;
use gtk4::glib;
use relm4::RelmApp;

use ui::AppModel;

fn main() {
    let app = Application::builder()
        .application_id("dev.vdkv.RCraft")
        .build();

    glib::set_application_name("RCraft");
    let relm_app = RelmApp::from_app(app);
    relm_app.run::<AppModel>(())
}
