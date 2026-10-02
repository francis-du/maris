#!/bin/bash
# Per-user CLI/TUI installer. Inspect native headers without running the input binary.
# The entry point is ../install.sh. No privilege elevation or audio/service configuration.
set -euo pipefail
umask 077
fail() { printf 'Maris install: %s\n' "$*" >&2; exit 1; }
usage() {
    cat <<'HELP'
Usage: bash install.sh [--from PATH | --build] [options]
  --from PATH        Local Maris payload directory containing bin/maris
  --build            Build this checkout with Cargo.lock before installing
  --prefix PATH      User installation prefix (default: $HOME/.local)
  --sha256 HASH      Expected binary SHA-256 obtained from a trusted source
  --allow-unsigned   Explicitly trust an unverified local development payload
  --yes             Confirm without an interactive prompt
  --dry-run         Validate and print the plan without writing files
  --help            Show this help

Installs PREFIX/lib/maris and an owned PREFIX/bin/maris symlink. PATH is not edited.
Previous installations are retained. No downloads, drivers, services or audio activation.
No GUI bundle or OS publisher signature is required by this CLI distribution contract.
HELP
}
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
SOURCE="$ROOT/Maris"
PREFIX="${HOME:?HOME is required}/.local"
EXPECTED=''
ALLOW=0; YES=0; DRY=0; BUILD=0; FROM=0
while [ "$#" -gt 0 ]; do
    case "$1" in
        --from|--prefix|--sha256)
            [ "$#" -ge 2 ] && [ -n "$2" ] || fail "$1 needs a value"
            case "$2" in --*) fail "$1 needs a value, not another option" ;; esac
            case "$1" in
                --from) SOURCE=$2; FROM=1 ;;
                --prefix) PREFIX=$2 ;;
                --sha256) EXPECTED=$2 ;;
            esac
            shift 2 ;;
        --build) BUILD=1; shift ;;
        --allow-unsigned) ALLOW=1; shift ;;
        --yes) YES=1; shift ;;
        --dry-run) DRY=1; shift ;;
        --help|-h) usage; exit 0 ;;
        *) fail "Unknown option: $1" ;;
    esac
done
case "$(uname -s)" in Darwin) SYSTEM=macos ;; Linux) SYSTEM=linux ;; *) fail 'This CLI helper requires macOS or Linux' ;; esac
[ "$BUILD" -eq 0 ] || fail 'CLI installation requires a precompiled --from payload; use the source build workflow to prepare one'
[ "$(id -u)" -ne 0 ] || fail 'Use a normal user account, not root'
[ "$FROM:$BUILD" != 1:1 ] || fail '--from and --build are mutually exclusive'
if [ -n "$EXPECTED" ]; then
    [[ "$EXPECTED" =~ ^[a-fA-F0-9]{64}$ ]] || fail '--sha256 requires exactly 64 hexadecimal characters'
    EXPECTED=$(printf '%s' "$EXPECTED" | tr 'A-F' 'a-f')
fi
case "$(uname -m)" in
    x86_64) ARCH=x86_64; MACHINE=62
        if [ "$SYSTEM" = macos ] && [ "$(/usr/sbin/sysctl -in sysctl.proc_translated 2>/dev/null || true)" = 1 ]; then ARCH=arm64; fi ;;
    aarch64|arm64) ARCH=arm64; MACHINE=183 ;;
    *) fail 'Supported CLI architectures are x86_64 and arm64' ;;
