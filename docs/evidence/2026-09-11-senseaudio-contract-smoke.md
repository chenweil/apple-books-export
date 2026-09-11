# SenseAudio fixed-text contract smoke

This is the evidence/runbook for GitHub issue #20. It is deliberately separate
from the Speech domain implementation in #21.

## Scope and safety boundary

The tracer is [`scripts/senseaudio-smoke.py`](../../scripts/senseaudio-smoke.py).
It has no Apple Books, SQLite, CLI text, or user-text input. Its only text
inputs are fixed constants in the script. The API key is read only from
`SENSEAUDIO_API_KEY`; it is never written to evidence or output. The real run
is double opt-in:

```text
RUN_SENSEAUDIO_SMOKE=1
SENSEAUDIO_API_KEY=<secret>
```

`SENSEAUDIO_API_BASE_URL`, `SENSEAUDIO_OUTPUT_DIR`,
`SENSEAUDIO_EMOTION_VOICE_ID`, and `SENSEAUDIO_STYLE_VOICE_ID` are optional
non-secret test/output controls. Text cannot be overridden through the
environment. The base URL override is intended for the local provider mock;
the default endpoint is the SenseAudio API.

The tracer keeps the HTTP response in memory. It does not retain the raw JSON
response or the provider's audio hex. After strict hex decoding, it writes only
the MP3 artifact to a temporary run directory and retains evidence containing
hashes, lengths, status values, provider trace identifiers, and MP3 metadata.
It never invokes `afplay`; generated files remain available for explicit human
listening.

## Fixed smoke matrix

The matrix is fixed in the tracer and is identified in evidence without saving
the source text:

| ID | Purpose | Expected controls |
| --- | --- | --- |
| `zh-default` | fixed Chinese sample | default voice, speed 1, volume 1 |
| `en-default` | fixed English sample | default voice, speed 1, volume 1 |
| `emotion-variant` | catalog-selected documented candidate | exact resolved `voice_id` |
| `style-variant` | catalog-selected documented candidate | exact resolved `voice_id` |
| `speed-override` | fixed English sample | speed 1.25 |
| `volume-override` | fixed English sample | volume 1.5 |
| `control-safe` | literal break-token-shaped fixed sample | U+200B guard after ASCII `<` |

