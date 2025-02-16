// Xibo player Rust implementation, (c) 2022-2024 Georg Brandl.
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
pub mod logger;
pub mod util;

use std::path::PathBuf;
use anyhow::{ensure, Context};
use clap::Parser;

#[derive(Parser)]
#[command(author, version, about)]
#[command(help_template = "by {author}{about-section}\n{usage-heading} {usage}\n\n{all-args}")]
struct Args {
    /// The directory to place config files and cached content.
    envdir: PathBuf,
    /// The CMS host including scheme, e.g. https://xibo.example.com/
    #[arg(long)]
    host: Option<String>,
    /// The CMS secret key.
    #[arg(long)]
    key: Option<String>,
    /// The ID for this display, autogenerated if omitted.
    #[arg(long)]
    display_id: Option<String>,
    /// The initial name for this display.
    #[arg(long)]
    display_name: Option<String>,
    /// URL for a proxy server for HTTP/XMDS requests.
    #[arg(long)]
    proxy: Option<String>,
    /// Show web inspector to debug layout problems.
    #[arg(long)]
    inspect: bool,
    /// Clear the local file cache and re-download any files.
    #[arg(long)]
    clear: bool,
    /// Disable HTTPS certificate verification.
    #[arg(long)]
    no_verify: bool,
    /// Allow starting and running without connection to the CMS,
    /// showing the last cached schedule.
    #[arg(long)]
    allow_offline: bool,
}

fn main() {
    log::set_logger(&logger::Logger).expect("failed to set logger");
    log::set_max_level(log::LevelFilter::Debug);
    if let Err(e) = main_inner() {
        log::error!("exiting on error: {:#}", e);
    }
}

fn main_inner() -> anyhow::Result<()> {
    log::info!("Arexibo {} starting up...", clap::crate_version!());

    let args = Args::parse();

    // check environment directory argument
    ensure!(args.envdir.exists(), "environment directory '{}' does not exist",
            args.envdir.display());
    let cmscfg = args.envdir.join("cms.json");

    // check if we have a CMS config either stored, or given with arguments
    let cms = if let Some((address, key)) = args.host.zip(args.key) {
        let display_id = args.display_id.unwrap_or_else(util::get_display_id);
        config::CmsSettings { address, key, display_id,
                              display_name: args.display_name,
                              proxy: args.proxy }
    } else if let Ok(from_json) = config::CmsSettings::from_file(&cmscfg) {
        from_json
    } else {
        anyhow::bail!("cms.json not found or invalid, run with the --host and --key \
                       options to reconfigure");
    };

    cms.to_file(&cmscfg).context("writing new CMS config")?;

    // create the backend handler and required channels
    let (togui_tx, togui_rx) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);
    let (fromgui_tx, fromgui_rx) = crossbeam_channel::bounded(1);

    let handler = collect::Handler::new(cms, args.clear, &args.envdir, args.no_verify,
                                        args.allow_offline, togui_tx, fromgui_rx)
        .context("creating backend handler")?;
    let settings = handler.player_settings();

    // apply setting to inhibit screensaver
    if settings.prevent_sleep {
        if let Err(e) = util::inhibit_screensaver() {
            log::warn!("could not inhibit screensaver: {:#}", e);
        }
    }

    // create the interval webserver on the requested port
    let webserver = server::Server::new(args.envdir.join("res"),
                                        settings.embedded_server_port)
        .context("creating internal HTTP server")?;
    webserver.start_pool();

    #[cfg(feature = "gui")]
    {
        std::thread::spawn(|| handler.run());
        gui::run(settings, args.inspect, togui_rx, fromgui_tx)
    }
    #[cfg(not(feature = "gui"))]
    {
        let _unused = (togui_rx, fromgui_tx);
        handler.run()
    }
}
