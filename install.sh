#!/bin/bash
# Standalone online installer. Download a precompiled approved GitHub release by default.
# Bash 3.2 compatible; local build/from modes delegate to the existing transactional installers.
set -euo pipefail
umask 077
export LC_ALL=C
unset TAR_OPTIONS GZIP GZIP_OPT || true
fail() { printf 'Maris install: %s\n' "$*" >&2; exit 1; }
usage() {
    cat <<'HELP'
Usage: bash install.sh [--version vX.Y.Z] [--prefix PATH] [--yes] [--dry-run]
  Default           Download the latest approved CI-built release; no Rust/compiler needed
  --version X.Y.Z   Pin an approved stable release instead of latest
  --prefix PATH     Override the normal per-user destination
  --yes             Confirm installation without an interactive prompt
  --dry-run         Print the online plan without downloads, writes or installation
  --from PATH       Offline installation from an already extracted local package
  --build           Developer-only: compile this source checkout, then install
  --allow-unsigned  Only valid with --from or --build, never an online-release bypass
  --sha256 HASH     Offline Linux binary checksum; online checksums come from the release manifest

Supported targets: macOS/Linux, x86_64/ARM64. Windows uses install.ps1.
Online failures never fall back to compilation or an unverified development build.
No sudo, drivers, audio activation, OS volume/default-output changes or security bypasses.
HELP
}
ARGS=("$@")
VERSION=''; PREFIX=''; LOCAL_PAYLOAD=''; YES=0; DRY=0; LOCAL=0; BUILD=0; FROM=0; UNTRUSTED=0
while [ "$#" -gt 0 ]; do
    case "$1" in
        --version|--prefix|--from|--sha256)
            [ "$#" -ge 2 ] && [ -n "$2" ] || fail "$1 needs a value"
            case "$2" in --*) fail "$1 needs a value, not another option" ;; esac
            case "$1" in
                --version) [ -z "$VERSION" ] || fail 'Duplicate --version'; VERSION=${2#v} ;;
                --prefix) PREFIX=$2 ;;
                --from) LOCAL=1; FROM=1; LOCAL_PAYLOAD=$2 ;;
                --sha256) UNTRUSTED=1 ;;
            esac
            shift 2 ;;
        --build) LOCAL=1; BUILD=1; shift ;;
        --allow-unsigned) UNTRUSTED=1; shift ;;
        --yes) YES=1; shift ;;
        --dry-run) DRY=1; shift ;;
        --help|-h) usage; exit 0 ;;
        *) fail "Unknown option: $1" ;;
    esac
done
[ "$BUILD:$FROM" != 1:1 ] || fail '--from and --build are mutually exclusive'
if [ "$LOCAL" -eq 1 ]; then
    [ -z "$VERSION" ] || fail '--version cannot be combined with --from or --build'
    ROOT=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
    if [ "$FROM" -eq 1 ] && [ -f "$LOCAL_PAYLOAD/.maris-package" ] && [ "$(sed -n '1p' "$LOCAL_PAYLOAD/.maris-package")" = maris-package-v2 ]; then
        exec /bin/bash "$ROOT/scripts/install_cli.sh" "${ARGS[@]}"
    fi
    case "$(uname -s)" in
        Linux) exec /bin/bash "$ROOT/scripts/install_linux.sh" "${ARGS[@]}" ;;
        Darwin) exec /bin/bash "$ROOT/scripts/install_macos.sh" "${ARGS[@]}" ;;
        *) fail 'Use install.ps1 on Windows' ;;
    esac
fi
[ "$UNTRUSTED" -eq 0 ] || fail 'Online installation requires release verification; use --from for explicit development/offline installation'
[ -z "$VERSION" ] || [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail '--version must be a stable X.Y.Z or vX.Y.Z'
case "$(uname -s)" in
    Darwin) SYSTEM=macos; DEFAULT_PREFIX="${HOME:?HOME is required}/Applications" ;;
    Linux) SYSTEM=linux; DEFAULT_PREFIX="${HOME:?HOME is required}/.local" ;;
    *) fail 'Use install.ps1 on Windows; unsupported Unix platform' ;;