The default voice is the exact `male_0004_a` ID. Emotion/style candidates are
selected from explicit IDs recorded in the research and the [official SenseAudio
voice catalog](https://docs.senseaudio.cn/guides/voice/catalog);
the current style candidates include `male_0028_a` and `male_0028_b`. The
tracer does not infer meaning from an ID suffix and does not silently substitute
a random voice. Missing catalog capabilities leave the run `blocked`.

## Voice-list comparison

Every run sends both voice-list request shapes:

1. `{"voice_type":"all"}` — the shape required by the current schema.
2. `{}` — the shape shown by the documentation example.

The evidence records each HTTP/provider status, group counts, known fixed
candidate IDs, and an `observed_difference` value. It never stores voice names
or descriptions, which could contain account-specific text.

## Literal control-markup probe

The [official synchronous TTS documentation](https://docs.senseaudio.cn/api-reference/endpoint/tts/synthesize)
documents an ASCII break token but does not document a reliable escape syntax
in the repository research. The tracer therefore uses the
reversible, visible-text-preserving candidate transformation of inserting one
U+200B zero-width guard immediately after each ASCII `<`. The request is
asserted not to contain the provider's exact ASCII break-token shape, and the
evidence records both source/provider text hashes plus the transformation name.

This is not considered proven by the local mock alone. A real run followed by
human listening must confirm that the literal token remains ordinary content
and does not create an unintended pause. Until that happens, this acceptance
item remains `unverified`.

## Automated checks

The local provider contract test uses a loopback mock and a temporary MP3
created at test time; it does not use a real key or Apple Books data:

```bash
bash tests/senseaudio_contract.sh
```

It proves the double opt-in guard, no-key preflight, exact voice-list request
shapes, exact synchronous TTS payload keys, fixed audio settings, control-markup
guard, error-output redaction, strict MP3 evidence, and the live manual-listen
recording described below. The mock response audio hex exists only in process
memory while serving the response.

## Real-run procedure

Run from the repository root in one subshell. Keep the credential file outside
the repository and do not enable shell tracing:

```bash
(
  unset SENSEAUDIO_API_BASE_URL SENSEAUDIO_OUTPUT_DIR \
    SENSEAUDIO_EMOTION_VOICE_ID SENSEAUDIO_STYLE_VOICE_ID
  source /path/to/private-senseaudio.env >/dev/null 2>&1
  python3 scripts/senseaudio-smoke.py
)
```

Before showing the captured output, scan it and the generated `evidence.json`
for the credential, authorization material, source text, and response audio
markers. Only show the `Evidence:` and `Audio ...:` paths after that scan. The
script's final status is `awaiting_manual_listen` when all technical checks
pass; it is not an acceptance claim.

## Live evidence recorded on 2026-09-11

The double-opt-in live run used the default SenseAudio endpoint and the private
0600 credential file supplied for this task. The credential value was not
printed, copied, or retained. The captured output and generated `evidence.json`
passed the pre-display redaction scan.

Voice-list result:

| Request shape | HTTP | Accepted | Observed response |
| --- | ---: | --- | --- |
| `{"voice_type":"all"}` | 200 | yes | 34 `system_voice`, 0 cloned, 0 generated |
| `{}` | 200 | yes | no voice group arrays in this response |

The recorded difference is `both_requests_accepted`: the empty-object example
was accepted by this account, but its response shape differed from the
explicit request. The exact resolved IDs used by the matrix were
`male_0004_a` (default), `female_0033_b` (emotion candidate), and
`male_0028_a` (content-style candidate).

All seven fixed samples were generated and passed strict response, hex, MP3,
and metadata checks. The companion
[`2026-09-11-senseaudio-contract-live.json`](2026-09-11-senseaudio-contract-live.json)
records each sample's checksum, size, duration, provider trace identifier,
provider usage count, and local audio settings; the following table is the safe
run summary:

| ID | Voice | Speed | Volume | Usage | Duration | Size |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `zh-default` | `male_0004_a` | 1.0 | 1.0 | 26 | 4860 ms | 78089 B |
| `en-default` | `male_0004_a` | 1.0 | 1.0 | 59 | 5508 ms | 88457 B |
| `emotion-variant` | `female_0033_b` | 1.0 | 1.0 | 59 | 4068 ms | 65417 B |
| `style-variant` | `male_0028_a` | 1.0 | 1.0 | 59 | 4212 ms | 67721 B |
| `speed-override` | `male_0004_a` | 1.25 | 1.0 | 59 | 4896 ms | 78665 B |
| `volume-override` | `male_0004_a` | 1.0 | 1.5 | 59 | 6120 ms | 98249 B |
| `control-safe` | `male_0004_a` | 1.0 | 1.0 | 56 | 7128 ms | 114377 B |

Every local artifact reported `format=mp3`, `sample_rate=32000`,
`bitrate=128000`, and `channel=2`. The committed JSON stores only non-reversible
digests and provider trace identifiers, not audio bytes or raw response hex;
the full artifact paths remain in the temporary run evidence and task handoff.

The `control-safe` request recorded
`provider_text_contains_ascii_break_control=false` and
`provider_text_contains_zero_width_guard=true`. This proves the tracer sent
the transformed fixed sample rather than the provider's exact ASCII control
shape.

## Manual listening recorded on 2026-09-11

The user listened to all seven temporary MP3s on 2026-09-11. Every artifact was
playable. The emotion, style, speed, and volume variants each had a perceptible
difference from the fixed default sample. The guarded control sample had no
abnormal control-shaped pause.

| ID | Result | Non-sensitive observation |
| --- | --- | --- |
| `zh-default` | `pass` | Played successfully; the fixed Chinese sample was clear and free of playback artifacts. |
| `en-default` | `pass` | Played successfully; the fixed English sample was clear and free of playback artifacts. |
| `emotion-variant` | `pass` | Played successfully; the emotion variant had a perceptible difference from the default voice. |
| `style-variant` | `pass` | Played successfully; the style variant had a perceptible difference from the default voice. |
| `speed-override` | `pass` | Played successfully; the speed override was audibly faster than the default sample. |
| `volume-override` | `pass` | Played successfully; the volume override was audibly louder than the default sample. |
| `control-safe` | `pass` | Played successfully; no abnormal control-shaped pause was heard in the guarded sample. |

`comparison_performed=false`: no direct listening comparison used a real
provider break control. The conclusion for `control-safe` is therefore limited
to the documented U+200B guarded transformation and the absence of an abnormal
pause in that guarded artifact.

## Acceptance status for this checkout

Automated/local, live technical, and manual-listen evidence is recorded; the
checkout status is `verified_with_manual_listen`. The direct provider-control
comparison remains unperformed (`comparison_performed=false`), so this evidence
does not claim a side-by-side listening result for an actual provider break
control. The tracer intentionally cannot make that judgment or play audio
automatically. The temporary artifact paths are emitted by the safe live-run
output in the task handoff and are not copied into the repository.

## Final verification recorded on 2026-09-11

The following checks passed at the shared-worktree snapshot:

- `bash tests/senseaudio_contract.sh`
- `cargo test` — 120 tests passed across the existing CLI plus the concurrent
  profile worktree tests
- `cargo check --all-targets`
- `cargo build --release`
- `bash tests/headless_mainline.sh`
- `bash skills/apple-books-export-rust/tests/contract.sh`

No Cargo dependency or shared Rust interface was added for #20. The shared
worktree still contains separate uncommitted #21 paths; they are outside this
evidence's ownership boundary.
