# Arexibo

<p align="center">
  <img src="https://github.com/birkenfeld/arexibo/blob/master/assets/logo.png?raw=true" alt="Logo"/>
</p>

Arexibo is an alternate Digital Signage Player for [Xibo](https://xibo.org.uk),
implemented in Rust with the GTK GUI components, for Linux platforms.

It is currently quite incomplete.  Don't expect any particular feature to work
unless tested.


## Installation

Currently, no binary builds are provided.

To build from source, you need:

* The [Rust toolchain](https://www.rust-lang.org/), version >= 1.54.  Refer to
  https://rustup.rs/ for the easiest way to install, if the Linux distribution
  provided package is too old.

* Development headers for `dbus` (>= 1.6), `webkit2gtk` (>= 2.22), and `zeromq`
  (>= 4.1), as well as a normal build system including `pkg-config`.

To build, run:

```
$ cargo build --release
```

The binary is placed in `target/release/arexibo` and can be run from there.

To install, run:

```
$ cargo install --path . --root /usr
```

The will install the binary to `/usr/bin/arexibo`.  It requires no other files
at runtime, except for the system libraries it is linked against.

Builds have been tested with the available dependency library versions on Debian
bullseye, Ubuntu focal and RHEL 8 with EPEL.  Debian/Ubuntu don't provide a new
enough Rust compiler though.

Since webkit2gtk uses gstreamer to play media, you might have to install
additional plugins such as `gst-libav`; they are not required at build time.


## Usage

Create a new directory where Arexibo can store configuration and media files.
Then, at first start, use the following command line to configure the player:

```
arexibo --host <https://my.cms/> --key <key> <dir>
```

Further configuration options are `--display-id` (which is normally
auto-generated from machine characteristics) and `--proxy` (if needed).

Arexibo will cache the configuration in the directory, so that in the future you
only need to start with

```
arexibo <dir>
```

Log messages are printed to stdout.  The GUI window will only show up once the
display is authorized.


## Standalone setup with X server

The following example systemd service file shows how to to start an X server
with Arexibo and no DPMS/screensaver:

```
[Unit]
Description=Start X with Arexibo player
After=network-online.target
Requires=network-online.target

[Service]
Type=simple
ExecStart=/usr/bin/xinit /usr/bin/arexibo /home/xibo/env -- :0 vt2 -s 0 -v -dpms
User=xibo
Restart=always
RestartSec=60
Environment=NO_AT_BRIDGE=1

[Install]
WantedBy=multi-user.target
```
