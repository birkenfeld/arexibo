# Arexibo

Arexibo is an alternate Digital Signage Player for [Xibo](https://xibo.org.uk),
implemented in Rust with the GTK GUI components.

It is currently very incomplete.


## Installation

Install `pkgconfig`, the development packages for `webkit2gtk` and `zeromq`, as
well as [the Rust toolchain](https://www.rust-lang.org/) >= 1.56.

Then you can call this in the checkout:

```
$ cargo install --path .
```


## Usage

Create a new directory where Arexibo can store configuration and media files.
Then, at first start, use the following command line to configure the player:

```
arexibo --host <https://my.cms/> --cms-key <key> --display-id <id> <dir>
```

Arexibo will cache the configuration in the directory, so that in the future
you only need to start with

```
arexibo <dir>
```

Note: the GUI will only show up once the display is authorized!
