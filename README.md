# Arexibo

Arexibo is an alternate Digital Signage Player for [Xibo](https://xibo.org.uk),
implemented in Rust with the GTK GUI components, for Linux platforms.

It is currently quite incomplete.


## Installation

Install `pkgconfig`, the development packages for `dbus`, `webkit2gtk` and
`zeromq`, as well as [the Rust toolchain](https://www.rust-lang.org/) >= 1.54.

Then you can call this in the checkout:

```
$ cargo install --path .
```


## Usage

Create a new directory where Arexibo can store configuration and media files.
Then, at first start, use the following command line to configure the player:

```
arexibo --host <https://my.cms/> --key <key> <dir>
```

Arexibo will cache the configuration in the directory, so that in the future
you only need to start with

```
arexibo <dir>
```

Note: the GUI window will only show up once the display is authorized!
