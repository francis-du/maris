# Maris Studio: design before implementation

Status: design specification, not user acceptance or hardware validation. This document precedes the Studio implementation. It replaces the equal-card dashboard composition without removing the existing controls, optional details, localization, revision/undo behavior or explicit output confirmation.

## Identity and documentation site

The original Maris mark is one continuous rounded M-shaped audio wave. The same vector geometry is used in `docs/assets/mark.svg`, its monochrome counterpart and the wordmark. The colored mark uses restrained mint/periwinkle against graphite; the monochrome master is available for small/system surfaces. These are reusable SVG assets, not a font file or a copied vendor logo. Native application-icon adoption is a separate integration check, not implied by website use.

The documentation site uses a large real Studio illustration, concise listening workflow, separated correction/preference explanations, and a consistent same-page language selector. Markdown in the six locale directories is the customer-facing source; technical records remain one English source and are explicitly labeled. The root README is the product/install entry, not another copy of the engineering manual. The normal installation call to action downloads precompiled CI release assets, not a local source build.

`docs_assets.py` exports actual labeled offline Ratatui buffers, including settings drafts and scene previews, and binds images to source/buffer hashes. `docs_build.py` renders the bounded Markdown subset into the website; `docs_check.py` validates the output; `site.py` stages only owned generated output. No remote fonts, trackers, required JavaScript or fictitious product telemetry are introduced. Desktop/mobile actual browser review supplements link/locale/schema tests; none of these establishes audio hardware acceptance.

## Professional audio interface study

### FabFilter Pro-Q 4: the analyzer is the workspace

Primary reference: https://www.fabfilter.com/help/pro-q/using/overview
Official annotated interface: https://www.fabfilter.com/help_pages/pro-q/images/overview%402x.jpg

The EQ display fills the window. Selected-band controls appear next to the work, while presets, undo and A/B occupy the top edge and output metering sits at the right edge. The display is not one small card among equally prominent diagnostics.

Maris interpretation: use one large analyzer surface with a shared logarithmic frequency axis. Distinguish the measured pre-EQ spectrum from the calculated profile-EQ response with separate vertical scales and an explicit legend. Dock stereo output meters at the right edge. Do not imply that the profile EQ includes the additional device correction, dynamic processing or acoustic response. Keep those scope labels visible.

### Ableton Live: channel-strip consistency and level hierarchy

Primary reference: https://www.ableton.com/en/live-manual/12/mixing/

The mixer places metering, level and pan controls consistently within each channel. Its additional meter detail depends on the available height and width. The repeated structure makes controls predictable rather than requiring a new layout for each function.

Maris interpretation: ten primary listening parameters remain in one fixed control rail, with aligned names, right-aligned values and quiet range tracks. The selected parameter receives a pointer, stronger text and a track highlight. The rail does not move when the terminal resizes. It uses two rows per control when tall enough and one row when short. All ten controls remain reachable and visible when selected. Mixer values and meters are shown only when they come from the active mixer data plane; decorative activity is never substituted for telemetry.

### iZotope Ozone: macro controls and an assistant overview

Primary reference: https://www.izotope.com/products/ozone-advanced?tab=features
The official Master Assistant View groups key macro controls in an overview around large tonal-balance/loudness displays. Its assistant overview does not require reading a log of every internal processing parameter.

Maris interpretation: the main analyzer remains dominant. Music context and Listening Assist occupy two quiet supporting areas below it: evidence, goal, preview availability and a short reason. Model provenance stays in developer research documents, not the customer interface. Bundled weights and a working inference runtime are separate states. Empty semantics say Unknown rather than inventing genre or instruments. A tuning proposal still requires the existing deterministic apply path and explicit confirmation.

## What is being rejected

Do not reproduce the DAW's timeline, clip grid, decorative knobs or brand artwork. Maris is a system listening console, not a DAW. Do not divide the screen into nine equally framed status cards. Do not give decorative borders more contrast than the sound. Do not fill the header with repeated output names or fill the footer with every shortcut. Do not display a render fixture as live hardware evidence.

## Composition

The default is one screen, not four mandatory workspaces. Optional EQ, Apps, Device, Listening Assist and Health inspectors have visible direct entries; Tab remains optional compatibility navigation and Esc returns home.

Normal desktop composition, approximately 140 columns by 40 rows:

    MARIS  LIVE       current output                         reference / enhanced
    preset and device mode                           sample rate / safe gain

    DEVICE + SOUND      SPECTRUM / PROFILE EQ                      OUTPUT
    capability          one continuous frequency canvas           L   R
    correction          quiet grid, clear separate scales          meters
    physical limits     measured spectrum / calculated EQ

    ten controls        pre-EQ LUFS / true peak / RMS
    aligned values      MUSIC CONTEXT             AI TUNING
    quiet range tracks  evidence and confidence   preview + reason

    MIXER configuration summary                 SYSTEM HEALTH / revision
    contextual controls                         O output · P preset · J assist · help