esac
if command -v sha256sum >/dev/null 2>&1; then SHA=(sha256sum); else SHA=(shasum -a 256); fi
safe_path() {
    local path=$1 part
    case "$path" in /*) ;; *) fail 'Path must be absolute' ;; esac
    case "$path" in *$'\n'*|*$'\r'*|*$'\t'*|*/../*|*/./*|*/..|*/.|//*) fail 'Ambiguous path rejected' ;; esac
    part=$path
    while [ "$part" != / ]; do
        [ ! -L "$part" ] || fail "Symlink component rejected: $part"
        part=$(dirname -- "$part")
    done
}
case "$SOURCE" in /*) ;; *) SOURCE="$PWD/$SOURCE" ;; esac
case "$PREFIX" in /*) ;; *) PREFIX="$PWD/$PREFIX" ;; esac
SOURCE=${SOURCE%/}; PREFIX=${PREFIX%/}
[ -n "$PREFIX" ] && [ "$PREFIX" != / ] || fail 'Root destination rejected'
safe_path "$SOURCE"; safe_path "$PREFIX"
DEST="$PREFIX/lib/maris"
LINK="$PREFIX/bin/maris"
[ "$SOURCE" != "$DEST" ] || fail 'Source and destination must differ'
BUILD_DIR=''; STAGE=''; BACKUP=''; LOCK=''; LINK_CREATED=0; COMMITTED=0
cleanup() {
    local code=$?
    trap - EXIT HUP INT TERM
    if [ "$COMMITTED" -eq 0 ]; then
        if [ -n "$BACKUP" ] && [ -d "$BACKUP/Maris" ] && [ ! -e "$DEST" ]; then
            mv -- "$BACKUP/Maris" "$DEST" || printf 'Recovery required: %s/Maris\n' "$BACKUP" >&2
        fi
        if [ "$LINK_CREATED" -eq 1 ] && [ -L "$LINK" ] && [ "$(readlink -- "$LINK")" = ../lib/maris/bin/maris ]; then
            rm -- "$LINK"
        fi
    fi
    [ -z "$STAGE" ] || rm -rf -- "$STAGE"
    [ -z "$BUILD_DIR" ] || rm -rf -- "$BUILD_DIR"
    [ -z "$BACKUP" ] || rmdir -- "$BACKUP" 2>/dev/null || true
    [ -z "$LOCK" ] || rmdir -- "$LOCK" 2>/dev/null || true
    exit "$code"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
marker() {
    local path=$1
    [ -f "$path/.maris-package" ] && [ ! -L "$path/.maris-package" ] || fail 'Maris package identity is missing'
    [ "$(wc -c < "$path/.maris-package")" -le 128 ] || fail 'Package identity is oversized'
    [ "$(sed -n '1p' "$path/.maris-package")" = maris-package-v2 ] || fail 'Incorrect package identity'
    [ "$(sed -n '2p' "$path/.maris-package")" = "$SYSTEM" ] || fail 'This is not a native CLI package'
    [ "$(sed -n '3p' "$path/.maris-package")" = "$ARCH" ] || fail 'Package architecture does not match this host'
    [[ "$(sed -n '4p' "$path/.maris-package")" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail 'Invalid package version'
    [ "$(sed -n '5p' "$path/.maris-package")" = cli ] || fail 'GUI payloads cannot use the CLI installer'
    [ "$(wc -l < "$path/.maris-package")" -eq 5 ] || fail 'Unexpected package identity fields'
}
validate() {
    local path=$1 digest
    local bytes=()
    [ -d "$path" ] && [ ! -L "$path" ] || fail "Missing local payload: $path"
    [ -z "$(find "$path" ! -type f ! -type d -print -quit)" ] || fail 'Linked or special payload files rejected'
    marker "$path"
    [ -f "$path/bin/maris" ] && [ -x "$path/bin/maris" ] || fail 'Native executable is missing or not executable'
    read -r -a bytes <<< "$(od -An -v -tu1 -N64 "$path/bin/maris" | tr '\n' ' ')"
    [ "${#bytes[@]}" -eq 64 ] || fail 'Truncated ELF executable'
    if [ "$SYSTEM" = macos ]; then
        [ "${bytes[0]}:${bytes[1]}:${bytes[2]}:${bytes[3]}" = 207:250:237:254 ] || fail 'Expected a 64-bit little-endian Mach-O executable'
        CPU=7; [ "$ARCH" != arm64 ] || CPU=12
        [ "${bytes[4]}:${bytes[5]}:${bytes[6]}:${bytes[7]}" = "$CPU:0:0:1" ] || fail 'Mach-O architecture mismatch'
        [ "${bytes[12]}:${bytes[13]}:${bytes[14]}:${bytes[15]}" = 2:0:0:0 ] || fail 'Mach-O is not an executable'
    else
        [ "${bytes[0]}:${bytes[1]}:${bytes[2]}:${bytes[3]}:${bytes[4]}:${bytes[5]}:${bytes[6]}" = 127:69:76:70:2:1:1 ] || fail 'Expected a 64-bit little-endian ELF executable'
        [ "${bytes[18]}:${bytes[19]}" = "$MACHINE:0" ] || fail 'ELF machine does not match the native architecture'
        [ "${bytes[16]}:${bytes[17]}" = 2:0 ] || [ "${bytes[16]}:${bytes[17]}" = 3:0 ] || fail 'ELF file is not an executable image'
    fi
    digest=$("${SHA[@]}" "$path/bin/maris"); digest=${digest%% *}
    if [ -n "$EXPECTED" ]; then
        [ "$digest" = "$EXPECTED" ] || fail 'Binary SHA-256 does not match'
    else
        [ "$ALLOW" -eq 1 ] || fail 'Provide a trusted --sha256 or explicitly acknowledge --allow-unsigned for local development'
    fi
}
validate "$SOURCE"
check_destination() {
    local parent=$PREFIX code=0 evidence=''
    safe_path "$PREFIX"; safe_path "$PREFIX/lib"; safe_path "$PREFIX/bin"; safe_path "$DEST"
    while [ ! -e "$parent" ]; do parent=$(dirname -- "$parent"); done
    [ -d "$parent" ] && [ -w "$parent" ] && [ -O "$parent" ] || fail 'Choose a writable user-owned prefix'
    if [ -e "$LINK" ] || [ -L "$LINK" ]; then
        [ -L "$LINK" ] && [ "$(readlink -- "$LINK")" = ../lib/maris/bin/maris ] || fail 'An unrelated command already occupies PREFIX/bin/maris'
        [ -d "$DEST" ] || fail 'An existing launcher without its Maris installation requires manual inspection'
    fi
    if [ -e "$DEST" ]; then
        [ -d "$DEST" ] && [ -O "$DEST" ] || fail 'Refusing an unrelated or unowned destination'
        marker "$DEST"
        [ -z "$(find "$DEST" ! -type f ! -type d -print -quit)" ] || fail 'Existing installation contains linked or special files'
        if [ "$SYSTEM" = macos ]; then
            command -v lsof >/dev/null 2>&1 || fail 'lsof is required before upgrading'
            evidence=$(lsof -t "$DEST/bin/maris" 2>&1) || code=$?
        else
            command -v fuser >/dev/null 2>&1 || fail 'Install psmisc before upgrading; open-file checks are required'
            evidence=$(fuser "$DEST/bin/maris" 2>&1) || code=$?
        fi
        [ "$code" -eq 1 ] && [ -z "$evidence" ] || fail 'Installed Maris is in use or its open-file state is unknown; close it first'
    fi
}
check_destination
printf 'Source: %s\nInstall: %s\nCommand: %s\n' "$SOURCE" "$DEST" "$LINK"
[ "$ALLOW" -eq 0 ] || printf '%s\n' 'WARNING: trusted offline payload installation; verify its origin and checksum.' >&2
[ "$DRY" -eq 0 ] || { printf '%s\n' 'Dry run complete; no files changed.'; exit 0; }
if [ "$YES" -eq 0 ]; then
    [ -t 0 ] || fail 'Noninteractive installation requires --yes'
    printf 'Install without starting audio? [y/N] '
    read -r answer
    case "$answer" in y|Y|yes|YES) ;; *) printf '%s\n' 'Cancelled.'; exit 0 ;; esac
fi
mkdir -p -- "$PREFIX"
CANDIDATE_LOCK="$PREFIX/.maris-install.lock"
mkdir -- "$CANDIDATE_LOCK" 2>/dev/null || fail 'Another installation may be active; do not delete a live install lock'
LOCK=$CANDIDATE_LOCK
check_destination
mkdir -p -- "$PREFIX/lib" "$PREFIX/bin"
STAGE=$(mktemp -d "$PREFIX/.maris-stage.XXXXXX")
cp -R -- "$SOURCE" "$STAGE/Maris"
validate "$STAGE/Maris"
diff -qr -- "$SOURCE" "$STAGE/Maris" >/dev/null || fail 'Source payload changed while copying'
check_destination
if [ ! -L "$LINK" ]; then
    ln -s -- ../lib/maris/bin/maris "$LINK"
    LINK_CREATED=1
fi
if [ -d "$DEST" ]; then
    BACKUP=$(mktemp -d "$PREFIX/.maris-backup.XXXXXX")
    mv -- "$DEST" "$BACKUP/Maris"
fi
mv -- "$STAGE/Maris" "$DEST"
COMMITTED=1
printf 'Installed: %s\nCommand: %s\n' "$DEST" "$LINK"
[ -z "$BACKUP" ] || printf 'Previous payload retained: %s/Maris\n' "$BACKUP"
printf '%s\n' 'No services or audio were started; shell PATH and preferences are unchanged.'
printf '%s\n' 'Audio was not started. Launch Maris explicitly to select an audio source and grant any required recording permission.'
