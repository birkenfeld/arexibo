// Xibo player Rust implementation, (c) 2022-2024 Georg Brandl.
// Licensed under the GNU AGPL, version 3 or later.

//! Main collect loop that also processes XMR requests.

use std::{fmt, fs, path::{Path, PathBuf}, thread, time::Duration};
use anyhow::{bail, Context, Result};
use crossbeam_channel::{after, never, select, tick, Receiver, Sender};
use itertools::Itertools;
use rand::rngs::OsRng;
use rsa::{RsaPrivateKey, RsaPublicKey, pkcs8::{DecodePrivateKey, EncodePrivateKey, EncodePublicKey}};
use subprocess::Popen;
use crate::config::{CmsSettings, PlayerSettings};
use crate::{logger, util, xmds, xmr};
use crate::resource::Cache;
use crate::schedule::Schedule;

/// Error indicating the display is registered but not yet authorized in the CMS.
/// Uses a distinct exit code (2) so the kiosk session holder can wait patiently
/// instead of treating it as a configuration failure.
#[derive(Debug)]
pub struct NotAuthorized;

impl fmt::Display for NotAuthorized {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "display is not authorized yet, try again after authorization in the CMS")
    }
}

impl std::error::Error for NotAuthorized {}

/// Messages sent to the GUI thread
pub enum ToGui {
    Settings(PlayerSettings),
    Layouts(Vec<i64>),
    Screenshot,
    WebHook(String),
}

pub enum Kill {
    No,
    Terminate,
    Kill,
}

/// Messages received from the GUI thread
pub enum FromGui {
    Showing(i64),
    Screenshot(Vec<u8>),
    Command(String),
    Shell(String, bool),
    StopShell(Kill),
}

/// Backend handler that performs the collect loop and XMDS requests.
pub struct Handler {
    to_gui: Sender<ToGui>,
    from_gui: Receiver<FromGui>,
    settings: PlayerSettings,
    xmds: xmds::Cms,
    cache: Cache,
    envdir: PathBuf,
    xmr: Receiver<xmr::Message>,
    schedule: Schedule,
    layouts: Vec<i64>,
    current_layout: i64,
    shell_process: Option<Popen>,
}

impl Handler {
    /// Create a new handler, with channels to the GUI thread.
    pub fn new(cms: CmsSettings, clear_cache: bool, envdir: &Path,
               no_verify: bool, allow_offline: bool,
               to_gui: Sender<ToGui>, from_gui: Receiver<FromGui>) -> Result<Self> {
        let (privkey, pubkey) = load_or_create_keypair(envdir)?;
        let cache = Cache::new(&cms, envdir.join("res"), clear_cache, no_verify)
            .context("creating cache")?;
        let setting_file = envdir.join("settings.json");
        let sched_file = envdir.join("sched.json");
        let mut schedule = Schedule::default();
        let layouts = Default::default();

        // create directory to store raw XML responses for debugging
        let xmldir = envdir.join("xml");
        if !fs::metadata(&xmldir).map_or(false, |p| p.is_dir()) {
            fs::create_dir_all(&xmldir)?;
        }

        // make an initial register call, in order to get player settings
        let mut xmds = xmds::Cms::new(&cms, pubkey, no_verify, xmldir)?;
        log::info!("doing initial register call to CMS");

        // try initial register call
        let res = match xmds.register_display() {
            Err(e) => {
                if !allow_offline {
                    bail!("CMS not reachable or call failed: {e:#}");
                }
                log::warn!("CMS not reachable or call failed: {e:#}");
                match PlayerSettings::from_file(&setting_file) {
                    Ok(settings) => {
                        log::info!("using cached settings");

                        if let Ok(cached_sched) = Schedule::from_file(sched_file) {
                            log::info!("using cached schedule, experience may be degraded");
                            schedule = cached_sched;
                        }

                        Some(settings)
                    }
                    Err(_) => bail!("initial register failed and no cached settings available")
                }
            }
            Ok(res) => res
        };

        // if we got settings, we are registered and authorized
        if let Some(settings) = res {
            // create the XMR manager which sends us updates via channel
            let (manager, xmr) = xmr::Manager::new(&cms, &settings.xmr_network_address, privkey)?;
            thread::spawn(|| manager.run());

            settings.to_file(&setting_file).context("writing player settings")?;

            let mut slf = Self { to_gui, from_gui, settings, cache, xmds, xmr, schedule,
                                 layouts, envdir: envdir.into(), current_layout: 0,
                                 shell_process: None };
            slf.update_settings();
            slf.schedule_check();  // only useful in case of cached schedule
            Ok(slf)
        } else {
            return Err(NotAuthorized.into());
        }
    }