The left rail uses 28-35 columns. The main analyzer/support surface receives the remaining width: normally more than 65% of the content width. Stereo meters are part of this surface, not separate dashboard cards. One horizontal separator divides lower evidence/support areas from the plot. Grouping uses shared background and spacing, not a rounded rectangle around every label.

## Visual system

Keep the existing dynamic palette tracker and its debounce. Backgrounds stay neutral charcoal. Primary text is near-white; units and auxiliary labels are quieter but legible. Cyan/aurora tones identify spectrum and live level evidence; the accent identifies the calculated profile curve and selected control. Warning color is reserved for actual warning states. A pointer, words and numbers accompany color: meaning must not depend on color alone.

Terminal cells do not support normal GUI font sizing. Use placement, weight, short labels, spacing and color roles for hierarchy. Use two columns of inner padding on broad surfaces, one row between groups and consistent value alignment. Long names are visibly ellipsized without moving the numeric columns. Dense technical identifiers belong in detail views.

## Interaction and capability preservation

O only opens a frozen output inventory. Arrow navigation must not route audio; Enter confirms through the current route controller. Space still compares the listening branches. All ten listening controls stay mapped to the same validated parameters; the default page may not silently omit Bass Assist, Dynamic EQ, Intensity, Balance or compression. Undo remains tied to the actual most recent edit, not merely the current cursor. Candidate download and semantic inference remain distinct. No DSP, callback, routing or OS-volume behavior changes are part of this visual refactor.

## Pointer and status implementation

The live Studio shell, header, rail rows and +/- hit targets share geometry from `studio_controls`; the configuration editor has separate read-only browser and draft-only edit geometry. Clicking a parameter selects it without changing sound; the selected row exposes decrement/increment targets that dispatch the same key actions and revision/undo transactions as the keyboard. The wheel changes selection only, and track dragging is intentionally inert. Header output and preset areas open the existing frozen pickers; Enter still confirms. The header A/B action uses the existing comparison path. Pointer input is ignored behind overlays, after a resize invalidates its rendered coordinates, and in compact mode. Inspector pointers use shared geometry; the fixed listening rail remains usable without leaving the inspector. The terminal restores mouse reporting on exit.

The second header line distinguishes `Applied`, `Pending audio update`, unknown application state, saved offline state and stale telemetry. Both applied revision counters must match before claiming application. LUFS/true-peak/RMS require a fresh analysis window matching the current sample rate, not merely an active engine. Output meters have a fixed digital scale and fixed warning zones; measured dynamic-EQ reduction is separate from its configured strength. Compact mode does not invent a missing peak or advertise the removed M-expand shortcut. Errors and offline-fixture notices occupy a dedicated footer row that shortcut wrapping cannot displace.

## Direct inspectors: shorten the path, not just restyle the pages

The previous detail screens were still unrelated full-page tables/cards. They also inherited the Dashboard cursor: opening EQ could require walking past sixteen unrelated controls before reaching a band. Those are rejected interaction patterns.

Keep the Studio shell and its device/listening rail in place. A quiet, always-visible action row opens one in-place inspector in the right work area: E EQ, M Apps, V Device, I Assist and H Health. Esc returns to the unexpanded Studio in one action. Clicking a section title is the equivalent direct entry. Tab/Shift+Tab remain optional compatibility navigation, never a required route. Inspectors can be switched directly without visiting home. Overlay confirmation continues to block all underlying navigation and changes.

Sound settings opens on its remembered band (the first band on first use), not Bass. The earlier live band-strip/mixed-control grid is replaced by the configuration editor described below: a read-only grouped browser, fixed Current/Draft station and Apply/Cancel targets. All original controls remain reachable, but list rows never edit sound. The Dashboard cursor and settings cursor are remembered separately.

Apps presents application names first, selection and active routing separately, plus a short source-to-DSP-to-output summary. Clicking a checkbox changes only the pending selection. The Apply button/Enter is the only route-changing step. Mixer buses remain configuration-only when the hardware data plane is unavailable. Device, Listening Assist and Health use the same quiet headings, aligned fields and contextual action row, not giant empty framed cards or dumps of raw runtime JSON.

Acceptance paths: home to any inspector is one visible click or one key; a first-band configuration change is E, +, Enter; any visible browser row is one read-only click; home to output picker remains O. Esc cancels a pending settings draft, otherwise it returns home. Tests must exercise the actual navigation reducer and shared pointer geometry, not only search for headings. Inspect all five inspectors at 100x32 and 140x40 using the actual Ratatui buffer, including unavailable data and pending application selection. This change does not claim new DSP, model inference or physical routing capability.

## Useful assistance, not a model catalog

