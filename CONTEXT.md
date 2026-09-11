# Domain Glossary

## Annotation

A reading record imported from Apple Books. An Annotation can contain a highlighted passage, a personal note, or both.

## Speech Clip

A generated reading of exactly one content part of an Annotation: either its highlighted passage or its personal note. Playing two Speech Clips in sequence does not turn them into one combined clip.

## Voice Profile

The reader's constrained choice of voice, supported emotion or style, speaking speed, volume, and other available speech controls. A Voice Profile must resolve to a combination the selected speech provider can actually produce.

## Unverified Voice Profile

A locally valid Voice Profile whose voice availability could not be checked against a current Voice Catalog. It may be saved for later but cannot authorize a Speech Attempt until generation-time validation succeeds.

## Speech Provider

An external service that turns selected Annotation content into a Speech Clip. Each Speech Provider owns its available voices and control vocabulary; the product does not promise that a Voice Profile is portable between providers.

## Speech Text

The exact highlighted passage or personal note chosen for a Speech Clip after line-ending and boundary-whitespace normalization. It remains ordinary content rather than provider control markup and is not automatically edited, translated, summarized, or paraphrased.

## Voice Catalog

The selected Speech Provider's current set of available voices and their supported emotion or style labels. A Voice Catalog constrains Voice Profiles for that provider; it is not a cross-provider vocabulary.

## Speech Receipt

The machine-readable record of a Speech Clip generation or export outcome. It identifies the source, resolved Voice Profile, cache behavior, audio artifact, and provider trace without repeating the Annotation text or embedding audio bytes.

## Unknown Speech Result

A speech generation attempt that may have reached the Speech Provider but did not return enough evidence to accept a Cached Speech Clip. It is neither a successful clip nor a safe signal for automatic retry.

## Speech Attempt

One request to a Speech Provider to generate a logical Speech Clip. Regenerating the same Speech Clip creates a new Speech Attempt without changing the clip's identity.

## Speech Cache Entry

The atomically accepted audio and metadata for one Cached Speech Clip. Partial, corrupt, or unverifiable files are not Speech Cache Entries.

## Speech Cache Rehydration

The local reconstruction of a Speech Cache Entry from a checksum-matching Exported Speech Clip. It does not contact a Speech Provider or create a Speech Attempt.

## Speech Attempt History

A time-bounded, metadata-only record of Speech Attempts used for billing and provider diagnosis. It excludes Annotation text, API keys, and audio bytes and has a lifecycle separate from the audio cache.

## Cached Speech Clip

A reusable local Speech Clip retained for preview and repeated playback. It remains application-managed and is not yet a user-owned exported file.

## Exported Speech Clip

A durable, user-owned copy created from a Cached Speech Clip for use outside the application, including linking from exported reading notes.

## Active Exported Speech Clip

The one Exported Speech Clip currently selected for a particular Annotation content part's generated note link. Older exported variants may remain user-owned files without being linked automatically.

## Speech Export Manifest

A provider-neutral record inside one exported book directory that identifies its Exported Speech Clips and the active clip for each Annotation content part. It uses relative paths so the exported book remains self-contained when moved.

## Share Card

A fixed 3:4 visual representation of one Annotation. A short Annotation produces one Card Page; a long Annotation produces a Card Sequence whose pages can be previewed and exported together. Its primary content is the highlighted passage, while a personal note is optional supplementary content.

## Card Page

One fixed-size 1200 by 1600 PNG page in a Share Card Sequence. The reader can select a page for preview, copying, or AirDrop without changing the full sequence saved to disk.

## Background Candidate

One locally bundled background treatment offered as part of a complete Card Template. The initial pool contains twelve backgrounds: six existing assets and six original assets inspired by broad visual references without copying their pixels, text, or composition. Four candidates are shown at a time without uploading the Annotation.

## Card Format

Share Card output is always a 3:4 portrait PNG at 1200 by 1600 pixels. The editor may scale the preview on screen, but the exported image size is fixed; square and landscape formats are deferred.

