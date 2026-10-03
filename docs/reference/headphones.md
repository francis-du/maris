# Supported headphones

Maris release builds bundle headphone-correction profiles from the pinned [AutoEq recommended-results snapshot](../../third_party/autoeq/results-index.md). The generated documentation page below expands that snapshot into the complete model list.

A model appearing here means Maris has a bundled correction profile for that exact AutoEq entry. It does **not** mean Maris has physically tested that headphone, verified your unit, or confirmed that pads, tips, fit, ANC/transparency mode, firmware, measurement rig or target match your setup. Variant names in parentheses are separate profiles.

Exact unique device names may be matched automatically. Generic endpoint names such as `Headphones` are never silently bound, and ambiguous/fuzzy matches require confirmation. You can inspect the detected model with `maris sound capability --json`, explicitly bind a model with the documented sound commands, or leave correction unbound and use Maris without AutoEq.

The pinned catalog currently contains **6,033 recommended headphone/IEM entries**. Use browser search on the generated page to find a model quickly.

## Attribution

The correction catalog and profile data come from [AutoEq](https://github.com/jaakkopasanen/AutoEq), created and maintained by **Jaakko Pasanen** and contributors. Maris pins commit `7ae0f56d53074872b028649617a22bbb4232feb7` and preserves the upstream MIT license and provenance records. See [provenance and notices](third-party.md) for the exact source and redistribution record.
