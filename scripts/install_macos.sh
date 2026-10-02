#!/bin/bash
# Unix entry point: native macOS bundle or per-user Linux payload. Windows uses install.ps1.
# Bash 3.2 compatible. No sudo, downloads, quarantine removal, or login-item changes.
set -euo pipefail
umask 077

fail() { printf 'Maris install: %s\n' "$*" >&2; exit 1; }
usage() {
    cat <<'HELP'
Usage: bash install.sh [--from PATH | --build] [options]
  --from PATH        Local Maris.app (beside this script, or dist/Maris.app)
  --build            Build the current source with Cargo.lock, then package it
  --prefix PATH      Application directory (default: $HOME/Applications)
  --allow-unsigned   Explicitly accept a local development build; not a release
  --yes             Confirm installation without an interactive prompt
  --dry-run         Validate and print the plan; do not create or change files
  --help            Show this help

The previous Maris.app is retained in a uniquely named backup on upgrade.
Preferences are never removed. Running installed binaries are never killed.
Signed installations require strict codesign verification and Gatekeeper approval.
This script never launches Maris; launch it yourself to activate system audio.
HELP
}
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
[ "$(uname -s)" = Darwin ] || fail 'This local installer requires macOS; use the root install.sh entry point'
SOURCE="$ROOT/dist/Maris.app"
[ ! -d "$ROOT/Maris.app" ] || SOURCE="$ROOT/Maris.app"
PREFIX="${HOME:?HOME is required}/Applications"
ALLOW_UNSIGNED=0
YES=0
DRY=0
BUILD=0
FROM=0
while [ "$#" -gt 0 ]; do
    case "$1" in
        --from|--prefix)
            [ "$#" -ge 2 ] && [ -n "$2" ] || fail "$1 needs a value"
            case "$2" in --*) fail "$1 needs a path, not an option" ;; esac
            if [ "$1" = --from ]; then SOURCE=$2; FROM=1; else PREFIX=$2; fi
            shift 2 ;;
        --build) BUILD=1; shift ;;
        --allow-unsigned) ALLOW_UNSIGNED=1; shift ;;
        --yes) YES=1; shift ;;
        --dry-run) DRY=1; shift ;;
        --help|-h) usage; exit 0 ;;
        *) fail "Unknown option: $1" ;;
    esac
done
[ "$FROM:$BUILD" != 1:1 ] || fail '--from and --build are mutually exclusive'
[ "$BUILD" -eq 0 ] || SOURCE="$ROOT/dist/Maris.app"
[ "$(id -u)" -ne 0 ] || fail 'Run as your normal account, without sudo'
VERSION=$(/usr/bin/sw_vers -productVersion)
MAJOR=${VERSION%%.*}
REST=${VERSION#*.}
MINOR=${REST%%.*}
case "$MAJOR:$MINOR" in *[!0-9:]*) fail 'Could not determine the macOS version' ;; esac
[ "$MAJOR" -gt 14 ] || { [ "$MAJOR" -eq 14 ] && [ "$MINOR" -ge 2 ]; } || fail 'macOS 14.2 or later is required'

