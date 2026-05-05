%global debug_package %{nil}

Name:           arexibo
Version:        0.3.3
Release:        3%{?dist}
Summary:        Rust-based digital signage player for Xibo CMS

License:        AGPLv3+
URL:            https://github.com/xibo-players/arexibo
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  rust >= 1.75
BuildRequires:  cargo
BuildRequires:  cmake
BuildRequires:  gcc-c++
BuildRequires:  dbus-devel >= 1.6
BuildRequires:  zeromq-devel >= 4.1
BuildRequires:  qt6-qtwebengine-devel

Requires:       qt6-qtwebengine
Requires:       dbus
Requires:       zeromq

%description
Arexibo is a Rust-based digital signage player compatible with Xibo CMS.
It provides a lightweight alternative to the official Xibo player,
designed for kiosk and digital signage deployments on Linux.

%prep
%autosetup -n %{name}-%{version}

%build
export CARGO_NET_GIT_FETCH_WITH_CLI=true
cargo build --release

%install
install -Dm755 target/release/arexibo %{buildroot}%{_bindir}/arexibo
install -Dm644 arexibo.desktop %{buildroot}%{_datadir}/applications/arexibo.desktop
install -Dm644 assets/arexibo-256.png %{buildroot}%{_datadir}/icons/hicolor/256x256/apps/arexibo.png
install -Dm644 assets/logo.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/arexibo.svg
install -Dm644 arexibo.service %{buildroot}%{_userunitdir}/arexibo.service

%files
%license LICENSE
%doc README.md CHANGELOG.md
%{_bindir}/arexibo
%{_userunitdir}/arexibo.service
%{_datadir}/applications/arexibo.desktop
%{_datadir}/icons/hicolor/256x256/apps/arexibo.png
%{_datadir}/icons/hicolor/scalable/apps/arexibo.svg

%changelog
* Wed Apr 02 2026 Pau Aliagas <linuxnow@gmail.com> - 0.3.3-3
- Add arexibo.service systemd user unit
- Add unit tests for config and util modules

* Wed Mar 12 2026 Pau Aliagas <pau@linuxnow.com> - 0.3.1-4
- Install desktop entry and icon in RPM and DEB packages
- Merge upstream dependency updates

* Sat Mar 29 2026 Pau Aliagas <linuxnow@gmail.com> - 0.3.3-2
- Rebuild for Fedora 44

* Sun Mar 23 2026 Pau Aliagas <linuxnow@gmail.com> - 0.3.3-1
- Sync with upstream v0.3.3

* Fri Feb 28 2026 Pau Aliagas <pau@linuxnow.com> - 0.3.1-2
- Install desktop entry and icon for proper desktop integration (closes #6)

* Tue Feb 18 2026 Pau Aliagas <pau@linuxnow.com> - 0.3.1-1
- Clean rebuild: PDF support, NotAuthorized exit code, dependency updates
- Thin workflow callers for RPM, DEB and image builds
