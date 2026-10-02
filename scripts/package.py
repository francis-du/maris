#!/usr/bin/env python3
"""Build a native macOS/Windows/Linux development archive; never install, sign or publish."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import plistlib
import re
import shutil
import subprocess
import tarfile
import tempfile
import zipfile
from source_audit import ROOT, source_digest
from portable_package import create_payload, host_target


def archive_payload(payload: Path, output: Path, system: str) -> None:
    if system == "macos":
        subprocess.run(["/usr/bin/ditto", "-c", "-k", "--sequesterRsrc", "--keepParent",
                        str(payload), str(output)], check=True)
    elif system == "linux":
        def clean(info: tarfile.TarInfo) -> tarfile.TarInfo:
            if not (info.isfile() or info.isdir()):
                raise ValueError("Archive may contain only regular files and directories")
            info.uid = info.gid = 0
            info.uname = info.gname = ""
            info.mtime = 0
            return info
        with tarfile.open(output, "w:gz") as archive:
            archive.add(payload, arcname=payload.name, filter=clean)
    else:
        with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            for path in sorted(payload.rglob("*")):
                if path.is_symlink():
                    raise ValueError("Linked archive payload rejected")
                if path.is_file():
                    archive.write(path, (Path(payload.name) / path.relative_to(payload)).as_posix())


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--local", action="store_true", required=True,
                        help="Acknowledge this is a development artifact, not an approved public release")
    parser.add_argument("--cli", action="store_true", help="Portable CLI/TUI distribution; no GUI application bundle")
    args = parser.parse_args()
    system, arch = host_target()
    source = source_digest()
    build_env = dict(os.environ, MARIS_BUNDLE_SMALL_MODELS="1", MARIS_BUNDLE_AUTOEQ_PROFILES="1")
    subprocess.run(["cargo", "build", "--release", "--locked", "--target-dir", str(ROOT / "target")],
                   cwd=ROOT, check=True, env=build_env)
    executable = ROOT / "target/release" / ("maris.exe" if system == "windows" else "maris")
    match = re.search(r'^version = "([0-9.]+)"$', (ROOT / "Cargo.toml").read_text(encoding="utf-8"), re.M)
    if match is None:
        raise ValueError("Cargo version is missing")
    version = match.group(1)
    stem = f"Maris-{version}-{system}-{arch}-local-{source[:12]}"
    dist = ROOT / "dist/packages"
    if dist.is_symlink() or (ROOT / "dist").is_symlink():
        raise ValueError("Refusing symlink package destination")
    dist.mkdir(parents=True, exist_ok=True)
    extension = ".tar.gz" if system == "linux" else ".zip"
    archive = dist / (stem + extension)
    if archive.exists():
        raise ValueError("Package already exists; retain it or choose a new source revision")
    with tempfile.TemporaryDirectory(prefix=".package-", dir=dist) as temporary:
        staging = Path(temporary)
        payload = staging / stem
        payload.mkdir()
        if system == "macos" and not args.cli:
            subprocess.run(["/usr/bin/lipo", str(executable), "-verify_arch", arch], check=True, capture_output=True)
            result = subprocess.run([str(executable), "--json", "package"], cwd=staging,
                                    capture_output=True, text=True, check=True,
                                    env=dict(os.environ, MARIS_STATE_DIR=str(staging / "isolated-state")))
            bundle = Path(json.loads(result.stdout)["application"])
            metadata = plistlib.loads((bundle / "Contents/Info.plist").read_bytes())
            if metadata.get("CFBundleShortVersionString") != version:
                raise ValueError("Cargo version and bundle version differ")
            shutil.move(str(bundle), payload / "Maris.app")
            resources = payload / "Maris.app/Contents/Resources"
            resources.mkdir(exist_ok=True)
            shutil.copyfile(ROOT / "docs/reference/third-party.md", resources / "THIRD_PARTY.md")
            shutil.copytree(ROOT / "third_party/eqmac", resources / "eqmac")
        else:
            create_payload(ROOT, executable, payload / "Maris", system, arch, version, cli=args.cli)
        if args.cli:
            from dependency_notices import collect
            resources = payload / "Maris/resources"
            collect(ROOT, resources / "notices", system, arch)
            if (ROOT / "LICENSE").is_file():
                shutil.copyfile(ROOT / "LICENSE", resources / "MARIS_LICENSE.txt")
            else:
                (resources / "MARIS_LICENSE_STATUS.txt").write_text(
                    "This source revision declares no project license grant. This package does not add an open-source license grant.\n"
                    "Third-party components retain the license terms reproduced in resources/notices.\n", encoding="utf-8", newline="\n")
        for source_doc, artifact_name in [("docs/en/install.md", "INSTALL.md"),
                                          ("docs/development/product-gates.md", "PRODUCT_GATES.md")]:
            shutil.copyfile(ROOT / source_doc, payload / artifact_name)
        if system == "windows":
            shutil.copyfile(ROOT / "install.ps1", payload / "install.ps1")
        else:
            shutil.copyfile(ROOT / "install.sh", payload / "install.sh")
            (payload / "install.sh").chmod(0o755)
            (payload / "scripts").mkdir()
            for helper in ["install_linux.sh", "install_macos.sh", "install_cli.sh"] if args.cli else ["install_linux.sh", "install_macos.sh"]:
                shutil.copyfile(ROOT / "scripts" / helper, payload / "scripts" / helper)
        (payload / "BUILD.json").write_text(json.dumps({
            "version": version, "platform": system, "architecture": arch, "source_sha256": source,
            "interface": "cli" if args.cli else "gui",
            "executable_sha256": hashlib.sha256(executable.read_bytes()).hexdigest(),
            "channel": "local-development", "developer_id_signed": False if system == "macos" else None,
            "authenticode_signed": False if system == "windows" else None,
            "notarized": False if system == "macos" else None,
            "libc_build_host": list(platform.libc_ver()) if system == "linux" else None,
            "native_system_takeover_implemented": system in {"macos", "linux", "windows"},
            "hardware_validation": False, "commercial_release_approved": False,
            "dependency_notice_audit_complete": False,
            "limitations": "Native system-audio code is packaged for all supported platforms; this local artifact does not establish real-device, signing, CI-matrix or listening acceptance."
        }, indent=2) + "\n", encoding="utf-8")
        staged_archive = staging / ("package" + extension)
        archive_payload(payload, staged_archive, system)
        if source_digest() != source:
            raise ValueError("Source changed while packaging")
        shutil.move(str(staged_archive), archive)
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    checksum = dist / (stem + ".sha256")
    checksum.write_text(f"{digest}  {archive.name}\n", encoding="utf-8")
    print(json.dumps({"archive": str(archive.relative_to(ROOT)), "sha256": digest,
                      "platform": system, "architecture": arch,
                      "channel": "local-development", "published": False}, indent=2))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        raise SystemExit(str(error)) from error