Listening Assist is a task surface: choose Balanced, Warm, Clear or Soft; preview the final constrained delta; confirm; compare; undo. Clicking a goal creates a preview only. The three primary buttons are Preview, Compare and Undo. Actual signal readiness and processing state replace a list of research checkpoints. Ordinary music enhancement requires no model download. Without a verified semantic backend, show measured bass/treble/crest/correlation evidence instead of empty genre/instrument/vocal placeholders. The already embedded speech network remains visibly speech-only; no candidate receives a usable or enhancement claim from cache presence.

The same fresh-evidence gate drives both readiness and live planning. A no-op cannot consume the user's last meaningful undo action. All ten primary controls, original EQ controls and explicit output confirmation remain intact.

## Responsive rules

At 140x40 and 180x50 the control rail uses two-line parameter treatments when space allows. At 100x32 and 120x36 it compresses controls without changing their order or hiding the selected value. The analyzer remains the largest region, and context, Listening Assist and mixer/health summaries stay on the default page. Below the current compact threshold, use the existing truthful compact monitor with working confirmation overlays; never allow unseen controls to change audio.

## Locales and scenario selection

Language coverage includes the values, messages and confirmations, not just section headings. Known route/device enums are presented through explicit label mappings; user output names and stable IDs are never translated. Named notification templates render in the current language, and saved locale changes propagate to open UI processes. Unknown OS diagnostic causes remain readable as original technical details. Keep the notice on its own footer row in every locale.

The existing P picker begins with ten practical listening scenes and then the legacy EQ curves. Scene and Tone curve are visually different categories; the list stays flat. A scene selection displays its localized purpose, correction-preservation notice, explicit compression warning and device-constrained deltas before Enter applies it. Merely opening or moving in the picker has no audio/state effect. Esc cancels and U uses the transaction that was actually changed. Do not add a scene tab or a new multi-step setup wizard.

Use cell-aware fitting for CJK text and longer German/Spanish labels. Verify status words, goals, preset names and confirmation instructions in addition to headings. `cargo run --locked --example ui_probe -- --locales` exports actual offline cell buffers for each language; `tests/console_locale.rs` checks the same presentation boundaries without hardware capture.

## Menu Bar quick controls

The native menu is a small listening remote, not another dashboard. Its top rows distinguish actual output, current-device listening settings and the separate global EQ curve. A pending callback revision must remain Pending, not Applied. A preset label is derived from matching actual device-constrained parameters, not the global EQ name; custom settings remain Custom.

Output and listening presets are reachable directly from the root. Presets have quiet section headings in one submenu rather than nested category menus. Choosing an output or preset stages one pending change without touching sound. The root has a named Apply action, Cancel and a read-only detail submenu with the target device, final changes and compression/correction warnings. A fresh explicit Apply rechecks session identity, device UID/binding/rebind counter, sample rate and preference/capability revisions; expired or changed context requires a new preview. Output entries retain selector identity rather than transient row indices.

Compare A/B and Undo are root actions. A/B shares the listening validation/revision path but does not overwrite tonal undo history. Current-device Undo may not change another device or the global EQ. Language selection is direct in the menu and uses the same persisted preference watcher. Errors have a dedicated retained message instead of being overwritten by the next profile refresh. Offline, stale or unsupported sessions disable quick audio actions rather than silently editing a default profile or starting capture.

Pending Apply/Cancel/Details rows are inserted only when needed; the idle menu should not look like an unconfigured wizard. Immediate Compare, Undo and processing-bypass commands are checked against the state that produced the visible labels, not a new unseen output. Global bypass is labeled as processing bypass with safety gain retained, because the existing DSP also forces the music reference branch. A/B and scene apply are disabled while that global bypass makes them inaudible. The no-makeup-gain compression warning has a separate short localized line so a long scene description cannot truncate the warning.

Configuration confirmation is context-bound, not just an Enter key binding. O/P dialogs freeze revisions and device binding; stale context invalidates the selection but leaves the modal shield in place until cancellation. A held/queued confirmation cannot leak to the app-scope screen underneath. Discrete repeat events and rapid duplicate key bursts are filtered; continuous arrow adjustments remain usable. No-op parameter writes preserve the user's previous useful undo. Distinct route requests from two UI surfaces cannot silently replace one another, and absent output fields must never mean default output.

A displayed OFF amount is zero for step interaction: Minus leaves it off and Plus starts at one 10% step, never a hidden saved amount. An actual nonzero change may audition the enhanced branch; a no-op must not. Unsupported Bass Assist activation reports a localized reason. No-op EQ/listening controls and reapplying the same preset must retain the last actual undo domain, not choose a domain based on cursor position. Undoing a target device's tonal change retains all other devices' current A/B selections.

No native menu review may start/stop capture or replace a running process. Test the controller against temporary state and exercise native item construction separately; neither is a headphone listening certification.