# Reject ambiguous paths and existing symlink components before resolving them.
safe_path() {
    local path=$1 part
    case "$path" in /*) ;; *) fail 'Use absolute paths for --from and --prefix' ;; esac
    case "$path" in *$'\n'*|*$'\r'*|*/../*|*/./*|*/..|*/.|//*) fail 'Ambiguous path rejected' ;; esac
    part=$path
    while [ "$part" != / ]; do
        [ ! -L "$part" ] || fail "Symlink path component rejected: $part"
        part=$(dirname -- "$part")
    done
}
# Relative source paths are convenient; resolution still rejects symlink components.
case "$SOURCE" in /*) ;; *) SOURCE="$PWD/$SOURCE" ;; esac
case "$PREFIX" in /*) ;; *) PREFIX="$PWD/$PREFIX" ;; esac
SOURCE=${SOURCE%/}
PREFIX=${PREFIX%/}
[ -n "$PREFIX" ] && [ "$PREFIX" != / ] || fail 'Refusing a root destination'
safe_path "$SOURCE"
safe_path "$PREFIX"
DEST="$PREFIX/Maris.app"
[ "$SOURCE" != "$DEST" ] || fail 'Source and destination must differ'

if [ "$BUILD" -eq 1 ]; then
    [ -f "$ROOT/Cargo.toml" ] && [ -f "$ROOT/Cargo.lock" ] || fail '--build requires the source checkout'
    if [ "$DRY" -eq 1 ]; then
        printf 'Dry run: cargo build --release --locked; package; validate; install to %s\n' "$DEST"
        exit 0
    fi
    command -v cargo >/dev/null 2>&1 || fail 'Install Rust 1.90+ and Xcode command-line tools before building'
    if [ -e "$SOURCE" ]; then
        [ -d "$SOURCE" ] || fail 'Existing build output is not an application directory'
        [ -z "$(find "$SOURCE" ! -type d ! -type f -print -quit)" ] || fail 'Existing build output contains linked or special files'
    fi
    (cd "$ROOT" && cargo build --release --locked --target-dir "$ROOT/target" && ./target/release/maris --json package)
fi

plist() { /usr/libexec/PlistBuddy -c "Print :$2" "$1/Contents/Info.plist" 2>/dev/null; }
validate() {
    local app=$1 special arch
    [ -d "$app" ] && [ ! -L "$app" ] || fail "Missing local application bundle: $app"
    special=$(find "$app" ! -type d ! -type f -print -quit)
    [ -z "$special" ] || fail 'Bundle contains a symlink or special file'
    [ "$(plist "$app" CFBundleIdentifier)" = audio.maris.app ] || fail 'Unexpected bundle identifier'
    [ "$(plist "$app" CFBundleExecutable)" = maris ] || fail 'Unexpected bundle executable'
    [ "$(plist "$app" CFBundlePackageType)" = APPL ] || fail 'Not an application bundle'
    [ -x "$app/Contents/MacOS/maris" ] || fail 'Bundle executable is missing or not executable'
    arch=$(uname -m)
    if [ "$arch" = x86_64 ] && [ "$(/usr/sbin/sysctl -in sysctl.proc_translated 2>/dev/null || true)" = 1 ]; then arch=arm64; fi
    /usr/bin/lipo -verify_arch "$arch" "$app/Contents/MacOS/maris" >/dev/null 2>&1 || fail "Bundle does not contain a native $arch executable"
    if [ "$ALLOW_UNSIGNED" -eq 0 ]; then
        /usr/bin/codesign --verify --deep --strict "$app" >/dev/null 2>&1 || fail 'Signature validation failed; use --allow-unsigned only for a trusted local development build'
        /usr/sbin/spctl --assess --type execute "$app" >/dev/null 2>&1 || fail 'Gatekeeper did not approve this bundle; no security settings were changed'
    fi
}
validate "$SOURCE"
if [ "$ALLOW_UNSIGNED" -eq 1 ]; then
    printf '%s\n' 'WARNING: explicitly accepting an unverified development build. Not notarization or public-release approval.' >&2
fi

check_destination() {
    local parent=$PREFIX code
    safe_path "$PREFIX"
    while [ ! -e "$parent" ]; do parent=$(dirname -- "$parent"); done
    [ -d "$parent" ] && [ -w "$parent" ] || fail 'Destination parent is not a writable directory'
    [ "$(/usr/bin/stat -f %u "$parent")" = "$(id -u)" ] || fail 'Destination must be inside a directory owned by your account'
    if [ -e "$DEST" ] || [ -L "$DEST" ]; then
        [ ! -L "$DEST" ] && [ -d "$DEST" ] || fail 'Refusing to replace an unrelated destination'
        [ "$(plist "$DEST" CFBundleIdentifier)" = audio.maris.app ] || fail 'Destination is not an existing Maris bundle'
        [ "$(/usr/bin/stat -f %u "$DEST")" = "$(id -u)" ] || fail 'Existing application is not owned by your account'
        # lsof exit 1 means no open file, not an invitation to terminate a process.
        code=0
        /usr/sbin/lsof -t "$DEST/Contents/MacOS/maris" >/dev/null 2>&1 || code=$?
        [ "$code" -eq 1 ] || fail 'Installed Maris is running, or its open-file state is unknown; quit it before upgrading'
    fi
}
check_destination
printf 'Source: %s\nInstall: %s\nVersion: %s\n' "$SOURCE" "$DEST" "$(plist "$SOURCE" CFBundleShortVersionString)"
[ "$DRY" -eq 0 ] || { printf '%s\n' 'Dry run complete; no files changed.'; exit 0; }
if [ "$YES" -eq 0 ]; then
    [ -t 0 ] || fail 'Noninteractive installation requires --yes'
    printf 'Install without starting audio? [y/N] '
    read -r answer
    case "$answer" in y|Y|yes|YES) ;; *) printf '%s\n' 'Cancelled; no files changed.'; exit 0 ;; esac
fi

mkdir -p -- "$PREFIX"
LOCK="$PREFIX/.maris-install.lock"
mkdir -- "$LOCK" 2>/dev/null || fail 'Another installation may be active; inspect the existing install lock before retrying'
STAGE=''
BACKUP=''
COMMITTED=0
cleanup() {
    local code=$?
    trap - EXIT HUP INT TERM
    if [ "$COMMITTED" -eq 0 ] && [ -n "$BACKUP" ] && [ -d "$BACKUP/Maris.app" ] && [ ! -e "$DEST" ]; then
        mv -- "$BACKUP/Maris.app" "$DEST" || printf 'Recovery required: previous bundle remains at %s\n' "$BACKUP/Maris.app" >&2
    fi
    # Only this invocation's mktemp staging directory is eligible for removal.
    [ -z "$STAGE" ] || rm -rf -- "$STAGE"
    [ -z "$BACKUP" ] || rmdir -- "$BACKUP" 2>/dev/null || true
    rmdir -- "$LOCK" 2>/dev/null || true
    exit "$code"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
check_destination
STAGE=$(mktemp -d "$PREFIX/.maris-stage.XXXXXX")
/usr/bin/ditto "$SOURCE" "$STAGE/Maris.app"
validate "$STAGE/Maris.app"
# Detect changes during the copy without ever executing the supplied binary.
/usr/bin/cmp -s "$SOURCE/Contents/MacOS/maris" "$STAGE/Maris.app/Contents/MacOS/maris" || fail 'Source executable changed during copy'
/usr/bin/cmp -s "$SOURCE/Contents/Info.plist" "$STAGE/Maris.app/Contents/Info.plist" || fail 'Source metadata changed during copy'
check_destination
if [ -d "$DEST" ]; then
    BACKUP=$(mktemp -d "$PREFIX/.maris-backup.XXXXXX")
    mv -- "$DEST" "$BACKUP/Maris.app"
fi
mv -- "$STAGE/Maris.app" "$DEST"
COMMITTED=1
printf 'Installed: %s\n' "$DEST"
[ -z "$BACKUP" ] || printf 'Previous bundle retained: %s/Maris.app\n' "$BACKUP"
printf '%s\n' 'Preferences preserved. No audio was started. Open the application yourself when ready.'
