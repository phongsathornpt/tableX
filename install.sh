#!/bin/sh
set -eu

# Set TABLEX_REPO when installing from a fork or a different release repository.
repository=${TABLEX_REPO:-phongsathornpt/tableX}
case "$repository" in
  */*) ;;
  *) echo "TABLEX_REPO must be in OWNER/REPOSITORY form" >&2; exit 1 ;;
esac
case "$repository" in
  *[!A-Za-z0-9_./-]*) echo "Invalid TABLEX_REPO value" >&2; exit 1 ;;
esac

os=$(uname -s)
machine=$(uname -m)
case "$os:$machine" in
  Darwin:arm64|Darwin:aarch64) asset=tablex-macos-aarch64.tar.gz; platform=macos ;;
  Darwin:x86_64) asset=tablex-macos-x86_64.tar.gz; platform=macos ;;
  Linux:x86_64|Linux:amd64) asset=tablex-linux-x86_64.tar.gz; platform=linux ;;
  *) echo "tableX installer does not support $os ($machine) yet" >&2; exit 1 ;;
esac

if ! command -v curl >/dev/null 2>&1; then
  echo "curl is required to install tableX" >&2
  exit 1
fi

temporary_dir=$(mktemp -d "${TMPDIR:-/tmp}/tablex-install.XXXXXX")
trap 'rm -rf "$temporary_dir"' EXIT HUP INT TERM
release_url="https://github.com/$repository/releases/latest/download"

curl --fail --location --silent --show-error "$release_url/$asset" -o "$temporary_dir/$asset"
curl --fail --location --silent --show-error "$release_url/SHA256SUMS" -o "$temporary_dir/SHA256SUMS"

expected_hash=$(awk -v name="$asset" '$2 == name { print $1 }' "$temporary_dir/SHA256SUMS")
if [ -z "$expected_hash" ]; then
  echo "No checksum found for $asset in the release" >&2
  exit 1
fi
if command -v sha256sum >/dev/null 2>&1; then
  actual_hash=$(sha256sum "$temporary_dir/$asset" | awk '{ print $1 }')
elif command -v shasum >/dev/null 2>&1; then
  actual_hash=$(shasum -a 256 "$temporary_dir/$asset" | awk '{ print $1 }')
else
  echo "sha256sum or shasum is required to verify the download" >&2
  exit 1
fi
if [ "$expected_hash" != "$actual_hash" ]; then
  echo "Checksum verification failed for $asset" >&2
  exit 1
fi

mkdir "$temporary_dir/extracted"
tar -xzf "$temporary_dir/$asset" -C "$temporary_dir/extracted"

if [ "$platform" = macos ]; then
  app_path=$(find "$temporary_dir/extracted" -maxdepth 2 -type d -name '*.app' -print -quit)
  if [ -z "$app_path" ]; then
    echo "The release archive does not contain a macOS app bundle" >&2
    exit 1
  fi
  applications_dir="$HOME/Applications"
  mkdir -p "$applications_dir"
  app_name=$(basename "$app_path")
  ditto "$app_path" "$applications_dir/$app_name"
  printf 'Installed %s to %s\n' "$app_name" "$applications_dir"
else
  binary_path="$temporary_dir/extracted/tableX"
  if [ ! -f "$binary_path" ]; then
    echo "The release archive does not contain the tableX executable" >&2
    exit 1
  fi
  bin_dir="$HOME/.local/bin"
  applications_dir="$HOME/.local/share/applications"
  mkdir -p "$bin_dir" "$applications_dir"
  install -m 0755 "$binary_path" "$bin_dir/tableX"
  cat > "$applications_dir/tablex.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=tableX
Comment=PostgreSQL database management
Exec=$bin_dir/tableX
Terminal=false
Categories=Development;Database;
EOF
  printf 'Installed tableX to %s/tableX\n' "$bin_dir"
  printf 'Make sure %s is in your PATH to launch it from a terminal.\n' "$bin_dir"
fi