    pub fn player_settings(&self) -> PlayerSettings {
        self.settings.clone()
    }

    /// Run the main collect loop.
    pub fn run(mut self) -> Result<()> {
        let mut collect = after(Duration::from_secs(0));  // do first collect immediately
        let mut screenshot = if self.settings.screenshot_interval != 0 {
            after(Duration::from_secs(self.settings.screenshot_interval * 60))
        } else {
            never()
        };
        let schedule_check = tick(Duration::from_secs(60));
        loop {
            select! {
                // timer channel that fires when collect is needed
                recv(collect) -> _ => {
                    if let Err(e) = self.collect_once() {
                        log::error!("during collect: {e:#}");
                    }
                    collect = after(Duration::from_secs(self.settings.collect_interval));
                },
                // timer channel that fires when screenshot is needed
                recv(screenshot) -> _ => {
                    self.to_gui.send(ToGui::Screenshot).unwrap();
                    screenshot = if self.settings.screenshot_interval != 0 {
                        after(Duration::from_secs(self.settings.screenshot_interval * 60))
                    } else {
                        never()
                    };
                },
                // timer channel that fires every minute, to check if current layouts change
                recv(schedule_check) -> _ => {
                    self.schedule_check();
                },
                // channel for XMR messages
                recv(self.xmr) -> msg => match msg {
                    Ok(xmr::Message::CollectNow) => collect = after(Duration::from_secs(0)),
                    Ok(xmr::Message::Screenshot) => screenshot = after(Duration::from_secs(0)),
                    Ok(xmr::Message::Purge) => {
                        if let Err(e) = self.cache.purge() {
                            log::error!("durign cache purge: {e:#}");
                        }
                        collect = after(Duration::from_secs(0));  // force re-download
                    }
                    Ok(xmr::Message::WebHook(code)) => {
                        self.to_gui.send(ToGui::WebHook(code)).unwrap();
                    }
                    Ok(xmr::Message::Command(code)) => {
                        self.run_command(&code);
                    }
                    Err(_) => ()
                },
                // channel for  from the GUI thread
                recv(self.from_gui) -> data => match data {
                    Ok(FromGui::Screenshot(data)) => {
                        if let Err(e) = self.xmds.submit_screenshot(data) {
                            log::error!("submitting screenshot: {e:#}");
                        }
                    }
                    Ok(FromGui::Showing(layout)) =>
                        self.current_layout = layout,
                    Ok(FromGui::Command(code)) =>
                        self.run_command(&code),
                    Ok(FromGui::Shell(code, with_shell)) =>
                        self.run_shell(code, with_shell),
                    Ok(FromGui::StopShell(kill_mode)) => {
                        if let Some(mut child) = self.shell_process.take() {
                            match kill_mode {
                                Kill::No => self.shell_process = None,  // let it run
                                Kill::Terminate => { let _ = child.terminate(); }
                                Kill::Kill => { let _ = child.kill(); }
                            }
                        }
                    }
                    Err(_) => ()
                }
            }
        }
    }

    /// Run a command, triggered from XMR or layout.
    fn run_command(&mut self, code: &str) {
        if let Some(cmd) = self.settings.commands.get(code) {
            let success = match cmd.run() {
                Ok(success) => success,
                Err(e) => {
                    log::warn!("running command {code}: {e:#}");
                    false
                }
            };
            let _ = self.xmds.notify_command_success(success);
        } else {
            log::error!("no such player command: {code}");
        }
    }

