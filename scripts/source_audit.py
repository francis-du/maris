#!/usr/bin/env python3
"""Audit the explicit Maris source set without traversing build/cache/credential directories."""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]
TOP = {".gitignore", ".gitattributes", "AGENTS.md", "README.md", "Cargo.lock", "Cargo.toml",
       "Maris-Info.plist", "build.rs", "install.sh", "install.ps1",
       ".wcode/project.yaml"}
PREFIXES = ("src/", "tests/", "scripts/", "docs/", ".github/", "third_party/eqmac/",
            "third_party/autoeq/", "third_party/musicnn/", "third_party/flexaudio-core/",
            ".wcode/design/")
SECRET_PATTERNS = [re.compile(r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----"),
                   re.compile(r"gh[pousr]_[A-Za-z0-9]{30,}"),
                   re.compile(r"github_pat_[A-Za-z0-9_]{40,}"),
                   re.compile(r"sk-(?:proj-)?[A-Za-z0-9_-]{40,}"),
                   re.compile(r"AKIA[A-Z0-9]{16}")]
FORBIDDEN = {".env", ".DS_Store", "runtime.json", "listening.json", "profile.json", "ui.json"}


def source_files(root: Path = ROOT) -> list[Path]:
    root = root.resolve()
    result = subprocess.run(["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
                            cwd=root, capture_output=True, check=True)
    names = sorted(set(result.stdout.decode("utf-8").split("\0")) - {""})
    files = []
    problems = []
    for name in names:
        path = root / name
        if name not in TOP and not name.startswith(PREFIXES):
            problems.append(f"Unexpected publish path: {name}")
            continue
        if (path.is_symlink() or not path.is_file() or not path.resolve().is_relative_to(root)
                or path.name in FORBIDDEN
                or any(p in {"__pycache__", "node_modules", "target", ".git"} for p in path.parts)
                or path.suffix.lower() in {".pem", ".p12", ".pfx", ".key", ".wav", ".log"}):
            problems.append(f"Forbidden publish input: {name}")
            continue
        if path.stat().st_size > 2_000_000:
            problems.append(f"Unexpected large source: {name}")
            continue
        text = path.read_text(encoding="utf-8")
        for line_no, line in enumerate(text.splitlines(), 1):
            if any(pattern.search(line) for pattern in SECRET_PATTERNS):
                problems.append(f"Potential credential at {name}:{line_no}; value withheld")
            if re.search(r"/Users/[A-Za-z][A-Za-z0-9._-]+/", line):
                problems.append(f"Personal absolute path at {name}:{line_no}")
        files.append(path)
    if problems:
        raise ValueError("\n".join(problems))
    if not files:
        raise ValueError("No audited source files")
    return files


def check_staged(root: Path = ROOT) -> list[Path]:
    """Check the complete Git index against audited files, including untracked source.

    Read blobs directly so assume-unchanged flags cannot hide stale staged content.
    This function never stages, commits, refreshes or resets the caller's index.
    """
    files = source_files(root)
    command = ["git", "ls-files", "--stage", "-z"]
    before = subprocess.run(command, cwd=root, capture_output=True, check=True).stdout
    entries = []
    for record in before.split(b"\0"):
        if not record:
            continue
        metadata, name = record.split(b"\t", 1)
        mode, oid, stage = metadata.split()
        if stage != b"0":
            raise ValueError("Unmerged index cannot be published")
        if mode not in (b"100644", b"100755"):
            raise ValueError("Only regular source files may be staged")
        entries.append((name.decode("utf-8"), oid))
    if {name for name, _ in entries} != {p.relative_to(root).as_posix() for p in files}:
        raise ValueError("Staged source set differs from the audited worktree")
    blobs = subprocess.run(["git", "cat-file", "--batch"], cwd=root,
                           input=b"\n".join(oid for _, oid in entries) + b"\n",
                           capture_output=True, check=True).stdout
    offset = 0
    for name, oid in entries:
        end = blobs.index(b"\n", offset)
        actual, kind, size_text = blobs[offset:end].split()
        size = int(size_text)
        if actual != oid or kind != b"blob" or not 0 <= size <= 2_000_000:
            raise ValueError("Invalid or oversized staged source blob")
        start = end + 1
        if blobs[start:start + size] != (root / name).read_bytes():
            raise ValueError(f"Staged content differs from the audited worktree: {name}")
        if blobs[start + size:start + size + 1] != b"\n":
            raise ValueError("Truncated staged source blob")
        offset = start + size + 1
    after = subprocess.run(command, cwd=root, capture_output=True, check=True).stdout
    if before != after or offset != len(blobs):
        raise ValueError("Git index changed during the source audit")
    return files


def source_digest(root: Path = ROOT) -> str:
    digest = hashlib.sha256()
    for path in source_files(root):
        digest.update(path.relative_to(root).as_posix().encode() + b"\0")
        digest.update(hashlib.sha256(path.read_bytes()).digest())
    return digest.hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", action="store_true", help="Write an exact NUL-delimited git pathspec")
    parser.add_argument("--staged", action="store_true", help="Require the index to match every audited source file")
    args = parser.parse_args()
    files = check_staged() if args.staged else source_files()
    if args.manifest:
        output = ROOT / ".maris-review/source-files.nul"
        output.parent.mkdir(exist_ok=True)
        output.write_bytes(b"\0".join(p.relative_to(ROOT).as_posix().encode() for p in files) + b"\0")
    print(json.dumps({"source_files": len(files), "source_sha256": source_digest(),
                      "credentials_found": False, "staged_checked": args.staged,
                      "scope": "explicit Maris source allowlist"}, indent=2))


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        raise SystemExit(str(error)) from error