## Attribution

Share Cards show the book title and author in their footer. If an author is unavailable, the footer shows only the title and does not reserve an empty author field.

## Typography

Share Cards use the selected bundled or system font. The reader may keep the default automatic size or choose a fixed readable size; after automatic sizing reaches the minimum readable size, long text also paginates instead of shrinking or truncating. Primary text supports independent horizontal and vertical alignment. Line height is fixed at 1.4 times the font size for measurement, pagination, and drawing. Supplementary notes inherit the selected font, primary text color, and alignment; their size is `max(30, primaryFontSize × 0.6)`, use a light oblique emphasis and a thin divider from the same text color, and overflow notes use the full text-safe region on continuation pages. Attribution remains template-positioned.

## Export

The export action writes every Card Page as a PNG named from the book title and author. A successful export presents a confirmation. It does not open the containing folder by default.

## Open Export Folder Setting

A system setting that lets the reader opt into opening the exported image's containing folder after a successful export. It is off by default.

## Theme Palette

Each Card Template combines a background with a readable text palette. The default palette uses near-black text over the background; additional palettes use high-contrast deep colours such as brown, green, blue, or purple. Arbitrary custom colour selection is deferred so templates can preserve contrast.

## Default Export

When the reader accepts the default Share Card choices, export begins immediately without a confirmation dialog. Completion is communicated with a lightweight success notice.

## Card Actions

The primary completion action saves all Card Pages as PNG files. The copy action copies the selected page by default and offers copying all pages as a secondary menu action. AirDrop occupies the former generic share-button position, generates a temporary PNG for the selected page, and sends it without requiring a prior save. When AirDrop is unavailable the stable button remains visible but disabled. The generic macOS sharing panel is not part of this surface.

## Long Passage Handling

The Share Card canvas is fixed at 1200 by 1600 pixels. Automatic sizing keeps the existing readable fitting behavior and paginates after reaching the minimum readable size; a manually selected size is preserved. If the content still does not fit, the system creates consecutive Card Pages instead of truncating the passage. A supplementary note uses its note region on the page carrying primary text, then uses the full text-safe region on note-only continuation pages. When settings reduce the sequence length, the selected page is retained when valid and clamped to the last page otherwise. The editor shows one large selected preview plus a bounded thumbnail strip.

## Share Card Surface

Share Cards belong to the current AppKit application. Older card interfaces on other product versions are outside this feature's scope.

## Card Entry

The Share Card action is contextual. It appears beside an Annotation only after the reader selects that Annotation, then opens the card editor for that one record. The action is not persistently shown on every list row.

## Background Generation

The card editor opens with a default template preview. The theme panel shows all twelve bundled backgrounds as bounded thumbnails. A Card Template is an atomic background-plus-palette choice. “Change it up” rotates through the canonical twelve-template order with a persistent cursor, shows the next four templates after the cursor while skipping the current template, and advances the cursor by four modulo twelve after each request. Selecting a candidate does not reset the cursor or user typography.
The six backgrounds added for this iteration have per-asset provenance, production constraints, hashes, palette colors, and safe-area records in `docs/assets/share-card-backgrounds/SOURCES.md`.

## Background Style

Background Candidates use restrained texture, paper, wash, line, or collage-inspired decoration. Each template declares a text-safe region and attribution region so alignment changes do not place text over important decoration. Text remains the dominant visual element.

## Text Safe Area

The rectangular region in a Card Template where primary text and supplementary notes may be aligned. Horizontal and vertical alignment operate inside this region; the background asset must keep its main decoration outside the region or at a contrast-safe level.

## Card-only Text Edit

The reader may temporarily edit the highlighted passage or supplementary note in the Share Card editor. This changes only the exported card and never changes the source Annotation.

## Progressive Card Editing

The card editor opens with a ready-to-export default. Theme colour, generated Background Candidates, and Card-only Text Edits are secondary actions that remain available without requiring choices before export.

## Card Template

