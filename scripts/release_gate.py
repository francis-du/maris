#!/usr/bin/env python3
"""Fail-closed public-release preflight. This script never signs, grants approval or publishes."""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import os
import sys
import tempfile
import uuid
import plistlib
import re
import stat
import subprocess
from source_audit import ROOT, source_digest
from verify_rounds import SUITES, test_passed
from portable_package import host_target, validate_binary
from release_requirements import require_complete, blockers as requirement_blockers

EVIDENCE = ("real_device_matrix", "listening_validation", "dependency_notices", "project_license", "remote_ci")


def within_root(path: Path, root: Path = ROOT) -> Path:
    absolute = path if path.is_absolute() else root / path
    if any(p == ".." for p in absolute.parts):
        raise ValueError("Evidence path traversal rejected")
    current = absolute
    while current != root and current != current.parent:
        if current.is_symlink():
            raise ValueError("Symlink evidence rejected")
        current = current.parent
    resolved = absolute.resolve()
    resolved.relative_to(root.resolve())
    return resolved


def check_report(report: dict, source: str, directory: Path, expected_platform: str | None = None) -> None:
    if expected_platform is not None and report.get("platform") != expected_platform:
        raise ValueError("Round evidence must come from the native release platform")
    if (report.get("source_sha256") != source or report.get("source_unchanged") is not True
            or report.get("passed") is not True or report.get("completed_rounds", 0) < 200
            or report.get("completed_rounds") != report.get("requested_rounds")
            or report.get("passed_rounds") != report.get("completed_rounds")):
        raise ValueError("Missing 200 successful current-source rounds")
    records = report.get("records", [])
    if len(records) != report["completed_rounds"]:
        raise ValueError("Incomplete round records")
    for index, record in enumerate(records, 1):
        if record.get("round") != index or record.get("seed") != index or record.get("passed") is not True:
            raise ValueError("Missing, duplicated or failed round")
        steps = record.get("steps", [])
        expected = ["release_matrix", SUITES[(index - 1) % len(SUITES)]]
        if len(steps) != 2 or [step.get("suite") for step in steps] != expected:
            raise ValueError("Round does not contain both required executions")
        for step in steps:
            name = step.get("log", "")
            if name != f"round-{index:03d}-{step['suite']}.log":
                raise ValueError("Unsafe, reused or incorrectly bound round log")
            path = directory / name
            if path.is_symlink() or not path.is_file():
                raise ValueError("Round log is missing or linked")
            data = path.read_bytes()
            if (step.get("passed") is not True or not test_passed(data, step.get("exit_code"))
                    or hashlib.sha256(data).hexdigest() != step.get("log_sha256")):
                raise ValueError("Failed, empty, ignored or modified round log")


def bundle_digest(bundle: Path) -> str:
    """Bind approval to all regular bundle files and permission bits, not only its name."""
    if bundle.is_symlink() or not bundle.is_dir():
        raise ValueError("Bundle is missing or linked")
    digest = hashlib.sha256()
    pending = [bundle]
    count = 0
    total = 0
    while pending:
        directory = pending.pop()
        for path in sorted(directory.iterdir()):
            count += 1
            if count > 10000:
                raise ValueError("Bundle file-count limit exceeded")
            info = path.lstat()
            if stat.S_ISDIR(info.st_mode):
                pending.append(path)
                continue
            if not stat.S_ISREG(info.st_mode):
                raise ValueError("Bundle contains a linked or special file")
            total += info.st_size
            if total > 512 * 1024 * 1024:
                raise ValueError("Bundle size limit exceeded")
            digest.update(path.relative_to(bundle).as_posix().encode() + b"\0")
            digest.update(str(stat.S_IMODE(info.st_mode)).encode() + b"\0")
            file_hash = hashlib.sha256()
            with path.open("rb") as file:
                for chunk in iter(lambda: file.read(65536), b""):
                    file_hash.update(chunk)
            digest.update(file_hash.digest())
    if count == 0:
        raise ValueError("Empty application bundle")
    return digest.hexdigest()


def check_approval(approval: dict, source: str, root: Path = ROOT) -> None:
    if approval.get("platform") not in ("macos", "windows", "linux"):
        raise ValueError("Acceptance must specify its release platform")
    reviewer = approval.get("reviewed_by")
    if approval.get("source_sha256") != source or not isinstance(reviewer, str) or not reviewer.strip():
        raise ValueError("Acceptance must identify the reviewer and exact current source")
    if not re.fullmatch(r"[a-f0-9]{64}", str(approval.get("bundle_sha256", ""))):
        raise ValueError("Acceptance must bind the final reviewed bundle SHA-256")
    def validate_evidence(item: str, evidence: dict) -> None:
        if not isinstance(evidence, dict) or evidence.get("accepted") is not True or not evidence.get("path"):
            raise ValueError(f"Acceptance evidence missing: {item}")
        path = within_root(Path(evidence["path"]), root)
        if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != evidence.get("sha256"):
            raise ValueError(f"Acceptance record missing or modified: {item}")
    required = EVIDENCE + (("publisher_key_current",) if approval["platform"] == "linux" else ())
    for item in required:
        validate_evidence(item, approval.get(item, {}))
    require_complete(root, approval, validate_evidence)