    /// Run a shell command, triggered from layout.
    fn run_shell(&mut self, code: String, with_shell: bool) {
        let config = Default::default();
        let res = if with_shell {
            Popen::create(&["/bin/sh", "-c", &code], config)
        } else {
            if let Some(parts) = shlex::split(&code) {
                Popen::create(&parts, config)
            } else {
                log::error!("invalid command line: {code}");
                return;
            }
        };
        match res {
            Ok(child) => self.shell_process = Some(child),
            Err(e) => log::error!("spawning command {code}: {e:#}"),
        }
    }

    /// Do a single collection cycle.
    fn collect_once(&mut self) -> Result<()> {
        log::info!("doing collection");

        // call register to get updated player settings
        if let Some(settings) = self.xmds.register_display()? {
            if settings != self.settings {
                self.settings = settings;
                self.update_settings();
            }
        } else {
            bail!("display is not authorized anymore");
        }

        // get the missing files
        let (required, purge) = self.xmds.required_files()?;

        // update layout code map
        self.cache.update_code_map(&required)?;

        // purge files
        let _ = self.cache.purge_some(&purge);

        // get the schedule
        let schedule = self.xmds.get_schedule()?;

        // download all missing files
        let mut result = Vec::new();
        let total = required.len();
        for (i, file) in required.into_iter().enumerate() {
            if !self.cache.has(&file) {
                let filedesc = file.description();
                let inventory = file.inventory();
                log::info!("downloading required file {}/{}: {}", i+1, total, filedesc);
                match self.cache.download(file, &mut self.xmds)
                                .with_context(|| format!("downloading {}", filedesc))
                {
                    Ok(_) => result.push((inventory, true)),
                    Err(e) => {
                        log::error!("{e:#}");
                        result.push((inventory, false));
                    }
                }
            }
        }

        // let the CMS know we have the media
        self.xmds.submit_media_inventory(result)?;

        // now that we should have all media, apply the schedule
        self.schedule = schedule;
        let _ = self.schedule.to_file(self.envdir.join("sched.json"));
        self.schedule_check();

        // send log messages
        self.xmds.submit_log(&logger::pop_entries())?;

        // collect status info
        let (avail, total) = util::space_info(self.cache.dir())?;
        let status = xmds::Status {
            currentLayoutId: self.current_layout,
            availableSpace: avail,
            totalSpace: total,
            lastCommandSuccess: false,  // not implemented yet
            deviceName: &self.settings.display_name,
            timeZone: &util::timezone(),
        };
        self.xmds.notify_status(status)?;

        log::info!("collection successful");
        Ok(())
    }

    /// Check if need to update the layouts to show.
    fn schedule_check(&mut self) {
        let new_layouts = self.schedule.layouts_now();
        if new_layouts != self.layouts {
            log::info!("new layouts in schedule: {}",
                       new_layouts.iter().format(", ").to_string());
            self.to_gui.send(ToGui::Layouts(new_layouts.clone())).unwrap();
            self.layouts = new_layouts;
        }
    }

    /// Apply new player settings.
    fn update_settings(&mut self) {
        // let the GUI know to reconfigure itself
        self.to_gui.send(ToGui::Settings(self.settings.clone())).unwrap();

        match &*self.settings.log_level {
            "trace" => log::set_max_level(log::LevelFilter::Trace),
            "debug" => log::set_max_level(log::LevelFilter::Debug),
            "info" => log::set_max_level(log::LevelFilter::Info),
            "error" => log::set_max_level(log::LevelFilter::Warn),
            "off" => log::set_max_level(log::LevelFilter::Off),
            s => log::error!("invalid log level {}", s)
        }
    }
}


/// Load the RSA private key for the XML channel from disk, or create a new
/// key if needed.  Returns the public key as a PEM string, which is how
/// it needs to be sent to the CMS.
fn load_or_create_keypair(dir: &Path) -> Result<(RsaPrivateKey, String)> {
    let privkey = if let Ok(key) = RsaPrivateKey::read_pkcs8_pem_file(dir.join("id_rsa")) {
        key
    } else {
        log::info!("generating new RSA key for XMR, please wait...");
        let key = RsaPrivateKey::new(&mut OsRng, 2048)?;
        key.write_pkcs8_pem_file(dir.join("id_rsa"), Default::default())?;
        key
    };
    let pubkey = RsaPublicKey::from(&privkey).to_public_key_pem(Default::default())?;
    Ok((privkey, pubkey))
}
