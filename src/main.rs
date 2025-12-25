//  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //
// RCraft v0.6                                                                      //
//                                                                                  //
// This program is free software under GPL-3.0: key freedoms and restrictions:      //
// - Free use, study, and modification for any purpose.                             //
// - Redistribution only under GPL-3.0 (copyleft: derivatives must be GPL-3).       //
// - Preserve all copyright attributions (including this one).                      //
// - Do not add proprietary clauses or remove notices.                              //
//                                                                                  //
// For the full text, see LICENSE in this repository.                               //
// Repository: https://github.com/vdkvdev/RCraft                                    //
//  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //

// ============================================================================
// Module Declarations
// ============================================================================

mod models;
mod config;
mod utils;
mod launcher;
mod ui;
mod settings;

// ============================================================================
// Imports
// ============================================================================


use adw::Application;
use gtk4::glib;
use relm4::RelmApp;

use ui::AppModel;

// ============================================================================
// Main Entry Point
// ============================================================================

fn main() {
    // Create a new libadwaita application for GNOME integration
    let app = Application::builder()
        .application_id("dev.vdkv.RCraft")
        .build();
    
    // Set application name for GNOME integration
    glib::set_application_name("RCraft");
    
    // Create Relm4 app wrapper
    let relm_app = RelmApp::from_app(app);
    
    // Run the application with our component
    relm_app.run::<AppModel>(())
}