// Xibo player Rust implementation, (c) 2022 Georg Brandl.
// Licensed under the GNU AGPL, version 3 or later.

//! The GUI window.

use std::convert::TryFrom;
use std::io::Cursor;
use anyhow::{anyhow, Result};
use cairo::{ImageSurface, Surface};
use crossbeam_channel::Sender;
use glib::{clone, prelude::*};
use gdk_pixbuf::Pixbuf;
use gtk::{prelude::*, Inhibit, Window, WindowType};
use webkit2gtk::{WebContext, WebView, UserContentManager, SnapshotRegion, SnapshotOptions};
use webkit2gtk::traits::{UserContentManagerExt, SettingsExt, WebViewExt, WebInspectorExt};
use crate::collect::Update;
use crate::config::PlayerSettings;

const LOGO_PNG: &[u8] = include_bytes!("../assets/logo.png");


pub fn run(settings: PlayerSettings, inspect: bool,
           updates: glib::Receiver<Update>, snaps: Sender<Vec<u8>>) -> Result<()> {
    gtk::init().expect("failed to init gtk");
    let base_uri = format!("http://localhost:{}/", settings.embedded_server_port);

    let logo = Pixbuf::from_read(Cursor::new(LOGO_PNG))?;

    let window = Window::new(WindowType::Toplevel);
    let context = WebContext::default().unwrap();
    let manager = UserContentManager::new();
    manager.connect("script-message-received::xibo", false, |_args| {
        // let arg = args[1].get::<webkit2gtk::JavascriptResult>().unwrap();
        // let ctx = arg.global_context().unwrap();
        // arg.value(&ctx)
        None
    });
    manager.register_script_message_handler("xibo");
    let webview = WebView::builder()
        .web_context(&context)
        .user_content_manager(&manager)
        .build();

    if inspect {
        let ws = WebViewExt::settings(&webview).unwrap();
        ws.set_enable_developer_extras(true);
        let inspector = webview.inspector().unwrap();
        inspector.show();
    }

    webview.load_uri(&format!("{}splash.html", base_uri));
    window.add(&webview);
    window.set_decorated(false);
    window.set_title(&settings.display_name);
    window.set_icon(Some(&logo));
    window.fullscreen();
    window.show_all();

    if let Some(win) = window.window() {
        win.set_cursor(gdk::Cursor::for_display(
            &gdk::Display::default().unwrap(),
            gdk::CursorType::BlankCursor).as_ref());
    }

    window.connect_delete_event(|_, _| {
        gtk::main_quit();
        Inhibit(false)
    });

    updates.attach(None, clone!(
        @weak webview, @weak window => @default-return Continue(true),
        move |update| {
            match update {
                Update::Screenshot => {
                    let snaps = snaps.clone();
                    webview.snapshot(
                        SnapshotRegion::Visible,
                        SnapshotOptions::NONE,
                        None::<&gio::Cancellable>,
                        move |result| match convert_shot(result) {
                            Ok(data) => snaps.send(data).unwrap(),
                            Err(e) => log::warn!("could not create snapshot: {:#}", e),
                        });
                }
                Update::Settings(settings) => {
                    window.set_title(&settings.display_name);
                    apply_size(&window, settings);
                }
                Update::Layouts(layouts) => if let Some(id) = layouts.first() {
                    log::info!("showing layout: {}", id);
                    // TODO: adapt webview scale to actual vs. designed size
                    webview.load_uri(&format!("{}layout?{}", base_uri, id));
                }
            }
            Continue(true)
        }
    ));

    gtk::main();

    Ok(())
}

fn apply_size(window: &Window, settings: PlayerSettings) {
    let PlayerSettings { size_x, size_y, position_x, position_y, .. } = settings;
    if size_x == 0 && size_y == 0 && position_x == 0 && position_y == 0 {
        window.fullscreen();
    } else {
        window.unfullscreen();
        window.resize(size_x, size_y);
        window.move_(position_x, position_y);
    }
}

fn convert_shot(surface_result: std::result::Result<Surface, glib::Error>) -> Result<Vec<u8>> {
    let img = ImageSurface::try_from(surface_result?)
        .map_err(|_| anyhow!("could not convert surface"))?;
    let mut vec = Vec::new();
    img.write_to_png(&mut vec)?;
    Ok(vec)
}
