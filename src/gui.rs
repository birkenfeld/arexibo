// Xibo player Rust implementation, (c) 2022-2024 Georg Brandl.
// Licensed under the GNU AGPL, version 3 or later.

//! Bindings to the C++/Qt GUI part of the application.

use std::ffi::{c_void, CStr, CString};
use std::sync::{Arc, Mutex};
use crossbeam_channel::{Sender, Receiver};
use crate::config::PlayerSettings;
use crate::mainloop::{ToGui, FromGui, Kill};
use crate::resource::LayoutId;

#[path = "qt_binding.rs"]
#[allow(non_camel_case_types)]
mod cpp;

struct CallbackData {
    sender: Sender<FromGui>,
    schedule: Arc<Mutex<Schedule<LayoutId>>>,
}

pub fn run(settings: PlayerSettings, inspect: bool, debug: bool,
           togui: Receiver<ToGui>, fromgui: Sender<FromGui>) {
    let base_uri = format!("http://localhost:{}/", settings.embedded_server_port);
    let fromgui_2 = fromgui.clone();

    let schedule = Arc::new(Mutex::new(Schedule::<LayoutId>::default()));

    let cb_data = CallbackData { sender: fromgui_2, schedule: schedule.clone() };
    let cb_data = Box::leak(Box::new(cb_data)) as *mut _ as *mut c_void;

    let title = CString::new(settings.display_name).unwrap();
    let base_uri = CString::new(base_uri).unwrap();
    unsafe {
        cpp::setup(base_uri.as_ptr(), inspect as _, debug as _, Some(callback), cb_data);
        cpp::set_title(title.as_ptr());
        cpp::set_size(settings.pos_x as _, settings.pos_y as _,
                      settings.size_x as _, settings.size_y as _);
    }

    std::thread::spawn(move || {
        for msg in togui {
            match msg {
                ToGui::Screenshot => {
                    unsafe { cpp::screenshot(); }
                }
                ToGui::Settings(s) => {
                    let title = CString::new(s.display_name).unwrap();
                    unsafe {
                        cpp::set_title(title.as_ptr());
                        cpp::set_size(s.pos_x as _, s.pos_y as _, s.size_x as _, s.size_y as _);
                    }
                }
                ToGui::Layouts(new_layouts) => {
                    if let Some(id) = schedule.lock().unwrap().update(new_layouts) {
                        log::info!("new schedule, showing layout: {}", id);
                        let file = CString::new(format!("{}.xlf.html", id)).unwrap();
                        unsafe {
                            cpp::navigate(file.as_ptr());
                        }
                    }
                }
                ToGui::WebHook(code) => {
                    let code = CString::new(format!(
                        "window.arexibo.trigger(\"{code}\");")).unwrap();
                    unsafe {
                        cpp::run_js(code.as_ptr());
                    }
                }
            }
        }
    });


    unsafe {
        cpp::run();
    }
}

extern "C" fn callback(ptr: *mut c_void, typ: isize, arg1: isize, arg2: isize, _arg3: isize) {
    let cb_data = unsafe { &*(ptr as *const CallbackData) };

    match typ {
        cpp::CB_SCREENSHOT => {
            let data = unsafe { std::slice::from_raw_parts(arg1 as *const u8, arg2 as usize) };
            cb_data.sender.send(FromGui::Screenshot(data.to_vec())).unwrap();
        }
        cpp::CB_LAYOUT_INIT => {
            if arg1 > 0 {  // don't announce the splash screen
                cb_data.sender.send(FromGui::Showing(arg1 as _)).unwrap();
            }
        }
        cpp::CB_LAYOUT_NEXT => {
            let mut schedule = cb_data.schedule.lock().unwrap();
            if let Some(id) = schedule.next() {
                log::info!("showing next layout: {}", id);
                let file = CString::new(format!("{}.xlf.html", id)).unwrap();
                unsafe {
                    cpp::navigate(file.as_ptr());
                }
            } else {
                schedule.mark_done();
            }
        }
        cpp::CB_LAYOUT_PREV => {
            if let Some(id) = cb_data.schedule.lock().unwrap().prev() {
                log::info!("showing previous layout: {}", id);
                let file = CString::new(format!("{}.xlf.html", id)).unwrap();
                unsafe {
                    cpp::navigate(file.as_ptr());
                }
            }
        }
        cpp::CB_LAYOUT_JUMP => {
            log::info!("jumping to layout: {}", arg2);
            let file = CString::new(format!("{}.xlf.html", arg2)).unwrap();
            unsafe {
                cpp::navigate(file.as_ptr());
            }
        }
        cpp::CB_COMMAND | cpp::CB_SHELL => {
            let cmd = unsafe { CStr::from_ptr(arg1 as *const _) };
            let cmd = cmd.to_str().unwrap_or_default().to_owned();
            if typ == cpp::CB_SHELL {
                let use_shell = arg2 != 0;
                cb_data.sender.send(FromGui::Shell(cmd, use_shell)).unwrap();
            } else {
                cb_data.sender.send(FromGui::Command(cmd)).unwrap();
            }
        }
        cpp::CB_STOPSHELL => {
            let killmode = match arg2 & 0xff {
                0 => Kill::No,
                1 => Kill::Terminate,
                _ => Kill::Kill,
            };
            cb_data.sender.send(FromGui::StopShell(killmode)).unwrap();
        }
        _ => {
            log::warn!("got unknown callback from Qt: {}", typ);
        }
    }
}

/// Keeps track of scheduled layouts and the currently shown one.
#[derive(Debug, Default)]
struct Schedule<T> {
    index: Option<usize>,
    layouts: Vec<T>,
    single_done: bool,
}

impl<T: Eq + Default + Clone> Schedule<T> {
    /// Update the scheduled layouts and return Some(id) if we need to change
    fn update(&mut self, new: Vec<T>) -> Option<T> {
        // determine the currently shown layout
        let cur_t = self.current();
        self.layouts = new;

        // if this layout is also in the new schedule, keep it
        if let Some(new_index) = self.layouts.iter().position(|t| t == &cur_t) {
            if self.single_done {
                self.index = Some((new_index + 1) % self.layouts.len());
            } else {
                self.index = Some(new_index);
            }
            self.single_done = false;
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

    /// Go to the previous layout, if more than one is scheduled, and return Some(id)
    fn prev(&mut self) -> Option<T> {
        let nlayouts = self.layouts.len();
        // if there is no layout or only one scheduled, no change
        if nlayouts < 2 {
            None
        } else {
            // otherwise just go further in the schedule
            let new_index = (self.index.unwrap() + nlayouts - 1) % nlayouts;
            self.index = Some(new_index);
            Some(self.layouts[new_index].clone())
        }
    }

    /// Return current layout.
    fn current(&self) -> T {
        self.index.map(|i| self.layouts[i].clone()).unwrap_or_default()
    }

    /// Mark current layout as having run.
    fn mark_done(&mut self) {
        self.single_done = true;
    }
}

#[cfg(test)]
#[test]
fn test_schedule() {
    let mut schedule = Schedule { index: None, layouts: vec![], single_done: false };
    assert_eq!(schedule.next(), None);
    assert_eq!(schedule.update(vec![]), Some(0));
    assert_eq!(schedule.update(vec![1]), Some(1));
    assert_eq!(schedule.update(vec![1]), None);
    assert_eq!(schedule.update(vec![2, 1, 3]), None);
    assert_eq!(schedule.next(), Some(3));
    assert_eq!(schedule.next(), Some(2));
    assert_eq!(schedule.update(vec![1, 3]), Some(1));
}
