# Maris security patch

This directory contains flexaudio-core v0.2.0 from Studio Sadola's flexaudio repository.

- Upstream tag: v0.2.0
- Upstream license: MIT (see LICENSE)
- Source changes: none
- Local manifest change: ringbuf is pinned to 0.5.2 instead of the upstream 0.4 range.

The dependency change removes the ringbuf memory-safety issue fixed by RUSTSEC-2026-0293
while preserving the flexaudio-core 0.2.0 API used by the remaining flexaudio crates.