## Configuration editor redesign

This replaces the Sound inspector's dense immediately-live controls, not just its validators. The default Studio remains the listening overview. E opens one focused Sound settings editor with a read-only parameter browser on the left and a single fixed edit station on the right. The browser groups This output separately from Global EQ; the scope and exact target stay above the editor. No +/- targets exist inside browser rows. Arrows and wheel only select. +/- (or the separated edit-station buttons) stage a local draft; [/] stages band Q. Space never toggles a hidden bypass or comparison in this screen.

The right station shows Current and Draft side by side, a labeled calculated EQ preview, and a visible change summary. When browser scrolling clips a group heading, its scope remains pinned above the visible rows. Pending draft state replaces the ambiguous Applied badge in the header, and the footer no longer advertises blocked output/preset/undo shortcuts. Apply uses the primary filled accent; Cancel remains quieter and spatially separated. Apply and Cancel stay in fixed, widely separated bottom targets. Enter applies the displayed draft once; Esc discards it without writing. One draft covers one existing transaction namespace, preventing partial two-library commits. Switching scopes with pending edits requires applying or cancelling first. Navigation, output/preset selection, A/B and Undo cannot silently discard or act around a pending draft. Stop remains available. No modal wizard is added.

Drafts never enter the audio update queue. Explicit Apply validates the captured endpoint, rate, capability and revisions, compiles through the existing DSP safety path, then uses the ordinary revision/undo transaction. An external edit, hotplug or stale telemetry permanently invalidates that draft until cancellation; it is not silently rebased. Missing data disables application. Saved versus callback-applied status remains separate. Compact mode cannot apply a hidden draft and keeps a visible cancel instruction.

Acceptance: E opens the editor; first band adjustment is E, +, Enter. Clicking any browser row, any arrow navigation or wheel movement has zero persistence/audio effect. Several same-scope changes apply as one undoable transaction. Test the real event reducer and actual hit geometry, including cancellation, no-ops, scope conflict, stale context, drag, double-confirmation and resize. Review actual buffers at 100x32, 140x40 and 180x50 with both a clean and a pending draft in all six locales. This supersedes the earlier E-then-Right immediate-edit path.

## Stable configuration interactions

The configuration browser now has three fixed, one-action selectors: Device sound, Global EQ and Playback switches. These are local sections in the same edit surface, not mandatory application tabs. Every section remains directly accessible; all 26 existing row IDs remain supported. Navigation stays inside the chosen section. Clicking a visible row must not recenter the list: use stable bounded pages so repeated clicks address the same parameter.

Playback switches are not numeric gain controls. Show explicit OFF/ON actions, the affected output scope, and short warnings about correction, comparison level or global bypass. The configuration header cannot perform a live A/B change. Editing tone preserves the enabled/reference flags even after Apply; do not import the quick-listening audition behavior into a configuration transaction.

Only a complete press/release on the same Apply button and same draft can commit via mouse. A drag, resize, keyboard event or revised draft cancels the incomplete gesture. Pressing a parameter row or an edit button cannot be interpreted as Apply. The fixed bottom Apply/Cancel targets remain separated; retain the original keyboard Enter path and single-transaction undo. A one-click section change is read-only and blocked when it would abandon a pending draft.

## Delivery is part of interaction

A valid button cannot appear inert because motion events occupy the input queue or because the previous successful action has not reached a timer-driven label refresh. Coalesce only motion; preserve all physical edges and matched draft confirmation. Acknowledge only a controller's own successful revision, never blindly adopt an external edit. Actual pseudo-terminal input tests complement geometry tests.

A native menu closes after selection. On macOS/Windows, open the existing preview as a native dialog immediately, with Cancel as its default and explicit Apply as the only commit action. Do not require reopening the menu to find newly inserted controls. Linux keeps the visible in-menu fallback. Native callbacks wake the main loop and never perform state mutations themselves. The offline menu probe exercises production native-ID dispatch but does not pretend that injected dialog responses are a user acceptance test.

## Review gates

1. Render the actual Ratatui buffer, not an imagined mockup. Export its colors and cell positions for visual review.
2. Inspect at least 100x32, 140x40 and 180x50; also check compact mode, no signal, a long device name and the output confirmation dialog.
3. For an offline active-looking review, obtain spectrum/statistics from deterministic generated audio and label every image as an offline fixture. Never start capture, route the user's audio or change OS volume to get a screenshot.
4. Test layout containment, analyzer dominance, all-control reachability, no synthetic live values, label scope and overlay safety. A heading-substring test alone is insufficient.
5. Run focused UI/localization tests, formatting and Clippy. Run the broader regression suite before claiming the refactor is ready. Record failures rather than weakening assertions to hide functional regressions.

Visual review is an engineering check, not a claim that the user likes the design. No number of repeated tests substitutes for that distinction.
