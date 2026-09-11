#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TRACER="$ROOT_DIR/scripts/senseaudio-smoke.py"

fail() {
  printf 'senseaudio contract test failed: %s\n' "$1" >&2
  exit 1
}

test_skip_without_double_opt_in() {
  local output
  output="$(env -u RUN_SENSEAUDIO_SMOKE \
    SENSEAUDIO_API_KEY=guard-key \
    SENSEAUDIO_API_BASE_URL=http://127.0.0.1:1 \
    python3 "$TRACER" 2>&1)" || fail "unset opt-in must be a safe skip"

  grep -Fq 'SKIPPED' <<<"$output" || fail "missing safe skip result"
  if grep -Eq 'Authorization|Bearer|SENSEAUDIO_API_KEY=' <<<"$output"; then
    fail "skip output contains authorization material"
  fi
}

test_missing_key_is_blocked_before_network() {
  local output
  local output_dir
  output_dir="$(mktemp -d "${TMPDIR:-/tmp}/senseaudio-contract.XXXXXX")"
  trap 'rm -rf "$output_dir"' RETURN

  if output="$(env -u SENSEAUDIO_API_KEY \
    RUN_SENSEAUDIO_SMOKE=1 \
    SENSEAUDIO_API_BASE_URL=http://127.0.0.1:1 \
    SENSEAUDIO_OUTPUT_DIR="$output_dir" \
    python3 "$TRACER" 2>&1)"; then
    fail "missing key must not run the smoke"
  fi

  grep -Fq 'BLOCKED' <<<"$output" || fail "missing blocked result"
  if [[ -n "$(find "$output_dir" -type f -print -quit)" ]]; then
    fail "missing-key preflight wrote retained evidence"
  fi
}

