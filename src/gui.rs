// Xibo player Rust implementation, (c) 2022 Georg Brandl.
// Licensed under the GNU AGPL, version 3 or later.

//! The GUI window.

use std::convert::TryFrom;
use std::{cell::RefCell, io::Cursor, rc::Rc, sync::Arc};
use anyhow::{anyhow, Result};
use cairo::{ImageSurface, Surface};
use crossbeam_channel::Sender;
use glib::{clone, prelude::*};
use gdk_pixbuf::Pixbuf;
use gtk::{prelude::*, Inhibit, Window, WindowType};
use webkit2gtk::{WebContext, WebView, UserContentManager, SnapshotRegion, SnapshotOptions,
                 JavascriptResult};
use webkit2gtk::traits::{UserContentManagerExt, SettingsExt, WebViewExt, WebInspectorExt};
use crate::collect::Update;
use crate::config::PlayerSettings;
use crate::resource::LayoutInfo;

const LOGO_PNG: &[u8] = include_bytes!("../assets/logo.png");


pub fn run(settings: PlayerSettings, inspect: bool,
           updates: glib::Receiver<Update>, snaps: Sender<Vec<u8>>) -> Result<()> {
    gtk::init().expect("failed to init gtk");
    let base_uri = format!("http://localhost:{}/", settings.embedded_server_port);

    let logo = Pixbuf::from_read(Cursor::new(LOGO_PNG))?;

    let window = Window::new(WindowType::Toplevel);
    let context = WebContext::default().unwrap();
    let manager = UserContentManager::new();
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

    webview.load_uri(&format!("{}0.xlf.html", base_uri));
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

    let schedule = Rc::new(RefCell::new(Schedule::<Arc<LayoutInfo>>::default()));

    manager.connect_local("script-message-received::xibo", false, clone!(
        @strong schedule, @strong base_uri, @weak webview => @default-return None,
        move |args| {
            if let Some(arg) = args.get(1).and_then(|a| a.get::<JavascriptResult>().ok()) {
                if let Some(ctx) = arg.global_context() {
                    if let Some(event) = arg.value().and_then(|v| v.to_string(&ctx)) {
                        match &*event {
                            "layout_done" => {
                                if let Some(info) = schedule.borrow_mut().next() {
                                    log::info!("showing next layout: {}", info.id);
                                    // TODO: adapt webview scale to actual vs. designed size
                                    webview.load_uri(&format!("{}{}.xlf.html", base_uri, info.id));
                                }
                            }
                            _ => ()
                        }
                    }
                }
            }
            None
        }
    ));

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
                Update::Layouts(new_layouts) => {
                    // TODO: adapt webview scale to actual vs. designed size
                    if let Some(info) = schedule.borrow_mut().update(new_layouts) {
                        log::info!("new schedule, showing layout: {}", info.id);
                        webview.load_uri(&format!("{}{}.xlf.html", base_uri, info.id));
                    }
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


/// Keeps track of scheduled layouts and the currently shown one.
#[derive(Debug, Default)]
struct Schedule<T> {
    index: Option<usize>,
    layouts: Vec<T>,
}

impl<T: Eq + Default + Clone> Schedule<T> {
    /// Update the scheduled layouts and return Some(id) if we need to change
    fn update(&mut self, new: Vec<T>) -> Option<T> {
        // determine the currently shown layout
        let cur_id = self.index.map(|i| self.layouts[i].clone()).unwrap_or_default();
        self.layouts = new;

        // if this layout is also in the new schedule, keep it
        if let Some(new_index) = self.layouts.iter().position(|id| id == &cur_id) {
            self.index = Some(new_index);
            None
        } else if !self.layouts.is_empty() {
            // otherwise, start showing the first of the new layouts if we have some
            self.index = Some(0);
            Some(self.layouts[0].clone())
        } else {
            // as last resort, show the splash screen
            self.index = None;
            Some(Default::default())
        }
    }

    /// Go to the next layout, if more than one is scheduled, and return Some(id)
    fn next(&mut self) -> Option<T> {
        let nlayouts = self.layouts.len();
        // if there is no layout or only one scheduled, no change
        if nlayouts < 2 {
            None
        } else {
            // otherwise just go further in the schedule
            let new_index = (self.index.unwrap() + 1) % nlayouts;
            self.index = Some(new_index);
            Some(self.layouts[new_index].clone())
        }
    }
}

#[cfg(test)]
#[test]
fn test_schedule() {
    let mut schedule = Schedule { index: None, layouts: vec![] };
    assert_eq!(schedule.next(), None);
    assert_eq!(schedule.update(vec![]), Some(0));
    assert_eq!(schedule.update(vec![1]), Some(1));
    assert_eq!(schedule.update(vec![1]), None);
    assert_eq!(schedule.update(vec![2, 1, 3]), None);
    assert_eq!(schedule.next(), Some(3));
    assert_eq!(schedule.next(), Some(2));
    assert_eq!(schedule.update(vec![1, 3]), Some(1));
}
