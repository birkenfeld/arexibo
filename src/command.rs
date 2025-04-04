// Xibo player Rust implementation, (c) 2022-2024 Georg Brandl.
// Licensed under the GNU AGPL, version 3 or later.

//! Player command handling.

use std::{collections::HashMap, io::{Read, Write}, time::Duration, process};
use anyhow::{bail, Context, Result};
use itertools::Itertools;
use serde::{Serialize, Deserialize};

const TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Command {
    pub command: String,
    pub validate: String,
    pub alerts: String,
}

impl Command {
    pub fn run(&self) -> Result<bool> {
        log::info!("running command {:?}", self.command);
        let result = if self.command == "SoftRestart" {
            std::process::exit(0);
        } else if self.command.starts_with("http|") {
            self.run_http()?
        } else if self.command.starts_with("rs232|") {
            self.run_rs232()?
        } else {
            self.run_shell()?
        };

        if !self.validate.is_empty() {
            log::info!("validating command result {result:?} against {:?}", self.validate);
            let rx = regex::Regex::new(&self.validate).context("invalid validation Regex")?;
            Ok(rx.is_match(&result))
        } else {
            Ok(true)
        }
    }

    fn run_shell(&self) -> Result<String> {
        let cmd = process::Command::new("/bin/sh")
            .arg("-c")
            .arg(&self.command)
            .output()
            .context("running shell command")?;
        Ok(String::from_utf8_lossy(&cmd.stdout).into())
    }

    fn run_http(&self) -> Result<String> {
        let (_, url, content_type, opts) = self.command.split("|").collect_tuple()
            .context("invalid HTTP command string")?;
        let opts: HttpOpts = serde_json::from_str(opts)
            .context("invalid HTTP option dictionary")?;

        let mut builder = ureq::http::Request::builder()
            .method(opts.method.as_str())
            .uri(url)
            .header("Content-Type", content_type);
        for (k, v) in opts.headers {
            builder = builder.header(k, v);
        }
        let request = builder.body(opts.body).context("invalid HTTP request")?;
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(TIMEOUT))
            .build().into();
        let result = agent.run(request).context("making HTTP request")?;

        // strange, but the status code is the only thing used for validation
        Ok(result.status().as_str().into())
    }

    fn run_rs232(&self) -> Result<String> {
        let (_, params, msg) = self.command.split("|").collect_tuple()
            .context("invalid RS232 command string")?;
        let (dev, baud, bits, parity, stop, handshake, hex) =
            params.split(",").collect_tuple().context("invalid RS232 param string")?;
        let baud = baud.parse().context("invalid RS232 baud rate")?;
        let bits = match bits {
            "5" => serialport::DataBits::Five,
            "6" => serialport::DataBits::Six,
            "7" => serialport::DataBits::Seven,
            "8" => serialport::DataBits::Eight,
            _ => bail!("invalid RS232 data bits")
        };
        let parity = match parity {
            "None" => serialport::Parity::None,
            "Odd" => serialport::Parity::Odd,
            "Even" => serialport::Parity::Even,
            _ => bail!("invalid RS232 parity")
        };
        let stop = match stop {
            "None" => serialport::StopBits::One,
            "One" => serialport::StopBits::One,
            "OnePointFive" => serialport::StopBits::Two,
            "Two" => serialport::StopBits::Two,
            _ => bail!("invalid RS232 stop bits")
        };
        let handshake = match handshake {
            "None" => serialport::FlowControl::None,
            "XOnXOff" => serialport::FlowControl::Software,
            "RequestToSend" => serialport::FlowControl::Hardware,
            _ => bail!("invalid RS232 handshake")
        };

        let mut port = serialport::new(dev, baud)
            .data_bits(bits)
            .stop_bits(stop)
            .parity(parity)
            .flow_control(handshake)
            .timeout(TIMEOUT)
            .open_native()?;

        let data = if hex == "1" {
            let msg = msg.chars().filter(|&c| !c.is_whitespace()).collect::<String>();
            hex::decode(&msg).context("invalid RS232 hex message")?
        } else {
            msg.as_bytes().to_vec()
        };

        port.write(&data).context("writing RS232 message")?;

        if self.validate.is_empty() {
            // don't try to read if it's not used
            return Ok(String::new());
        }

        let mut buf = [0];
        let mut result = String::new();
        loop {
            port.read_exact(&mut buf).context("reading RS232 result")?;
            if buf[0] == b'\n' {
                break;
            }
            result.push(buf[0] as char);
        }
        Ok(result)
    }
}

#[derive(Deserialize)]
struct HttpOpts {
    method: String,
    headers: HashMap<String, String>,
    body: String
}
