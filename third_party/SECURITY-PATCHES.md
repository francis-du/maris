# Security compatibility patches

Maris vendors two dependency sources only to remove known unsafe transitive versions without suppressing advisories.

- security-flexaudio-core: flexaudio-core 0.2.0 source, unchanged except its ringbuf dependency is raised from 0.4 to 0.5.2. RustSec RUSTSEC-2026-0293 is fixed in ringbuf 0.5.2.
- security-tao: Tao 0.37.1 source for Maris' macOS/Windows-only dependency. The vendored manifest removes Tao's unused Linux/BSD GTK3 target dependency section, so glib 0.18.5 (RUSTSEC-2024-0429) is not part of Maris' resolved release graph.

These patches do not disable cargo-audit checks and do not claim changes to upstream ownership or licensing. Original crate license files are retained in each vendored directory.