An internal, ready-to-use atomic combination of one background, one text palette, a text-safe region, an attribution region, and restrained decoration. Selecting a different template preserves the reader's font, size mode, size, and alignment; it changes only the visual treatment and template-owned layout. The old variant-based safe-area mechanism is not part of the template contract.

## Alternative Cards

The “change it up” action presents four complete Card Templates selected by the persistent cursor rule from the canonical twelve-template pool. It is not limited to swapping a background image, and it does not overwrite user typography choices.

## Default Card Content

When an Annotation includes both a highlighted passage and a note, the initial Share Card displays only the highlighted passage. The reader may explicitly add the note as supplementary content.

## Note-only Card

An Annotation with a note but no highlighted passage can still produce a Share Card. Its note becomes the primary text, using the same contextual Card Entry as a highlighted passage.

## Version Discovery

Version Discovery is the AppKit application's check of official stable release metadata to tell the reader when a newer Compatible Release exists. It does not download or install the application.

## Manual Update

A Manual Update begins when the reader chooses to view the official release page, then downloads and installs the release themselves. It is separate from Version Discovery and is the only update path currently supported.

## Stable Channel

The Stable Channel contains public, non-prerelease releases intended for general use. Version Discovery ignores beta, nightly, and other prerelease versions.

## Compatible Release

A Compatible Release is a Stable Channel release that meets the reader's minimum macOS version and CPU architecture. Incompatible releases are not presented as available updates.

## Headless Mainline

The Headless Mainline is the `main` product surface without a shipped GUI. It provides the Rust CLI, the read-only TUI, and the Agent Data Skill. The deprecated Tauri GUI may remain in source history during migration, but it is not the default product entry or release target.

## Canonical Rust Data Core

The Canonical Rust Data Core is the single source of truth for reading Apple Books data, normalizing books and annotations, applying selection identity, and producing export results. Human CLI, TUI, Agent Data Skill, and the future AppKit GUI consume its contracts rather than maintaining separate database rules.

## Machine JSON Protocol

The Machine JSON Protocol is the versioned, structured boundary between the Canonical Rust Data Core and non-human consumers. Successful results are machine-readable, unsupported schema versions fail explicitly, and diagnostics use stable error codes rather than exposing SQLite implementation details.

## Stable Asset Identity

Stable Asset Identity is the Apple Books `asset_id` used by machine consumers to refer to a book across refreshed lists and changing display order. Human-facing commands may continue to accept a display index, but an index is not a durable identity.

## Read-only TUI Surface

The Read-only TUI Surface is the terminal experience for searching and browsing books and annotation details. It does not modify Apple Books, export files, invoke AI operations, or replace the GUI.

## Agent Data Skill

The Agent Data Skill is the repository-managed workflow that lets an AI agent list, select, inspect, and export Apple Books annotations through the Rust CLI. It is local-first, verifies its executable and output, and does not silently invoke AI, modify Apple Books, or upload note content.

## AppKit GUI Surface

The AppKit GUI Surface is the future official macOS graphical experience. It remains a separate implementation during migration, then consumes the Canonical Rust Data Core through the Machine JSON Protocol before it is merged into the Headless Mainline.

## Tauri Legacy GUI

The Tauri Legacy GUI is the existing Rust/Tauri graphical surface that is no longer the target product entry. Its source is retained for rollback and historical comparison until the Cutover Gate is satisfied; it is not expanded as part of the migration.

## Cutover Gate

The Cutover Gate is the set of acceptance conditions for making AppKit the official GUI and deleting the Tauri Legacy GUI: stable CLI contracts, working TUI and Agent Data Skill, AppKit integration with Rust, permission and packaging verification, real macOS smoke evidence, and an explicit capability-gap decision.

## Local Data Boundary

The Local Data Boundary means that listing, reading, browsing, and Markdown export keep Apple Books content on the local machine. Network-backed AI enrichment or other remote operations require a separate, explicit future decision and are not implied by the Agent Data Skill.
