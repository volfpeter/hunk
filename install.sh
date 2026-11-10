#!/usr/bin/env bash
# Builds this clone and installs hunk for the current user: the `hunk` command and a launcher
# entry. `--uninstall` removes everything it installs. Linux and macOS.
set -euo pipefail

name=hunk
app_id=io.github.volfpeter.hunk
here=$(cd "$(dirname "$0")" && pwd)
bin_dir=$HOME/.local/bin

case $(uname) in
    Linux)
        data=${XDG_DATA_HOME:-$HOME/.local/share}
        entry=$data/applications/$app_id.desktop
        icon=$data/icons/hicolor/scalable/apps/$app_id.svg
        installed=("$bin_dir/$name" "$entry" "$icon")
        ;;
    Darwin)
        bundle=$HOME/Applications/$name.app
        installed=("$bin_dir/$name" "$bundle")
        ;;
    *)
        echo "$name: only Linux and macOS are supported" >&2
        exit 1
        ;;
esac

# Replaces by renaming, so a running copy of the file isn't written to.
place() {
    local mode=$1 from=$2 to=$3
    mkdir -p "$(dirname "$to")"
    install -m "$mode" "$from" "$to.new"
    mv -f "$to.new" "$to"
}

uninstall() {
    rm -rf "${installed[@]}"
    echo "Removed ${installed[*]}"
}

install_linux() {
    place 755 "$binary" "$bin_dir/$name"
    place 644 "$here/assets/icon.svg" "$icon"
    local desktop
    desktop=$(mktemp)
    cat > "$desktop" <<EOF
[Desktop Entry]
Type=Application
Name=$name
Comment=Review Git changes
Exec=$bin_dir/$name
Icon=$app_id
Terminal=false
Categories=Development;RevisionControl;
EOF
    place 644 "$desktop" "$entry"
    rm -f "$desktop"
}

# Untested: see step 4i of docs/plans/01-kickstart.md.
install_macos() {
    local contents=$bundle/Contents icon_key=""
    place 755 "$binary" "$contents/MacOS/$name"
    if [[ -f $here/assets/icon.icns ]]; then
        place 644 "$here/assets/icon.icns" "$contents/Resources/$name.icns"
        icon_key="<key>CFBundleIconFile</key><string>$name</string>"
    fi
    cat > "$contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>$name</string>
    <key>CFBundleIdentifier</key><string>$app_id</string>
    <key>CFBundleExecutable</key><string>$name</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>NSHighResolutionCapable</key><true/>
    $icon_key
</dict>
</plist>
EOF
    codesign --force --sign - "$bundle"
    mkdir -p "$bin_dir"
    ln -sf "$contents/MacOS/$name" "$bin_dir/$name"
}

case ${1:-} in
    --uninstall)
        uninstall
        exit
        ;;
    "") ;;
    *)
        echo "usage: $0 [--uninstall]" >&2
        exit 2
        ;;
esac

cargo build --release --manifest-path "$here/Cargo.toml"
binary=$(cargo metadata --format-version 1 --no-deps --manifest-path "$here/Cargo.toml" \
    | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')/release/$name

case $(uname) in
    Linux) install_linux ;;
    Darwin) install_macos ;;
esac
echo "Installed ${installed[*]}"

case ":$PATH:" in
    *":$bin_dir:"*) ;;
    *) echo "Add $bin_dir to PATH to run $name from a terminal." ;;
esac
