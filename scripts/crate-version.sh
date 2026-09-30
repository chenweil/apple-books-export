#!/usr/bin/env bash
#
# Print the version the crate declares in [package] of Cargo.toml.
#
# One implementation, called from every place that needs the crate version: the
# release tag check, the AppKit packaging script's default, and the headless
# guard. Two readers agreeing by accident is exactly the failure this exists to
# remove -- an earlier version of that guard carried its own positional regex
# and was reading [workspace.package] while this read [package].
#
# Usage: crate-version.sh [path/to/Cargo.toml]   (defaults to the repo's own)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
MANIFEST="${1:-$REPO_DIR/Cargo.toml}"

if ! command -v python3 >/dev/null 2>&1; then
  printf 'python3 is required to read the crate version\n' >&2
  exit 1
fi

# Read the [package] version with a real TOML parser rather than a positional
# regex. A regex for the first `version = "..."` inverts silently the moment the
# manifest is restructured -- a [workspace.package] block or a table placed
# above [package] would be read as the crate's own version, and every caller
# would then agree on the wrong answer. tomllib is stdlib from 3.11; older
# interpreters fall back to a section-aware line scan rather than failing, so
# the release does not become uncuttable on an older runner.
python3 - "$MANIFEST" <<'PY'
import sys

try:
    import tomllib
except ModuleNotFoundError:
    tomllib = None

# Written with chr() rather than as a literal: the single-quoted triple was
# once written as "'\'\''", which Python reads as FOUR apostrophes, so ''' never
# opened a multi-line string and the decoy protection silently did not apply to
# it. chr() cannot be miscounted.
TRIPLE_D = chr(34) * 3
TRIPLE_S = chr(39) * 3

if tomllib is not None:
    with open(sys.argv[1], "rb") as handle:
        data = tomllib.load(handle)
    version = data.get("package", {}).get("version")
    if not isinstance(version, str):
        # Unreachable as a decision: a non-str has no .strip() and dies with an
        # AttributeError on the next line instead. Kept because the error it
        # prints is the useful one, but do not expect a test to tell it apart
        # from the next line.
        raise SystemExit(f"{sys.argv[1]} has no [package] version")
    if not version.strip():
        raise SystemExit(f"{sys.argv[1]} [package] version is empty")
    # cargo normalises surrounding whitespace out of a version, so strip here
    # too. Leaving it would make the two branches disagree quietly on
    # `version = """\n  1.2.3  \n"""`.
    print(version.strip())
    raise SystemExit(0)

# Fallback for Python < 3.11 -- which is the system python3 on macOS, so this is
# the default path for anyone packaging locally, not a corner case. Track which
# section each line belongs to, so a dependency table above [package] cannot be
# mistaken for it, and read the quoted literal properly: TOML allows both double
# and single quotes and allows a trailing comment, and a naive strip('"') turned
# `version = "1.2.3" # bump` into the string `1.2.3" # bump`, which then gets
# stamped into a public artifact and pushed as an update.
# Scan the whole document before answering rather than returning on the first
# hit. A manifest that defines the version twice is one cargo rejects; taking
# the first match anyway would make this branch answer a question the build
# never gets to.
in_package = False
seen_table = False
found = None
in_multiline = None
collecting = None
with open(sys.argv[1], encoding="utf-8") as handle:
    for line in handle:
        stripped = line.strip()

        if in_multiline is not None:
            closes_here = in_multiline in stripped
            if collecting is not None:
                body, _, tail = stripped.partition(in_multiline)
                collecting += body
                if closes_here:
                    if tail.strip() and not tail.strip().startswith("#"):
                        raise SystemExit(
                            f"{sys.argv[1]} [package] version has trailing garbage")
                    literal = collecting.strip()
                    if literal.startswith("{") or not literal:
                        raise SystemExit(
                            f"{sys.argv[1]} [package] version is not a literal")
                    if found is not None:
                        raise SystemExit(
                            f"{sys.argv[1]} defines the [package] version twice")
                    found = literal
                    collecting = None
            if closes_here:
                in_multiline = None
            continue

        # Any key can open a multi-line string, and a `version = "..."` inside
        # one is a value, not a key. Reading it as a key once made this branch
        # answer 9.9.9 for a manifest cargo reads as 0.0.0 -- a wrong version
        # stamped into a public artifact, which is the thing this script
        # exists to prevent. Track the delimiter until it closes, for every
        # key, not just version.
        if "=" in stripped:
            _k, _, _v = stripped.partition("=")
            _v = _v.strip()
            for _q in (TRIPLE_D, TRIPLE_S):
                if _v.startswith(_q) and _v.find(_q, 3) == -1:
                    in_multiline = _q
                    collecting = "" if _k.strip() == "version" else None
                    break

        if stripped.startswith("["):
            # cargo accepts "[ package ]" and a trailing comment on the header;
            # rejecting those here would make local packaging fail on a
            # manifest that builds fine, which is the split this script exists
            # to remove.
            header = stripped.split("#", 1)[0].strip()
            if header.startswith("[") and header.endswith("]"):
                in_package = header[1:-1].strip() == "package"
                seen_table = True
                continue
        key, _, value = stripped.partition("=")
        key = key.strip().strip("'\"")
        if in_package:
            if key != "version":
                continue
        elif seen_table or key != "package.version":
            # `package.version = "..."` is the dotted spelling of
            # [package] version, and cargo accepts it at the document root.
            continue
        value = value.strip()
        if not value or value[0] not in "\"'":
            raise SystemExit(f"{sys.argv[1]} [package] version is not a quoted literal")
        if value.startswith((TRIPLE_D, TRIPLE_S)):
            quote, width = value[:3], 3
        else:
            quote, width = value[0], 1
        end = value.find(quote, width)
        if end == -1:
            if width == 1:
                raise SystemExit(f"{sys.argv[1]} [package] version has an unterminated string")
            # Opened a multi-line string. For any key other than version there is
            # nothing to collect -- it just has to be skipped so its body cannot
            # be mistaken for keys.
            continue
        literal = value[width:end]
        trailing = value[end + width:].strip()
        # Anything after the literal must be a comment, as TOML requires.
        if trailing and not trailing.startswith("#"):
            raise SystemExit(f"{sys.argv[1]} [package] version has trailing garbage")
        if literal.startswith("{") or not literal.strip():
            raise SystemExit(f"{sys.argv[1]} [package] version is not a literal")
        if found is not None:
            raise SystemExit(f"{sys.argv[1]} defines the [package] version twice")
        found = literal

if found is None:
    raise SystemExit(f"{sys.argv[1]} has no [package] version")
print(found)
PY
