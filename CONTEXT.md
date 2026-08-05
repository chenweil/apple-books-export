# Domain Glossary

## Annotation

A reading record imported from Apple Books. An Annotation can contain a highlighted passage, a personal note, or both.

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

Share Cards use the selected bundled or system font. The reader may keep the default automatic size or choose a fixed readable size; after automatic sizing reaches the minimum readable size, long text also paginates instead of shrinking or truncating. Primary text supports independent horizontal and vertical alignment. Line height is fixed at 1.4 times the font size for measurement, pagination, and drawing. Supplementary notes inherit the selected font, palette, and alignment; their size is `max(30, primaryFontSize × 0.6)`, and overflow notes use the full text-safe region on continuation pages. Attribution remains template-positioned.

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