def verify_publisher(bundle: Path, system: str, arch: str, version: str, approval: dict, args, digest: str) -> None:
    if system == "macos":
        metadata = plistlib.loads((bundle / "Contents/Info.plist").read_bytes())
        if metadata.get("CFBundleIdentifier") != "audio.maris.app" or metadata.get("CFBundleShortVersionString") != version:
            raise ValueError("Incorrect application identity or source version")
        commands = [["/usr/bin/lipo", "-verify_arch", arch, str(bundle / "Contents/MacOS/maris")],
                    ["/usr/bin/codesign", "--verify", "--deep", "--strict", str(bundle)],
                    ["/usr/sbin/spctl", "--assess", "--type", "execute", str(bundle)],
                    ["/usr/bin/xcrun", "stapler", "validate", str(bundle)]]
    else:
        marker = (bundle / ".maris-package").read_text(encoding="utf-8").splitlines()
        if marker != ["maris-package-v1", system, arch, version]:
            raise ValueError("Incorrect package identity, native architecture or source version")
        binary = bundle / "bin" / ("maris.exe" if system == "windows" else "maris")
        validate_binary(binary, system, arch)
        if system == "windows":
            # Reuse the installer Authenticode/PE validator in its no-write mode.
            prefix = Path(os.environ["USERPROFILE"]) / (".maris-release-check-" + uuid.uuid4().hex)
            commands = [["powershell.exe", "-NoProfile", "-NonInteractive", "-File", str(ROOT / "install.ps1"),
                         "-From", str(bundle), "-Prefix", str(prefix), "-DryRun"]]
        else:
            if args.signature is None or args.digest_file is None or args.keyring is None:
                raise ValueError("Linux publication requires a signed payload digest and reviewed public keyring")
            fingerprint = str(approval.get("publisher_key_fingerprint", "")).upper()
            if not re.fullmatch(r"[A-F0-9]{40}|[A-F0-9]{64}", fingerprint):
                raise ValueError("A reviewed Linux signing-key fingerprint is required")
            signature, digest_file, keyring = [within_root(path) for path in (args.signature, args.digest_file, args.keyring)]
            if digest_file.read_text(encoding="ascii").strip() != digest:
                raise ValueError("Signed digest does not describe the reviewed payload")
            review = within_root(Path(".maris-review"))
            review.mkdir(exist_ok=True)
            # gpgv trusts the supplied keyring and does not establish revocation/expiry status.
            # publisher_key_current is therefore a separate required owner-reviewed record.
            with tempfile.TemporaryDirectory(prefix="signature-", dir=review) as temporary:
                result = subprocess.run(["gpgv", "--homedir", temporary, "--keyring", str(keyring),
                                         "--status-fd", "1", str(signature), str(digest_file)], capture_output=True, timeout=60)
            signatures = re.findall(rb"\[GNUPG:\] VALIDSIG ([A-F0-9]{40,64}) ", result.stdout)
            if result.returncode or fingerprint.encode() not in signatures:
                raise ValueError("Linux publisher signature validation failed")
            return
    for command in commands:
        result = subprocess.run(command, capture_output=True, timeout=60)
        if result.returncode:
            raise ValueError(f"Release payload validation failed: {Path(command[0]).name}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path)
    parser.add_argument("--approval", type=Path)
    parser.add_argument("--bundle", type=Path, help="Maris.app on macOS, Maris payload directory on Windows/Linux")
    parser.add_argument("--signature", type=Path, help="Linux detached OpenPGP signature of the payload digest file")
    parser.add_argument("--digest-file", type=Path)
    parser.add_argument("--keyring", type=Path, help="Explicit reviewed Linux public-key ring; no private keys")
    args = parser.parse_args()
    source = source_digest()
    system, arch = host_target()
    blockers = []
    try:
        unfinished = requirement_blockers(ROOT)
        if unfinished:
            blockers.append("Unfinished user requirements: " + ", ".join(unfinished))
    except (ValueError, OSError, TypeError, KeyError) as error:
        blockers.append(str(error))
    accepted_bundle = None
    approval = {}
    for label, path in [("automated report", args.report), ("acceptance evidence", args.approval)]:
        try:
            if path is None:
                raise ValueError(f"Missing {label}")
            file = within_root(path)
            data = json.loads(file.read_text(encoding="utf-8"))
            if label == "automated report":
                check_report(data, source, file.parent, sys.platform)
            else:
                check_approval(data, source)
                if data["platform"] != system:
                    raise ValueError("Acceptance belongs to another release platform")
                approval = data
                accepted_bundle = data["bundle_sha256"]
        except (ValueError, OSError, TypeError, KeyError) as error:
            blockers.append(str(error))
    if args.bundle is None:
        blockers.append(f"A reviewed {system} payload and its native publisher verification are required")
    else:
        try:
            bundle = within_root(args.bundle)
            digest = bundle_digest(bundle)
            if digest != accepted_bundle:
                raise ValueError("Bundle does not match final reviewed acceptance evidence")
            version = re.search(r'^version = "([0-9.]+)"$', (ROOT / "Cargo.toml").read_text(encoding="utf-8"), re.M)
            if version is None:
                raise ValueError("Missing Cargo version")
            verify_publisher(bundle, system, arch, version.group(1), approval, args, digest)
            if bundle_digest(bundle) != digest or source_digest() != source:
                raise ValueError("Bundle or source changed during release verification")
        except (ValueError, OSError, subprocess.SubprocessError) as error:
            blockers.append(str(error))
    print(json.dumps({"source_sha256": source, "platform": system, "architecture": arch,
                      "bundle_sha256": accepted_bundle, "release_ready": not blockers,
                      "blockers": blockers, "published": False}, indent=2))
    return 1 if blockers else 0


if __name__ == "__main__":
    raise SystemExit(main())
