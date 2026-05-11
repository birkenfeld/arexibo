# Arexibo

<p align="center">
  <img src="https://github.com/birkenfeld/arexibo/blob/master/assets/logo.png?raw=true" alt="Logo"/>
</p>

Arexibo is an unofficial alternate Digital Signage Player for [Xibo](https://xibo.org.uk),
implemented mostly in Rust but making use of Qt GUI components, for Linux platforms.

It is currently still incomplete.  Don't expect more complex features to work
unless tested.


## Installation

Binary builds are published on every tagged release:

* **RPM** (Fedora/RHEL) and **DEB** (Debian/Ubuntu) packages, produced by
  the `rpm.yml` and `deb.yml` GitHub Actions workflows on tag pushes.
* **SRPMs** are published alongside the GitHub Releases assets so you can
  rebuild from source without running the full Cargo toolchain directly.

Install from the xiboplayer package repos listed at
<https://xiboplayer.github.io/>.

To build from source, you need:

* The [Rust toolchain](https://www.rust-lang.org/), version >= 1.75.  Refer to
  https://rustup.rs/ for the easiest way to install, if the Linux distribution
  provided package is too old.

* CMake and a C++ compiler.

* Qt 6 with the QtWebEngine component and its development headers.

* Development headers for `dbus` (>= 1.6), `zeromq` (>= 4.1)
  as well as `pkg-config`.

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

Builds have been tested with the available dependency library versions on Fedora
41, RHEL 9 with EPEL and Ubuntu 24.04.  Note that in order to play some media
like mp4 videos, you will require a `ffmpeg` package that includes some codecs
that RHEL/Fedora don't include in their packages, e.g. from rpmfusion.org.

For RHEL derived distributions, install `cmake gcc-c++ cargo dbus-devel
zeromq-devel qt6-qtwebengine-devel`.  For Debian derived, install `cmake g++
cargo libdbus-1-dev libzmq3-dev qt6-webengine-dev`.


## SRPMs (source RPMs)

The `rpm/arexibo.spec` file drives RPM packaging for Fedora and RHEL-derived
distributions. Two ways to use it:

### Option A — rebuild from a published SRPM

When a release ships, the CI workflow runs `rpmbuild -ba` on
`rpm/arexibo.spec`, which produces both a binary RPM and a **source RPM
(SRPM)**. Both are published to the xiboplayer R2 bucket under
`dl.xiboplayer.org`. The layout mirrors the rpmbuild directory convention:

```
# Binary RPM
https://dl.xiboplayer.org/rpm/fedora/<fedora_version>/<arch>/arexibo-<version>-<release>.fc<fedora_version>.<arch>.rpm

# SRPM (note the uppercase SRPMS/ directory)
https://dl.xiboplayer.org/rpm/fedora/<fedora_version>/SRPMS/arexibo-<version>-<release>.fc<fedora_version>.src.rpm
```

For example, the current Fedora 43 x86_64 binary and matching SRPM:

```bash
# Binary install
curl -O https://dl.xiboplayer.org/rpm/fedora/43/x86_64/arexibo-0.3.3-3.fc43.x86_64.rpm
sudo dnf install ./arexibo-0.3.3-3.fc43.x86_64.rpm

# SRPM download
curl -O https://dl.xiboplayer.org/rpm/fedora/43/SRPMS/arexibo-0.3.3-3.fc43.src.rpm
```

Old releases remain in the bucket — the SRPMS/ directory is append-only, so
any historical release you want to rebuild or audit is reachable at its
original URL.

With the SRPM in hand, rebuild a binary RPM for your distribution and
architecture with either `rpmbuild --rebuild` (native toolchain) or `mock`
(clean chroot — recommended for reproducible builds):

```bash
# Native rebuild
rpmbuild --rebuild arexibo-0.3.3-3.fc43.src.rpm

# Clean-chroot rebuild via mock (reproducible; requires mock installed)
mock -r fedora-43-x86_64 arexibo-0.3.3-3.fc43.src.rpm

# Rebuild for a different Fedora release
mock -r fedora-44-x86_64 arexibo-0.3.3-3.fc43.src.rpm

# Rebuild for aarch64 (if your host supports it)
mock -r fedora-43-aarch64 arexibo-0.3.3-3.fc43.src.rpm
```

The resulting binary RPM lands in `~/rpmbuild/RPMS/<arch>/` (native) or
`/var/lib/mock/<target>/result/` (mock).

### Option B — build an SRPM locally from a git checkout

For verifying the spec file, testing packaging changes, or producing an SRPM
for a version that hasn't been released yet:

```bash
# 1. Clone the repo and check out the version you want
git clone https://github.com/xiboplayer/arexibo.git
cd arexibo
git checkout v0.3.3           # or any tag / commit

# 2. Create the rpmbuild directory tree
mkdir -p ~/rpmbuild/{BUILD,RPMS,SOURCES,SPECS,SRPMS}

# 3. Copy the spec file into position
cp rpm/arexibo.spec ~/rpmbuild/SPECS/

# 4. Create the source tarball with the expected layout
#    (rpmbuild -bs expects ${name}-${version}/ inside the tarball)
VERSION=$(awk '/^Version:/ {print $2}' rpm/arexibo.spec)
git archive --prefix=arexibo-${VERSION}/ HEAD \
  | gzip > ~/rpmbuild/SOURCES/arexibo-${VERSION}.tar.gz

# 5. Build the SRPM (just the source RPM — no compile step)
rpmbuild -bs ~/rpmbuild/SPECS/arexibo.spec

# 6. The SRPM lands at:
ls ~/rpmbuild/SRPMS/arexibo-${VERSION}-*.src.rpm
```

Useful for auditing the packaging, testing a spec change before pushing, or
producing an SRPM for a development branch.

### Verifying SRPM signatures

SRPMs published to `dl.xiboplayer.org` are GPG-signed with the xiboplayer
signing key. To verify a downloaded SRPM:

```bash
# Import the xiboplayer signing key (one-time)
sudo rpm --import https://dl.xiboplayer.org/rpm/RPM-GPG-KEY

# Verify the SRPM signature
rpm --checksig arexibo-0.3.3-3.fc43.src.rpm
# Expected output: arexibo-0.3.3-3.fc43.src.rpm: digests signatures OK
```

### Why SRPMs are worth keeping

- **Reproducible rebuilds** for distros or architectures that aren't in the
  CI matrix (e.g. RHEL 10, OpenSUSE, ppc64le).
- **Packaging audits** — the SRPM bundles the exact source tarball, spec
  file, and patches used to produce the binary RPM, so a downstream
  packager or security auditor can verify the build inputs without
  trusting the binary.
- **Copr / OBS forks** — the SRPM is the canonical input format for Fedora
  Copr and OpenSUSE Build Service, so a community member running their own
  build pipeline can do it with one file.
- **Air-gapped deployments** — SRPMs are self-contained, so a signage
  deployment without internet access can ship with an SRPM and rebuild
  on-site against the exact same source and spec the upstream release used.


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
