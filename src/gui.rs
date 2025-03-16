// Xibo player Rust implementation, (c) 2022-2024 Georg Brandl.
// Licensed under the GNU AGPL, version 3 or later.

//! Bindings to the C++/Qt GUI part of the application.

use std::ffi::{c_char, c_void, CString};
use std::sync::{Arc, Mutex};
use crossbeam_channel::{Sender, Receiver};
use crate::config::PlayerSettings;
use crate::collect::{ToGui, FromGui};
use crate::resource::LayoutInfo;

#[path = "qt_binding.rs"]
mod cpp;

struct CallbackData {
    sender: Sender<FromGui>,
    schedule: Arc<Mutex<Schedule<Arc<LayoutInfo>>>>,
}

pub fn run(settings: PlayerSettings, inspect: bool,
           togui: Receiver<ToGui>, fromgui: Sender<FromGui>) {
    let base_uri = format!("http://localhost:{}/", settings.embedded_server_port);
    let fromgui_2 = fromgui.clone();

    let schedule = Arc::new(Mutex::new(Schedule::<Arc<LayoutInfo>>::default()));

    let cb_data = CallbackData { sender: fromgui_2, schedule: schedule.clone() };
    let cb_data = Box::leak(Box::new(cb_data)) as *mut _ as *mut c_void;

    let title = CString::new(settings.display_name).unwrap();
    let base_uri = CString::new(base_uri).unwrap();
    unsafe {
        cpp::setup(base_uri.as_ptr(), inspect as _, cb_data,
                   layoutdone_callback as *mut c_void,
                   screenshot_callback as *mut c_void);
        cpp::set_title(title.as_ptr());
        cpp::set_size(settings.pos_x as _, settings.pos_y as _,
                      settings.size_x as _, settings.size_y as _);
        cpp::set_scale(0, 0);
    }

    std::thread::spawn(move || {
        for msg in togui {
            match msg {
                ToGui::Screenshot => {
                    unsafe { cpp::screenshot(); }
                }
                ToGui::Settings(s) => {
                    let layout_size = schedule.lock().unwrap().current().size;
                    let title = CString::new(s.display_name).unwrap();
                    unsafe {
                        cpp::set_title(title.as_ptr());
                        cpp::set_size(s.pos_x as _, s.pos_y as _, s.size_x as _, s.size_y as _);
                        cpp::set_scale(layout_size.0 as _, layout_size.1 as _);
                    }
                }
                ToGui::Layouts(new_layouts) => {
                    if let Some(info) = schedule.lock().unwrap().update(new_layouts) {
                        log::info!("new schedule, showing layout: {}", info.id);
                        let file = CString::new(format!("{}.xlf.html", info.id)).unwrap();
                        unsafe {
                            cpp::set_scale(info.size.0 as _, info.size.1 as _);
                            cpp::navigate(file.as_ptr());
                        }
                        fromgui.send(FromGui::Showing(info.id)).unwrap();
                    }
                }
            }
        }
    });


    unsafe {
        cpp::run();
    }
}

extern "C" fn layoutdone_callback(ptr: *mut c_void) {
    let cb_data = unsafe { &*(ptr as *const CallbackData) };
    if let Some(info) = cb_data.schedule.lock().unwrap().next() {
        log::info!("showing next layout: {}", info.id);
        let file = CString::new(format!("{}.xlf.html", info.id)).unwrap();
        unsafe {
            cpp::set_scale(info.size.0 as _, info.size.1 as _);
            cpp::navigate(file.as_ptr());
        }
        cb_data.sender.send(FromGui::Showing(info.id)).unwrap();
    } else {
        // TODO: record that the layout is done so that we
        // can switch to the next one on update.
    }
}

extern "C" fn screenshot_callback(ptr: *mut c_void, data: *mut c_char, len: usize) {
    let cb_data = unsafe { &*(ptr as *const CallbackData) };
    let data = unsafe { std::slice::from_raw_parts(data as *const u8, len) };
    cb_data.sender.send(FromGui::Screenshot(data.to_vec())).unwrap();
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
