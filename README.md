# tableX

tableX is a Rust desktop database management application for PostgreSQL. It
provides saved connections, database object browsing, and a SQL workspace with
query results.

## Install

After a tagged release is published, download the macOS `.dmg` or Linux `.deb`
from the [Releases page](https://github.com/phongsathornpt/tableX/releases).
Each release also includes checksummed archives for the one-line installer.

The installer supports Apple Silicon and Intel Macs, plus x86_64 Linux:

```sh
curl -fsSL https://raw.githubusercontent.com/phongsathornpt/tableX/main/install.sh | sh
```

The script checks the downloaded archive against the release's `SHA256SUMS`
before installing. macOS apps are installed to `~/Applications`; Linux installs
the executable under `~/.local/bin` and adds a desktop entry under
`~/.local/share/applications`. On Linux, ensure `~/.local/bin` is on your
`PATH`.

Linux password storage uses the system Secret Service. A compatible keyring
service, such as GNOME Keyring or KDE Wallet, should be available in the
desktop session. The Linux desktop also needs working X11 or Wayland and GPU
drivers.

## Build from source

Install Rust using [rustup](https://rustup.rs/), then run the project checks and
build commands:

```sh
make verify
make build-release
```

On Linux, install the GPUI build dependencies used by the release workflow
before building. Run the app with `cargo run` or use `make run`.

## Releases

Push a version tag matching `Cargo.toml`, for example `v0.0.1`, to run
`.github/workflows/release.yml`. The workflow runs formatting, checking,
Clippy, and unit tests, then packages macOS Apple Silicon, macOS Intel, and
Linux x86_64 builds. It publishes `.dmg`, `.deb`, installer archives, and
`SHA256SUMS` to a GitHub Release.

The current workflow does not sign or notarize the macOS app. Until Apple
Developer ID signing and notarization are added, Gatekeeper may require users
to approve the downloaded app manually. Do not describe those artifacts as
notarized.