esac
case "$(uname -m)" in
    x86_64) ARCH=x86_64
        if [ "$SYSTEM" = macos ] && [ "$(/usr/sbin/sysctl -in sysctl.proc_translated 2>/dev/null || true)" = 1 ]; then ARCH=arm64; fi ;;
    aarch64|arm64) ARCH=arm64 ;;
    *) fail 'Only x86_64 and ARM64 release packages are supported' ;;
esac
PREFIX_PROVIDED=$PREFIX
[ -n "$PREFIX" ] || PREFIX=$DEFAULT_PREFIX
BASE=https://github.com/francis-du/maris/releases
if [ -n "$VERSION" ]; then MANIFEST_URL="$BASE/download/v$VERSION/maris-release.tsv";
else MANIFEST_URL="$BASE/latest/download/maris-release.tsv"; fi
PLAN_DESTINATION=$PREFIX
if [ "$SYSTEM" = macos ] && [ -z "$PREFIX_PROVIDED" ]; then
    PLAN_DESTINATION="${HOME}/.local (CLI) or ${HOME}/Applications (GUI), resolved from the release manifest"
fi
printf 'Mode: online precompiled release\nTarget: %s / %s\nDestination: %s\nManifest: %s\n' "$SYSTEM" "$ARCH" "$PLAN_DESTINATION" "$MANIFEST_URL"
[ "$DRY" -eq 0 ] || { printf '%s\n' 'Dry run: download, verify and install after approval. No network or files changed.'; exit 0; }
if [ "$YES" -eq 0 ]; then
    [ -r /dev/tty ] || fail 'Noninteractive installation requires --yes'
    printf 'Download, verify and install Maris without starting audio? [y/N] ' >/dev/tty
    IFS= read -r ANSWER </dev/tty || fail 'Cannot read installation confirmation'
    case "$ANSWER" in y|Y|yes|YES|Yes) YES=1 ;; *) printf '%s\n' 'Cancelled.'; exit 0 ;; esac
