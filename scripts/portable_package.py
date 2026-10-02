"""Shared Windows/Linux payload format and non-executing native binary validation."""
from __future__ import annotations
import os
from pathlib import Path
import platform
import shutil
import struct


def host_target() -> tuple[str, str]:
    system = {"Darwin": "macos", "Linux": "linux", "Windows": "windows"}.get(platform.system())
    if system is None:
        raise ValueError("Supported package platforms are macOS, Linux and Windows")
    machine = platform.machine().lower()
    if system == "windows":
        machine = os.environ.get("PROCESSOR_ARCHITEW6432", os.environ.get("PROCESSOR_ARCHITECTURE", machine)).lower()
    arch = {"amd64": "x86_64", "x86_64": "x86_64", "aarch64": "arm64", "arm64": "arm64"}.get(machine)
    if arch is None:
        raise ValueError("Supported package architectures are x86_64 and arm64")
    return system, arch


def validate_binary(path: Path, system: str, arch: str) -> None:
    if arch not in ("x86_64", "arm64") or system not in ("macos", "linux", "windows"):
        raise ValueError("Unsupported portable binary target")
    if path.is_symlink() or not path.is_file():
        raise ValueError("Binary is missing or linked")
    with path.open("rb") as file:
        header = file.read(64)
        if len(header) != 64:
            raise ValueError("Truncated executable")
        if system == "macos":
            machine = 0x01000007 if arch == "x86_64" else 0x0100000C
            if (header[:4] != b"\xcf\xfa\xed\xfe" or struct.unpack_from("<I", header, 4)[0] != machine
                    or struct.unpack_from("<I", header, 12)[0] != 2):
                raise ValueError("Mach-O platform or architecture mismatch")
        elif system == "linux":
            machine = 62 if arch == "x86_64" else 183
            if (header[:7] != b"\x7fELF\x02\x01\x01" or struct.unpack_from("<H", header, 18)[0] != machine
                    or struct.unpack_from("<H", header, 16)[0] not in (2, 3)):
                raise ValueError("ELF platform or architecture mismatch")
        else:
            offset = struct.unpack_from("<I", header, 60)[0]
            if header[:2] != b"MZ" or offset < 64 or offset + 26 > path.stat().st_size:
                raise ValueError("Invalid PE header")
            file.seek(offset)
            pe = file.read(26)
            machine = 0x8664 if arch == "x86_64" else 0xAA64
            flags = struct.unpack_from("<H", pe, 22)[0]
            if (pe[:4] != b"PE\x00\x00" or struct.unpack_from("<H", pe, 4)[0] != machine
                    or not flags & 2 or flags & 0x2000 or struct.unpack_from("<H", pe, 24)[0] != 0x20B):
                raise ValueError("PE platform or architecture mismatch")


def create_payload(root: Path, executable: Path, destination: Path, system: str, arch: str, version: str, *, cli: bool = False) -> None:
    validate_binary(executable, system, arch)
    if destination.exists() or destination.is_symlink():
        raise ValueError("Portable payload destination already exists")
    (destination / "bin").mkdir(parents=True)
    resources = destination / "resources"
    resources.mkdir()
    binary = destination / "bin" / ("maris.exe" if system == "windows" else "maris")
    shutil.copyfile(executable, binary)
    binary.chmod(0o755)
    shutil.copyfile(root / "docs/reference/third-party.md", resources / "THIRD_PARTY.md")
    shutil.copytree(root / "third_party/eqmac", resources / "eqmac")
    marker = f"maris-package-v2\n{system}\n{arch}\n{version}\ncli\n" if cli else f"maris-package-v1\n{system}\n{arch}\n{version}\n"
    (destination / ".maris-package").write_text(marker, encoding="utf-8", newline="\n")
