# Documentation

Edit the Markdown and language resources here. `scripts/site.py` generates the website in `_site/`; generated HTML is not committed.

## Where to start

| Task | Page |
| --- | --- |
| Install or upgrade | [Installation](en/install.md) |
| Use presets, settings and Menu Bar | [User guide](en/guide.md) |
| Use CLI or MCP | [Command reference](reference/commands.md) |
| Change the implementation | [Architecture](reference/architecture.md) and [interface design](development/ui-design.md) |
| Run tests or prepare a release | [Audio tests](reference/audio-quality.md) and [build procedure](development/releasing.md) |
| Review dependencies or models | [Third-party components](reference/third-party.md) and [model review](development/models.md) |
| Review remaining product work | [Release checklist](development/product-gates.md) |

Product, user-guide, installation and status pages exist in `en`, `zh-CN`, `zh-TW`, `ja`, `de` and `es`. Engineering references have one English source; the website labels them as English. The language menu stays on the same page.

## Writing

Describe the task, the steps and the result. Use the names shown in the application. Explain a necessary technical term once. Avoid slogans, invented terminology, progress-report prose and repeated claims about quality or safety. State a limitation where it affects the reader's next action, then link to the detailed reference.

Keep each topic in its owning page. Installation owns prerequisites and recovery; the user guide owns interaction; the command reference owns syntax; architecture owns module responsibilities; audio tests own regression evidence. Do not copy the same explanation into each file.

Each page starts with one H1. The renderer supports H2–H4, explicit heading anchors, paragraphs, code fences, lists, tables, blockquotes, inline emphasis/code/links and standalone SVG illustrations. Raw HTML is escaped on the website. Missing local targets, missing translations and stale screenshots fail validation.

`locales/<language>.json` holds navigation, captions and shared labels. Commands, JSON keys and user-supplied names are not translated. Installation examples are checked against README so the website and repository instructions stay consistent. The built site copies the root installers byte-for-byte to its short download URLs.

## Preview and check

```sh
python3 scripts/site.py --check
python3 scripts/site.py
python3 -m http.server 8000 --directory _site
```

On macOS, `swift tests/support/site_probe.swift` checks the generated pages in WebKit and writes local screenshots under `.maris-review/`. Review desktop and narrow layouts. Automated structure checks are not a complete accessibility audit.

## Images and logo

The SVG logo, monochrome mark and wordmark are in `assets/`. Interface illustrations are exported from actual Ratatui buffers using generated audio. Their captions identify them as offline images; they do not show a hardware listening test. Image hashes and source hashes are recorded in `assets/illustrations.json`.

```sh
python3 scripts/docs_assets.py --refresh
```

Refresh runs the offline UI probe. Normal website generation does not run Maris, access audio devices, load models or download fonts. No font files are distributed.

## Release status

`development/requirements.json` supplies the status table and release checks. Keep unfinished requirements until they have acceptance evidence. Source publication and Pages deployment do not approve an application release.

The document structure draws on [Diátaxis](https://diataxis.fr/), [W3C page structure](https://www.w3.org/WAI/tutorials/page-structure/) and [HTML language declarations](https://www.w3.org/International/questions/qa-html-language-declarations).
