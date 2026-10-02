# AutoEq source snapshot

Upstream: https://github.com/jaakkopasanen/AutoEq
Commit: 7ae0f56d53074872b028649617a22bbb4232feb7
Recommended-results index blob SHA: b6ef90735cfdcf464862a7cc796c5c5a0187eb96
License blob SHA: 6bd650197b01da004aec3e870b010c0c0e17e1a0

`results-index.md` is an unmodified snapshot of `results/README.md` at the pinned commit. Maris uses this checked-in index for model matching so normal product use does not download the catalog. Release builds use `MARIS_BUNDLE_AUTOEQ_PROFILES=1` to check out this exact upstream revision and embed its recommended ParametricEQ text files in a verified profile pack. The build and runtime catalog parser preserve the complete generated link destination, including literal parentheses in filenames. Normal release use reads the compiled pack without network acquisition. Unbundled development builds retain only explicit profile acquisition.

The pinned index contains one case mismatch among its 6,033 recommended paths: Samsung Galaxy Buds2 Pro `(passive mode)` points to an actual Git directory and ParametricEQ filename spelled `(Passive mode)`. The shared `src/devices/catalog_link.rs` parser maps that exact destination to the pinned tree's canonical spelling for both build-time pack keys and runtime URLs. Other paths and the upstream index bytes remain unchanged; no missing profile is skipped.

Windows release builds enable `core.longpaths` only in this temporary pinned Git checkout, since some full profile filenames exceed the legacy path limit beneath Cargo's output directory. No global Git or operating-system setting changes.
