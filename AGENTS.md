# Maris engineering guide

- Source identifiers, comments, documentation, and machine-readable schemas use English. Human-facing UI supports localized resources with English fallback.
- Optimize for enjoyable music on the current output, not a growing effects checklist. Separate measured device correction from subjective listening preferences. Never infer headphone response from a song spectrum. Keep speech denoising out of default music playback.
- Implement product logic in Rust; isolate audio platform code in src/audio. Only src/main.rs and src/lib.rs belong directly under src. Keep DSP in dsp/, device matching in devices/, preferences and proposals in tuning/, and presentation in ui/tui/ or ui/desktop/. The module map is in docs/reference/architecture.md.
- Use domain module paths in production code. Existing root re-exports in lib.rs are compatibility aliases, not a second implementation. Update module declarations, file includes, documentation and .wcode together when moving source.
- Audio callbacks must not allocate, lock, perform file/network I/O, log, or call models. Use preallocated state and bounded queues.
- Agents tune through validated commands and profiles. Never execute arbitrary shell through a plugin.
- A digital limiter is not hearing protection. Do not report dB SPL without calibrated hardware sensitivity and acoustic measurements.
- Do not start capture or modify system routing without explicit user activation.
- Report failures honestly. Device discovery, offline rendering, and compilation do not prove successful system takeover or sound quality.
- Put standalone tests in tests/. Keep production source files below 1000 lines.
- Run cargo fmt --check, cargo test, and cargo clippy --all-targets -- -D warnings after changes.
