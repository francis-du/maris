# MusicNN source snapshots

Original implementation: https://github.com/jordipons/musicnn
Pinned original commit: 516acb2a0ff5ef73f64547898e018e793152c506
Original license: ISC

Vendored source snapshots:
- configuration.py — d19dce5580d663579718a20391fd012db22bb016
- models.py — b0150ef6cb83421ea03ff273df5efb1f5d757dbe
- extractor.py — a85a8d44c52cae7f81df71cf17919b1fab9abe1f
- tagger.py — 349131d5534a365575f8e54e4be5bbdb9f255b4c
- LICENSE.md — 9b7ee72ed715fe0dea6f9b617a3b9d74246696d3

The native MusicNN adapter uses the reviewed PyTorch safetensors port pinned to Hugging Face revision `7cff1a4f9899825ddba77130899dfac4c8cfe9d5`. The accepted `model.safetensors` SHA-256 is `cc0b9400fcaed6e9ce7fbcfa97ec91e4fcb5f2ab34ca3a0cd6bef4af74753e1a`. Release builds acquire that exact small artifact in `build.rs`, verify its size and digest, and embed it in the executable. Normal product use never downloads this model or executes remote custom code. A locally staged file with the same digest can be used by builders for offline/reproducible packaging.
