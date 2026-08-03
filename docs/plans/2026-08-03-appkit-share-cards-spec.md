## Problem Statement

Readers can browse Apple Books Annotations and export Markdown, but cannot turn a meaningful Highlight or note into a polished image for saving or sharing. Existing card work belongs to another product version and must not determine the current AppKit experience. A reader should get a readable, attractive Share Card with almost no design decisions.

## Solution

Add Share Cards to the AppKit application. Selecting an Annotation reveals a contextual Card Entry. The editor opens with a ready-to-export 3:4 card, using a restrained internal Card Template. It supports a Highlight as primary text, a note-only Annotation, optional supplementary notes, curated themes, and on-demand Alternative Cards. The reader saves PNG output first and may then use system sharing.

## User Stories

1. As a reader, I want a Card Entry to appear only after I select an Annotation, so that the annotation list stays uncluttered.
2. As a reader, I want to create a Share Card from a Highlight, so that I can share a passage I value.
3. As a reader, I want to create a Share Card from a note-only Annotation, so that my own writing is shareable even without a Highlight.
4. As a reader, I want the default card to use the Highlight as its primary text, so that I can export immediately without making content choices.
5. As a reader, I want to optionally include the note attached to a Highlight, so that I can add my interpretation when I want it.
6. As a reader, I want the card footer to show the book title and author when available, so that the passage retains its attribution.
7. As a reader, I want author-less books to show only their title, so that the card never contains a misleading empty field.
8. As a reader, I want a ready-to-export default Card Template, so that I do not have to understand visual design controls.
9. As a reader, I want six calm curated themes, so that I can change the visual mood without risking unreadable text.
10. As a reader, I want to request Alternative Cards only when I choose, so that opening the editor remains quick and simple.
11. As a reader, I want four complete Alternative Cards at a time, so that I can choose a finished option rather than assemble background, texture, and layout separately.
12. As a reader, I want subtle low-contrast texture and decoration, so that the card feels designed without competing with the text.
13. As a reader, I want temporary edits to card text, so that I can tailor a particular image without changing the original Annotation.
14. As a reader, I want long passages to remain intact across consecutive cards, so that text is never silently truncated.
15. As a reader, I want a single 3:4 PNG export, so that the result fits common social-sharing formats.
16. As a reader, I want a saved image named from the book title and author, so that I can find it later.
17. As a reader, I want an unobtrusive success message after saving, so that export does not interrupt my flow.
18. As a reader, I want system sharing after an image is saved, so that I can choose the appropriate destination.
19. As a reader, I want opening the export folder to be optional and off by default, so that the application does not unexpectedly change my workspace.

## Implementation Decisions

- Share Cards are an AppKit-only feature, per ADR-0002; prior card interfaces on other product versions are not reused.
- An Annotation is the source record. A Highlight takes precedence as primary card text; a note-only Annotation promotes its note to primary text.
- A Share Card has one ready default and progressive disclosure for optional text edits, note inclusion, theme changes, and Alternative Cards.
- A Card Template is an internal complete composition, not merely a background. Alternative Cards return four complete templates.
- The first release uses six local, bundled Background Candidates with restrained low-contrast visual treatment. It does not call a network image-generation service or upload Annotation text.
- The initial output is a 1200 by 1600 PNG. Main text can shrink only to a provisional 42-pixel minimum, then becomes a consecutive-card sequence; readability validation may revise the threshold.
- PNG saving is the primary completion path. A system-share action becomes available after successful saving. The export-folder preference defaults to off.
- A single public card-generation-and-export service is the highest feature seam. The AppKit view layer requests it from a selected Book and Annotation; rendering, pagination, naming, and export results stay behind that service boundary.

## Testing Decisions

- Test externally observable Share Card behavior, not AppKit subview layout or private rendering steps.
- Cover the public card-generation-and-export service with a Highlight, a note-only Annotation, and an Annotation containing both a Highlight and a note.
- Verify default content selection, attribution fallback, four Alternative Cards, card-only text edits, output naming, valid PNG export, and multi-card output when the minimum readable font boundary is exceeded.
- Verify the export-folder preference controls reveal behavior without changing the saved image result.
- Use focused AppKit/package verification and generated-image inspection for visual readability; add tests at the public service seam rather than coupling tests to table-cell internals.

## Out of Scope

- Square and landscape cards.
- Arbitrary custom colours.
- Cloud or AI-generated imagery.
- Modifying source Apple Books Annotations from the card editor.
- Reusing or maintaining older non-AppKit card interfaces.
- AI question, explanation, or continuation features based on Highlights, notes, or source-text context.

## Further Notes

Six prototype background assets are available in the project as mist wash, sage leaf, blush arcs, sand contours, lavender stars, and stone textile. They are a visual starting set for the internal Card Templates. The specification uses the vocabulary defined in `CONTEXT.md` and respects ADR-0002.
