// Xibo player Rust implementation, (c) 2022 Georg Brandl.
// Licensed under the GNU AGPL, version 3 or later.

//! Main entry point for the application.

#[cfg(feature = "gui")]
pub mod gui;
pub mod config;
pub mod collect;
pub mod server;
pub mod resource;
pub mod schedule;
pub mod layout;
pub mod xmds;
pub mod xmr;
pub mod util;

use std::path::PathBuf;
use anyhow::Context;
use clap::Parser;

#[derive(Parser)]
#[clap(author, version, about)]
struct Args {
    /// The directory to place config files and cached content.
    /// Defaults to the current directory.
    workdir: Option<String>,
    /// The CMS host including scheme, e.g. https://xibo.example.com/
    #[clap(long)]
    host: Option<String>,
    /// The CMS secret key for displays.
    #[clap(long)]
    cms_key: Option<String>,
    /// The ID for this display.  Use e.g. /etc/machine-id.
    #[clap(long)]
    display_id: Option<String>,
    /// Show web inspector to debug layout problems.
    #[clap(long)]
    inspect: bool,
}

fn main() {
    log::set_logger(&util::ConsoleLog).expect("failed to set logger");
    log::set_max_level(log::LevelFilter::Debug);
    if let Err(e) = main_inner() {
        log::error!("exiting on error: {:#}", e);
    }
}

fn main_inner() -> anyhow::Result<()> {
    log::info!("Arexibo {} starting up...", clap::crate_version!());

    let args = Args::parse();

    let workdir = PathBuf::from(args.workdir.as_deref().unwrap_or("."));
    let cmscfg = workdir.join("cms.json");

    let settings = if let (Some(address), Some(key), Some(display_id)) = (args.host, args.cms_key, args.display_id) {
        config::CmsSettings { address, key, display_id }
    } else if let Ok(from_json) = config::CmsSettings::from_file(&cmscfg) {
        from_json
    } else {
        anyhow::bail!("cms.json not found or invalid, run with the --host, --cms-key \
                       and --display-id options to reconfigure");
    };

    settings.to_file(&cmscfg).context("writing new CMS config")?;

    let (updates_tx, updates_rx) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);
    let (snaps_tx, snaps_rx) = crossbeam_channel::bounded(1);

    let handler = collect::Handler::new(settings, &workdir, updates_tx, snaps_rx)
        .context("creating backend handler")?;
    let settings = handler.player_settings();

    // TODO: prevent sleep

    let webserver = server::Server::new(workdir.join("res"), settings.embedded_server_port)
        .context("creating internal HTTP server")?;
    webserver.start_pool();

    #[cfg(feature = "gui")]
    {
        std::thread::spawn(|| handler.run());
        return gui::run(settings, args.inspect, updates_rx, snaps_tx);
    }
    #[cfg(not(feature = "gui"))]
    {
        let _unused = (updates_rx, snaps_tx);
        handler.run();
        Ok(())
    }
}