test_provider_contract_and_safe_outputs() {
  local test_dir
  local mock_audio
  local port_file
  local summary_file
  local stdout_file
  local stderr_file
  local mock_stderr
  local mock_pid
  local evidence_file
  local port

  test_dir="$(mktemp -d "${TMPDIR:-/tmp}/senseaudio-contract.XXXXXX")"
  trap 'if [[ -n "${mock_pid:-}" ]]; then kill "$mock_pid" 2>/dev/null || true; wait "$mock_pid" 2>/dev/null || true; fi; rm -rf "$test_dir"' RETURN
  mock_audio="$test_dir/mock.mp3"
  port_file="$test_dir/port"
  summary_file="$test_dir/requests.json"
  stdout_file="$test_dir/stdout"
  stderr_file="$test_dir/stderr"
  mock_stderr="$test_dir/mock-stderr"

  if ! command -v ffmpeg >/dev/null 2>&1; then
    fail "provider contract test requires ffmpeg"
  fi
  if ! command -v jq >/dev/null 2>&1; then
    fail "provider contract test requires jq"
  fi

  ffmpeg -hide_banner -loglevel error \
    -f lavfi -i sine=frequency=440:duration=0.25:sample_rate=32000 \
    -ac 2 -ar 32000 -c:a libmp3lame -b:a 128k -y "$mock_audio" \
    >/dev/null 2>"$mock_stderr" || fail "could not create mock MP3"

  python3 "$ROOT_DIR/tests/fixtures/senseaudio_mock.py" \
    --port-file "$port_file" \
    --summary-file "$summary_file" \
    --audio-file "$mock_audio" \
    --expected-key contract-test-key \
    >/dev/null 2>"$mock_stderr" &
  mock_pid=$!

  for _ in {1..10}; do
    if [[ -s "$port_file" ]]; then
      break
    fi
    sleep 1
  done
  [[ -s "$port_file" ]] || fail "mock server did not start"
  port="$(<"$port_file")"

  if ! env \
    RUN_SENSEAUDIO_SMOKE=1 \
    SENSEAUDIO_API_KEY=contract-test-key \
    SENSEAUDIO_API_BASE_URL="http://127.0.0.1:$port" \
    SENSEAUDIO_OUTPUT_DIR="$test_dir/output" \
    python3 "$TRACER" >"$stdout_file" 2>"$stderr_file"; then
    fail "provider contract smoke failed"
  fi

  if grep -Eq 'contract-test-key|Authorization:|Bearer |<break time=500|data\.audio' \
    "$stdout_file" "$stderr_file"; then
    fail "runtime output contains secret, authorization, source text, or response audio"
  fi

  evidence_file="$(find "$test_dir/output" -type f -name evidence.json -print -quit)"
  [[ -n "$evidence_file" ]] || fail "safe evidence file was not written"
  if grep -Eq 'contract-test-key|<break time=500|data\.audio' "$evidence_file"; then
    fail "retained evidence contains forbidden material"
  fi

  jq -e '.voice_requests | length == 2' "$summary_file" >/dev/null \
    || fail "voice-list request count mismatch"
  jq -e '.voice_requests[0].body_shape == "voice_type_all" and
    .voice_requests[0].voice_type == "all" and
    .voice_requests[1].body_shape == "empty_object" and
    .voice_requests[0].authorization_valid and
    .voice_requests[1].authorization_valid' "$summary_file" >/dev/null \
    || fail "voice-list request contract mismatch"
  jq -e '.tts_requests | length == 7' "$summary_file" >/dev/null \
    || fail "fixed smoke matrix request count mismatch"
  jq -e 'all(.tts_requests[];
    .request_keys == ["audio_setting", "model", "stream", "text", "voice_setting"] and
    .authorization_valid and
    .stream == false and
    .audio_format == "mp3" and
    .sample_rate == 32000 and
    .bitrate == 128000 and
    .channel == 2 and
    (.raw_break_control == false) and
    (.has_product_control_fields == false))' "$summary_file" >/dev/null \
    || fail "TTS request mapping or safety contract mismatch"
  jq -e 'any(.tts_requests[]; .zero_width_guard == true)' "$summary_file" >/dev/null \
    || fail "control-markup safe transformation was not sent"
  jq -e 'any(.tts_requests[]; .voice_id == "male_0004_a") and
    any(.tts_requests[]; .voice_id == "female_0033_b") and
    any(.tts_requests[]; .voice_id == "male_0028_a") and
    any(.tts_requests[]; .speed == 1.25) and
    any(.tts_requests[]; .volume == 1.5)' "$summary_file" >/dev/null \
    || fail "voice/control coverage mismatch"
  jq -e '.status == "awaiting_manual_listen" and
    (.samples | length == 7) and
    all(.samples[]; .manual_listen == "pending" and
      .audio.format == "mp3" and
      .audio.sample_rate == 32000 and
      .audio.bitrate == 128000 and
      .audio.channel == 2)' "$evidence_file" >/dev/null \
    || fail "MP3 evidence or manual-listen state mismatch"

  if env \
    RUN_SENSEAUDIO_SMOKE=1 \
    SENSEAUDIO_API_KEY=wrong-key \
    SENSEAUDIO_API_BASE_URL="http://127.0.0.1:$port" \
    SENSEAUDIO_OUTPUT_DIR="$test_dir/unauthorized-output" \
    python3 "$TRACER" >"$test_dir/unauthorized-stdout" 2>"$test_dir/unauthorized-stderr"; then
    fail "401 response must block the smoke"
  fi
  if grep -Eq 'wrong-key|Authorization:|Bearer ' \
    "$test_dir/unauthorized-stdout" "$test_dir/unauthorized-stderr"; then
    fail "401 output contains authorization material"
  fi
  local unauthorized_evidence
  unauthorized_evidence="$(find "$test_dir/unauthorized-output" -type f -name evidence.json -print -quit)"
  [[ -n "$unauthorized_evidence" ]] || fail "401 evidence was not written"
  if grep -Eq 'wrong-key|Authorization:|Bearer ' "$unauthorized_evidence"; then
    fail "401 evidence contains authorization material"
  fi
  jq -e '.voice_catalog.explicit_all.http_status == 401 and
    .voice_catalog.empty_object.http_status == 401 and
    .generated_sample_count == 0' "$unauthorized_evidence" >/dev/null \
    || fail "401 evidence status mismatch"
}

test_skip_without_double_opt_in
test_missing_key_is_blocked_before_network
test_provider_contract_and_safe_outputs
printf 'senseaudio opt-in guard tests passed\n'
