# Domain Glossary

## Annotation

A reading record imported from Apple Books. An Annotation can contain a highlighted passage, a personal note, or both.

## Share Card

A single-image representation of one Annotation for saving or sharing. Its primary content is the highlighted passage. When the Annotation has a personal note, the note is optional supplementary content that the reader can hide.

## Background Candidate

One locally generated visual treatment offered for a Share Card. The first release creates four candidates from the selected theme colour without uploading the Annotation. AI-generated imagery is a future optional enhancement, not part of the initial card flow.

## Card Format

The initial Share Card export is a 3:4 portrait PNG, selected for social sharing. Square and landscape formats are deferred.

## Attribution

Share Cards show the book title and author in their footer. If an author is unavailable, the footer shows only the title and does not reserve an empty author field.

## Typography

Share Cards use readable system Chinese typography. The book title may use a stronger weight, but the initial format uses no decorative font system.

## Export

The initial export is a PNG named from the book title and author. A successful export presents a confirmation. It does not open the containing folder by default.

## Open Export Folder Setting

A system setting that lets the reader opt into opening the exported image's containing folder after a successful export. It is off by default.

## Theme Colour

The initial Share Card editor offers six curated theme colours. Arbitrary custom colour selection is deferred so the initial themes can preserve text contrast.

## Default Export

When the reader accepts the default Share Card choices, export begins immediately without a confirmation dialog. Completion is communicated with a lightweight success notice.

## Save and Share

The primary completion action saves the selected Share Card as a PNG. After a successful save, the reader may invoke the system share action. Saving remains the primary action so the image is retained independently of a sharing destination.

## Long Passage Handling

The initial Share Card canvas is 1200 by 1600 pixels. Main text may shrink only to a provisional 42-pixel minimum. If the content still does not fit, the system creates consecutive cards instead of truncating the passage. The minimum remains subject to readability validation during implementation.

## Share Card Surface

Share Cards belong to the current AppKit application. Older card interfaces on other product versions are outside this feature's scope.

## Card Entry

The Share Card action is contextual. It appears beside an Annotation only after the reader selects that Annotation, then opens the card editor for that one record. The action is not persistently shown on every list row.

## Background Generation

The card editor opens with a default theme preview. It creates four Background Candidates only when the reader explicitly requests generation, rather than generating them on every editor open.

## Background Style

Background Candidates use restrained, low-contrast texture or decoration. The six theme colours are selected to support the reading text rather than compete with it; text remains the dominant visual element.

## Card-only Text Edit

The reader may temporarily edit the highlighted passage or supplementary note in the Share Card editor. This changes only the exported card and never changes the source Annotation.

## Progressive Card Editing

The card editor opens with a ready-to-export default. Theme colour, generated Background Candidates, and Card-only Text Edits are secondary actions that remain available without requiring choices before export.

## Card Template

An internal, ready-to-use combination of layout and restrained Background Style. The system selects a suitable default Card Template so the reader does not have to assemble card options. The reader can quickly choose an alternative only when wanted.

## Alternative Cards

The “change it up” action presents four complete Card Templates, each combining a background, restrained decoration, and layout variation. It is not limited to swapping a background image.

## Default Card Content

When an Annotation includes both a highlighted passage and a note, the initial Share Card displays only the highlighted passage. The reader may explicitly add the note as supplementary content.

## Note-only Card

An Annotation with a note but no highlighted passage can still produce a Share Card. Its note becomes the primary text, using the same contextual Card Entry as a highlighted passage.