fi
[ "$(id -u)" -ne 0 ] || fail 'Run as your normal account, without sudo'
for tool in curl tar gzip head awk wc mktemp find cat mkdir sleep; do command -v "$tool" >/dev/null 2>&1 || fail "Required system tool missing: $tool"; done
if command -v sha256sum >/dev/null 2>&1; then SHA=(sha256sum);
elif command -v shasum >/dev/null 2>&1; then SHA=(shasum -a 256);
else fail 'A system SHA-256 utility is required'; fi
WORK=$(mktemp -d "${TMPDIR:-/tmp}/maris-download.XXXXXX")
cleanup_download() { local code=$?; trap - EXIT HUP INT TERM; rm -rf -- "$WORK"; exit "$code"; }
trap cleanup_download EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
fetch() {
    local url=$1 output=$2 limit=$3 attempt
    # Retry whole transfers into a freshly truncated private file. curl's stdout
    # retries cannot rewind a pipe and would otherwise append a second response.
    # The pipe also caps chunked transfers without a Content-Length header.
    for attempt in 1 2 3; do
        if curl --fail --silent --show-error --location --max-redirs 5 --proto '=https' --proto-redir '=https' \
            --tlsv1.2 --connect-timeout 15 --max-time 180 "$url" \
            | head -c "$((limit + 1))" > "$output"; then
            [ "$(wc -c < "$output")" -le "$limit" ] && return 0
            return 1
        fi
        [ "$(wc -c < "$output")" -le "$limit" ] || return 1
        [ "$attempt" -eq 3 ] || sleep "$attempt"
    done
    return 1
}
fetch "$MANIFEST_URL" "$WORK/manifest.tsv" 32768 || fail 'Approved release manifest unavailable (unpublished version, HTTP/network failure or size limit). Nothing installed; no compilation fallback.'
SELECTED=$(awk -F '\t' -v target_os="$SYSTEM" -v arch="$ARCH" -v requested="$VERSION" '
function bad() { invalid=1; exit 2 }
function hash(s) { return length(s)==64 && s !~ /[^a-f0-9]/ }
NR==1 { if ($0!="maris-release-v1" && $0!="maris-release-v2") bad(); v2=($0=="maris-release-v2"); interface="gui"; next }
NR==2 { if (NF!=2 || $1!="version" || $2 !~ /^[0-9]+\.[0-9]+\.[0-9]+$/) bad(); version=$2; next }
NR==3 { if (NF!=2 || $1!="source_sha256" || !hash($2)) bad(); source=$2; next }
NR==4 { if (NF!=2 || $1!="channel" || $2!="stable") bad(); next }
NR==5 && v2 { if (NF!=2 || $1!="interface" || $2!="cli") bad(); interface=$2; next }
NR>4 {
    if (NF!=7 || $1!="asset" || $2 !~ /^(macos|linux|windows)$/ || $3 !~ /^(x86_64|arm64)$/) bad()
    name="Maris-" version "-" $2 "-" $3 ($2=="windows" ? ".zip" : ".tar.gz")
    if ($4!=name || !hash($5) || $6 !~ /^[1-9][0-9]*$/ || length($6)>9 || $6>134217728 || !hash($7)) bad()
    if (seen[$2 "/" $3]++) bad()
    if ($2==target_os && $3==arch) selected=version "\t" source "\t" $4 "\t" $5 "\t" $6 "\t" $7
}
END { if (invalid || NR!=10+v2 || selected=="" || (requested!="" && version!=requested)) exit 2; print selected "\t" interface }
' "$WORK/manifest.tsv") || fail 'Release manifest is incomplete, unapproved, malformed or does not match the requested version/target'
IFS=$'\t' read -r VERSION SOURCE_HASH NAME ARCHIVE_HASH ARCHIVE_BYTES BINARY_HASH INTERFACE <<< "$SELECTED"
[ "$INTERFACE" != cli ] || [ -n "$PREFIX_PROVIDED" ] || PREFIX="${HOME}/.local"
# Pin all remaining requests to the resolved tag, so latest cannot race a second release.
printf 'Version: %s\nAsset: %s\nDestination: %s\n' "$VERSION" "$NAME" "$PREFIX"
fetch "$BASE/download/v$VERSION/$NAME" "$WORK/package.tar.gz" "$ARCHIVE_BYTES" || fail 'Package download failed; existing installation is unchanged'
[ "$(wc -c < "$WORK/package.tar.gz")" -eq "$ARCHIVE_BYTES" ] || fail 'Package length mismatch'
ACTUAL=$("${SHA[@]}" "$WORK/package.tar.gz"); ACTUAL=${ACTUAL%% *}
[ "$ACTUAL" = "$ARCHIVE_HASH" ] || fail 'Package SHA-256 mismatch; refusing extraction'
gzip -dc "$WORK/package.tar.gz" | head -c 536870913 > "$WORK/package.tar" || fail 'Invalid or oversized compressed package'
[ "$(wc -c < "$WORK/package.tar")" -le 536870912 ] || fail 'Expanded archive exceeds 512 MiB'
tar -tf "$WORK/package.tar" | head -n 10001 > "$WORK/names" || fail 'Cannot inspect package entries'
[ "$(wc -l < "$WORK/names")" -le 10000 ] || fail 'Package has too many entries'
KIT="Maris-$VERSION-$SYSTEM-$ARCH"
awk -v root="$KIT" '
{ p=$0; sub(/\/$/,"",p); if (p !~ /^[A-Za-z0-9_.\/-]+$/ || (p!=root && index(p,root "/")!=1)) exit 1;
  n=split(p,a,"/"); for(i=1;i<=n;i++) if(a[i]=="" || a[i]=="." || a[i]=="..") exit 1;
  if(seen[tolower(p)]++) exit 1;
  prefix=""; for(i=1;i<=n;i++) { prefix=prefix (i==1 ? "" : "/") a[i]; key=tolower(prefix);
    if (key in spelling && spelling[key]!=prefix) exit 1; spelling[key]=prefix; } }
' "$WORK/names" || fail 'Unsafe, duplicate, case-colliding or out-of-scope archive path'
tar --numeric-owner -tvf "$WORK/package.tar" > "$WORK/entries" || fail 'Cannot inspect archive types'
SIZE_COLUMN=3; [ "$SYSTEM" != macos ] || SIZE_COLUMN=5
awk -v col="$SIZE_COLUMN" '
{ if ($1 !~ /^[-d][rwx-]+$/ || length($1)!=10 || $col !~ /^[0-9]+$/) exit 1;
  total+=$col; if (total>536870912 || NR>10000) exit 1; }
END { if (NR==0) exit 1 }
' "$WORK/entries" || fail 'Linked, special, privileged or oversized archive entries rejected'
mkdir "$WORK/extracted"
EXTRACT_OPTIONS=(--no-same-owner --no-same-permissions)
# Preserve the approved macOS payload metadata inside the private extraction root.
[ "$SYSTEM" != macos ] || EXTRACT_OPTIONS+=(--mac-metadata)
tar "${EXTRACT_OPTIONS[@]}" -xf "$WORK/package.tar" -C "$WORK/extracted" || fail 'Package extraction failed'
PAYLOAD="$WORK/extracted/$KIT"
EXPECTED_MARKER=$(printf 'maris-install-kit-v1\n%s\n%s\n%s\n%s\nstable\n%s' "$VERSION" "$SYSTEM" "$ARCH" "$SOURCE_HASH" "$BINARY_HASH")
if [ "$INTERFACE" = cli ]; then EXPECTED_MARKER=$(printf 'maris-install-kit-v2\n%s\n%s\n%s\n%s\nstable\n%s\ncli' "$VERSION" "$SYSTEM" "$ARCH" "$SOURCE_HASH" "$BINARY_HASH"); fi
[ -f "$PAYLOAD/.maris-release" ] && [ "$(cat "$PAYLOAD/.maris-release")" = "$EXPECTED_MARKER" ] || fail 'Package identity/channel differs from the approved manifest'
[ -z "$(find "$PAYLOAD" ! -type d ! -type f -print -quit)" ] || fail 'Extracted package contains linked or special files'
if [ "$INTERFACE" = cli ]; then
    SOURCE="$PAYLOAD/Maris"; BINARY="$SOURCE/bin/maris"
    [ -f "$SOURCE/.maris-package" ] && [ "$(cat "$SOURCE/.maris-package")" = "$(printf 'maris-package-v2\n%s\n%s\n%s\ncli' "$SYSTEM" "$ARCH" "$VERSION")" ] || fail 'CLI payload identity differs from its release manifest'
elif [ "$SYSTEM" = macos ]; then
    SOURCE="$PAYLOAD/Maris.app"; BINARY="$SOURCE/Contents/MacOS/maris"
    [ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$SOURCE/Contents/Info.plist" 2>/dev/null)" = "$VERSION" ] || fail 'Application version differs from its release manifest'
else
    SOURCE="$PAYLOAD/Maris"; BINARY="$SOURCE/bin/maris"
    [ -f "$SOURCE/.maris-package" ] && [ "$(cat "$SOURCE/.maris-package")" = "$(printf 'maris-package-v1\nlinux\n%s\n%s' "$ARCH" "$VERSION")" ] || fail 'Native payload identity differs from its release manifest'
fi
ACTUAL=$("${SHA[@]}" "$BINARY"); ACTUAL=${ACTUAL%% *}
[ "$ACTUAL" = "$BINARY_HASH" ] || fail 'Executable checksum does not match the release manifest'
HELPER="$PAYLOAD/scripts/install_$SYSTEM.sh"
[ "$INTERFACE" != cli ] || HELPER="$PAYLOAD/scripts/install_cli.sh"
[ -f "$HELPER" ] || fail 'Verified package is missing its native installer'
OPTIONS=(--from "$SOURCE" --prefix "$PREFIX" --yes)
[ "$SYSTEM" != linux ] && [ "$INTERFACE" != cli ] || OPTIONS+=(--sha256 "$BINARY_HASH")
# Execute only the installer from the hash-verified release kit; no arbitrary URL/code parameter.
/bin/bash "$HELPER" "${OPTIONS[@]}"
