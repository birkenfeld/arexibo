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
use gtk::{prelude::*, Fixed, Inhibit, Window, WindowType};
use webkit2gtk::{WebContext, WebView, UserContentManager, SnapshotRegion, SnapshotOptions,
                 JavascriptResult};
use webkit2gtk::traits::{UserContentManagerExt, SettingsExt, WebViewExt, WebInspectorExt};
use crate::collect::{FromGui, ToGui};
use crate::config::PlayerSettings;
use crate::resource::LayoutInfo;

const LOGO_PNG: &[u8] = include_bytes!("../assets/logo.png");


pub fn run(settings: PlayerSettings, inspect: bool,
           to_gui: glib::Receiver<ToGui>, from_gui: Sender<FromGui>) -> Result<()> {
    gtk::init().expect("failed to init gtk");
    let base_uri = format!("http://localhost:{}/", settings.embedded_server_port);

    let logo = Pixbuf::from_read(Cursor::new(LOGO_PNG))?;

    let window = Window::new(WindowType::Toplevel);
    let container = Fixed::new();

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
    container.put(&webview, 0, 0);
    window.add(&container);
    window.set_app_paintable(true);
    window.set_decorated(false);
    window.set_title(&settings.display_name);
    window.set_icon(Some(&logo));
    apply_size(&window, settings);
    apply_scale((0, 0), &window, &container, &webview);
    window.show_all();

    if let Some(gdkwin) = window.window() {
        gdkwin.set_background_rgba(&gdk::RGBA { red: 0., green: 0., blue: 0., alpha: 1. });
        gdkwin.set_cursor(Some(&gdk::Cursor::for_display(
            &gdk::Display::default().unwrap(),
            gdk::CursorType::BlankCursor)));
    }

    window.connect_delete_event(|_, _| {
        gtk::main_quit();
        Inhibit(false)
    });

    let schedule = Rc::new(RefCell::new(Schedule::<Arc<LayoutInfo>>::default()));

    // handler for events from the webview content
    let from_gui_2 = from_gui.clone();
    manager.connect_local("script-message-received::xibo", false, clone!(
        @strong schedule, @strong base_uri, @weak webview, @weak window,
        @weak container => @default-return None,
        move |args| {
            if let Some(request) = extract_js_string(args.get(1)).as_deref() {
                if request == "layout_done" {
                    // layout has run through, need to change layouts?
                    if let Some(info) = schedule.borrow_mut().next() {
                        log::info!("showing next layout: {}", info.id);
                        apply_scale(info.size, &window, &container, &webview);
                        webview.load_uri(&format!("{}{}.xlf.html", base_uri, info.id));
                        from_gui_2.send(FromGui::Showing(info.id)).unwrap();
                    } else {
                        // TODO: record that the layout is done so that we
                        // can switch to the next one on update.
                    }
                } else if request.starts_with("play:") {
                    // request to start a non-muted video which needs to come
                    // from outside the webview...
                    webview.run_javascript(
                        &format!("document.getElementById('m{}').play();", &request[5..]),
                        None::<&gio::Cancellable>, |_| ());
                }
            }
            None
        }
    ))?;

    // handler for events from the collect backend
    to_gui.attach(None, clone!(
        @weak webview, @weak window, @weak container => @default-return Continue(true),
        move |update| {
            match update {
                ToGui::Screenshot => {
                    let channel = from_gui.clone();
                    webview.snapshot(
                        SnapshotRegion::Visible,
                        SnapshotOptions::NONE,
                        None::<&gio::Cancellable>,
                        move |result| match convert_shot(result) {
                            Ok(data) => channel.send(FromGui::Screenshot(data)).unwrap(),
                            Err(e) => log::warn!("could not create snapshot: {:#}", e),
                        });
                }
                ToGui::Settings(settings) => {
                    window.set_title(&settings.display_name);
                    apply_size(&window, settings);
                    apply_scale(schedule.borrow().current().size,
                                &window, &container, &webview);
                }
                ToGui::Layouts(new_layouts) => {
                    if let Some(info) = schedule.borrow_mut().update(new_layouts) {
                        log::info!("new schedule, showing layout: {}", info.id);
                        apply_scale(info.size, &window, &container, &webview);
                        webview.load_uri(&format!("{}{}.xlf.html", base_uri, info.id));
                        from_gui.send(FromGui::Showing(info.id)).unwrap();
                    }
                }
            }
            Continue(true)
        }
    ));

    gtk::main();

    Ok(())
}

fn extract_js_string(arg: Option<&glib::Value>) -> Option<String> {
    Some(arg?.get::<JavascriptResult>().ok()?.js_value()?.to_string())
}

fn apply_size(window: &Window, settings: PlayerSettings) {
    let (screen_w, screen_h) = if let Some(screen) = window.screen() {
        let pos = window.position();
        let monitor = screen.monitor_at_point(pos.0, pos.1);
        let size = screen.monitor_geometry(monitor);
        (size.width, size.height)
    } else {
        return;
    };
    let PlayerSettings { mut size_x, mut size_y, pos_x, pos_y, .. } = settings;
    if size_x == 0 && size_y == 0 && pos_x == 0 && pos_y == 0 {
        window.fullscreen();
        window.set_size_request(screen_w, screen_h);
        window.resize(screen_w, screen_h);
    } else {
        if size_x == 0 { size_x = screen_w; }
        if size_y == 0 { size_y = screen_h; }
        window.unfullscreen();
        window.set_size_request(size_x, size_y);
        window.resize(size_x, size_y);
        window.move_(pos_x, pos_y);
    }
}

fn apply_scale(size: (i32, i32), window: &Window, container: &Fixed, webview: &WebView) {
    let (window_w, window_h) = window.size_request();
    let (mut layout_w, mut layout_h) = size;
    // the easy case: direct match
    if window_w == layout_w && window_h == layout_h {
        container.move_(webview, 0, 0);
        webview.set_size_request(layout_w, layout_h);
        webview.set_zoom_level(1.0);
        return;
    }
    // nothing specified for the layout (e.g. splash)
    if layout_w == 0 || layout_h == 0 {
        layout_w = 1920;
        layout_h = 1080;
    }
    let window_aspect = (window_w as f64) / (window_h as f64);
    let layout_aspect = (layout_w as f64) / (layout_h as f64);
    if window_aspect > layout_aspect {
        let scale_factor = (window_h as f64) / (layout_h as f64);
        let webview_w = (layout_w as f64 * scale_factor).round() as i32;
        container.move_(webview, (window_w - webview_w) / 2, 0);
        webview.set_size_request(webview_w, window_h);
        webview.set_zoom_level(scale_factor);
    } else {
        let scale_factor = (window_w as f64) / (layout_w as f64);
        let webview_h = (layout_h as f64 * scale_factor).round() as i32;
        container.move_(webview, 0, (window_h - webview_h) / 2);
        webview.set_size_request(window_w, webview_h);
        webview.set_zoom_level(scale_factor);
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
        let cur_t = self.current();
        self.layouts = new;

        // if this layout is also in the new schedule, keep it
        if let Some(new_index) = self.layouts.iter().position(|t| t == &cur_t) {
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

    /// Return current layout.
    fn current(&self) -> T {
        self.index.map(|i| self.layouts[i].clone()).unwrap_or_default()
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
