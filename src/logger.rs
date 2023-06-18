// Xibo player Rust implementation, (c) 2022 Georg Brandl.
// Licensed under the GNU AGPL, version 3 or later.

//! Xibo logger.

use time::OffsetDateTime;
use parking_lot::{Mutex, const_mutex};

/// A single cached log entry.
pub struct LogEntry {
    pub date: OffsetDateTime,
    pub category: &'static str,
    pub message: String,
}


static LOG_ENTRIES: Mutex<Vec<LogEntry>> = const_mutex(Vec::new());

/// Xibo logger, logs to console and stores entries for transfer to
/// the display.
pub struct Logger;

impl log::Log for Logger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        // filter out messages not from our modules
        let path = record.module_path().unwrap_or("");
        if !path.starts_with("arexibo") {
            return;
        }

        // print to console
        println!("{:5}: [{}] {}", record.level(), path, record.args());

        // add to stashed entries for submission to CMS
        let mut entries = LOG_ENTRIES.lock();
        // avoid taking up arbitrary amounts of memory
        if entries.len() > 1000 {
            entries.drain(0..500).for_each(drop);
        }
        entries.push(LogEntry {
            date: OffsetDateTime::now_local().unwrap(),
            category: record.level().as_str(),
            message: record.args().to_string(),
        });
    }

    fn flush(&self) {}
}

pub fn pop_entries() -> Vec<LogEntry> {
    std::mem::take(&mut LOG_ENTRIES.lock())
}
