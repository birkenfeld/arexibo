// Xibo player Rust implementation, (c) 2022 Georg Brandl.
// Licensed under the GNU AGPL, version 3 or later.

//! Main collect loop that also processes XMR requests.

use std::{path::Path, sync::Arc, thread, time::Duration};
use anyhow::{bail, Context, Result};
use crossbeam_channel::{after, never, select, tick, Receiver};
use rand::rngs::OsRng;
use rsa::{RsaPrivateKey, RsaPublicKey, pkcs8::{FromPrivateKey, ToPrivateKey, ToPublicKey}};
use crate::config::{CmsSettings, PlayerSettings};
use crate::{xmds, xmr};
use crate::resource::{Cache, LayoutInfo};
use crate::schedule::Schedule;

/// Messages sent to the GUI thread
pub enum Update {
    Settings(PlayerSettings),
    Layouts(Vec<Arc<LayoutInfo>>),
    Screenshot,
}

/// Backend handler that performs the collect loop and XMDS requests.
pub struct Handler {
    updates: glib::Sender<Update>,
    snaps: Receiver<Vec<u8>>,
    settings: PlayerSettings,
    xmds: xmds::Cms,
    cache: Cache,
    xmr: Receiver<xmr::Message>,
    schedule: Schedule,
    layouts: Vec<Arc<LayoutInfo>>,
}

impl Handler {
    /// Create a new handler, with channels to the GUI thread.
    pub fn new(cms: CmsSettings, workdir: &Path, updates: glib::Sender<Update>,
               snaps: Receiver<Vec<u8>>) -> Result<Self> {
        let (privkey, pubkey) = load_or_create_keypair(&workdir)?;
        let cache = Cache::new(workdir.join("res")).context("creating cache")?;
        let schedule = Schedule::default();
        let layouts = Default::default();

        // make an initial register call, in order to get player settings
        let mut xmds = xmds::Cms::new(&cms, pubkey);
        log::info!("doing initial register call to CMS");
        let res = xmds.register_display().context("initial registration")?;

        // if we got settings, we are registered and authorized
        if let Some(settings) = res {
            // create the XMR manager which sends us updates via channel
            let (manager, xmr) = xmr::Manager::new(&cms, &settings.xmr_network_address, privkey)?;
            thread::spawn(|| manager.run());

            let mut slf = Self { updates, snaps, settings, cache, xmds, xmr, schedule, layouts };
            slf.update_settings();
            Ok(slf)
        } else {
            bail!("display is not authorized yet, try again after authorization in the CMS");
        }
    }

    pub fn player_settings(&self) -> PlayerSettings {
        self.settings.clone()
    }

    /// Run the main collect loop.
    pub fn run(mut self) {
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
                        log::error!("during collect: {:#}", e);
                    }
                    collect = after(Duration::from_secs(self.settings.collect_interval));
                },
                // timer channel that fires when screenshot is needed
                recv(screenshot) -> _ => {
                    self.updates.send(Update::Screenshot).unwrap();
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
                    Err(_) => ()
                },
                // channel for screenshot data from the GUI thread
                recv(self.snaps) -> data => if let Ok(data) = data {
                    if let Err(e) = self.xmds.submit_screenshot(data) {
                        log::error!("submitting screenshot: {:#}", e);
                    }
                }
            }
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
        let required = self.xmds.required_files()?;

        // get the schedule
        let schedule = self.xmds.get_schedule()?;

        // download all missing files
        let mut result = Vec::new();
        for file in required {
            if !self.cache.has(&file) {
                let filedesc = file.description();
                let inventory = file.inventory();
                log::info!("downloading: {}", filedesc);
                match self.cache.download(file, &mut self.xmds)
                                .with_context(|| format!("downloading {}", filedesc))
                {
                    Ok(_) => result.push((inventory, true)),
                    Err(e) => {
                        log::error!("{:#}", e);
                        result.push((inventory, false));
                    }
                }
            }
        }

        // let the CMS know we have the media
        self.xmds.submit_media_inventory(result)?;

        // now that we should have all media, apply the schedule
        self.schedule = schedule;
        self.schedule_check();

        // TODO: send logs and stats

        log::info!("collection successful");
        Ok(())
    }

    /// Check if need to update the layouts to show.
    fn schedule_check(&mut self) {
        let new_layouts = self.schedule.layouts_now(&self.cache);
        if new_layouts != self.layouts {
            log::info!("schedule: new layouts {:?}", new_layouts);
            self.updates.send(Update::Layouts(new_layouts.clone())).unwrap();
            self.layouts = new_layouts;
        }
    }

    /// Apply new player settings.
    fn update_settings(&mut self) {
        // let the GUI know to reconfigure itself
        self.updates.send(Update::Settings(self.settings.clone())).unwrap();

        match &*self.settings.log_level {
            "trace" => log::set_max_level(log::LevelFilter::Trace),
            "debug" => log::set_max_level(log::LevelFilter::Debug),
            "info" => log::set_max_level(log::LevelFilter::Info),
            "error" => log::set_max_level(log::LevelFilter::Error),
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
        key.write_pkcs8_pem_file(dir.join("id_rsa"))?;
        key
    };
    let pubkey = RsaPublicKey::from(&privkey).to_public_key_pem()?;
    Ok((privkey, pubkey))
}
