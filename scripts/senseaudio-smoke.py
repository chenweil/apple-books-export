#!/usr/bin/env python3
"""Run the opt-in, fixed-text SenseAudio contract smoke.

This entry point deliberately has no text or database input. The live mode is
enabled only when RUN_SENSEAUDIO_SMOKE=1 and the API key is present in the
environment. HTTP response JSON, including audio hex, is processed in memory
only; retained evidence contains hashes and metadata, never source text.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.request import HTTPRedirectHandler, Request, build_opener


DEFAULT_API_BASE_URL = "https://api.senseaudio.cn"
MODEL = "sensenova-tts-2.0"
DEFAULT_VOICE_ID = "male_0004_a"
EMOTION_CANDIDATES = ("female_0033_b", "male_0021_c")
# Exact documented candidates; the adapter never infers semantics from an ID
# suffix. The first two are the current catalog's content-style variants.
STYLE_CANDIDATES = ("male_0028_a", "male_0028_b", "female_0038_b")
EXPECTED_AUDIO = {
    "format": "mp3",
    "sample_rate": 32000,
    "bitrate": 128000,
    "channel": 2,
}
SAFE_PROVIDER_ID = re.compile(r"[A-Za-z0-9_.:-]{1,200}\Z")
RAW_BREAK_CONTROL = re.compile(r"<break(?:\s|>)", re.IGNORECASE)
ZERO_WIDTH_GUARD = "\u200b"


class NoRedirectHandler(HTTPRedirectHandler):
    """Do not forward an API key to a redirected endpoint."""

    def redirect_request(self, *_args: Any, **_kwargs: Any) -> None:
        return None


HTTP_OPENER = build_opener(NoRedirectHandler)

# These are fixed, non-user samples. There is intentionally no option to
# provide text through argv or an environment variable.
FIXED_SAMPLES = {
    "zh-default": "这是 SenseAudio 合同验证的固定中文片段。",
    "en-default": "This is a fixed English sample for the SenseAudio contract.",
    "control-safe": "Literal <break time=500> text remains ordinary content.",
}


class SmokeFailure(Exception):
    """An expected, safe-to-report smoke failure."""

    def __init__(self, code: str) -> None:
        super().__init__()
        self.code = code


@dataclass
class ApiResult:
    http_status: int | None
    payload: dict[str, Any] | None
    error_kind: str | None
    latency_ms: int


def post_json(url: str, api_key: str, payload: dict[str, Any]) -> ApiResult:
    """POST JSON without retaining the raw response outside this function."""

    request = Request(
        url,
        data=json.dumps(payload, ensure_ascii=False, separators=(",", ":")).encode(
            "utf-8"
        ),
        headers={
            "Authorization": f"Bearer {api_key}",
            "Content-Type": "application/json",
        },
        method="POST",
    )
    started = time.monotonic()
    try:
        with HTTP_OPENER.open(request, timeout=30) as response:
            http_status = int(response.status)
            response_bytes = response.read()
    except HTTPError as error:
        return ApiResult(
            http_status=int(error.code),
            payload=None,
            error_kind="http_error",
            latency_ms=elapsed_ms(started),
        )
    except (OSError, URLError, TimeoutError, ValueError):
        return ApiResult(
            http_status=None,
            payload=None,
            error_kind="transport_error",
            latency_ms=elapsed_ms(started),
        )

    try:
        decoded = json.loads(response_bytes)
    except (UnicodeDecodeError, json.JSONDecodeError):
        return ApiResult(
            http_status=http_status,
            payload=None,
            error_kind="invalid_json",
            latency_ms=elapsed_ms(started),
        )

    if not isinstance(decoded, dict):
        return ApiResult(
            http_status=http_status,
            payload=None,
            error_kind="invalid_json_shape",
            latency_ms=elapsed_ms(started),
        )
    return ApiResult(
        http_status=http_status,
        payload=decoded,
        error_kind=None,
        latency_ms=elapsed_ms(started),
    )


def elapsed_ms(started: float) -> int:
    return max(0, round((time.monotonic() - started) * 1000))


def safe_integer(value: Any) -> int | None:
    if isinstance(value, bool):
        return None
    if isinstance(value, int):
        return value
    return None


def safe_provider_id(value: Any, api_key: str) -> str | None:
    if not isinstance(value, str) or not SAFE_PROVIDER_ID.fullmatch(value):
        return None
    if api_key and api_key in value:
        return None
    return value


def provider_status(payload: dict[str, Any] | None) -> int | None:
    if not isinstance(payload, dict):
        return None
    base_resp = payload.get("base_resp")
    if not isinstance(base_resp, dict):
        return None
    return safe_integer(base_resp.get("status_code"))


def response_error_code(result: ApiResult) -> str:
    if result.error_kind == "transport_error":
        return "transport_unknown"
    if result.http_status == 401:
        return "auth_failed"
    if result.http_status == 429:
        return "rate_limited"
    if result.error_kind:
        return "provider_response_invalid"
    return "provider_failed"


def voice_response_summary(
    result: ApiResult, request_shape: str, api_key: str
) -> tuple[dict[str, Any], set[str]]:
    payload = result.payload
    known_ids: set[str] = set()
    group_counts: dict[str, int | None] = {}
    if isinstance(payload, dict):
        for group in ("system_voice", "voice_cloning", "voice_generation"):
            values = payload.get(group)
            group_counts[group] = len(values) if isinstance(values, list) else None
            if isinstance(values, list):
                for entry in values:
                    if not isinstance(entry, dict):
                        continue
                    voice_id = safe_provider_id(entry.get("voice_id"), api_key)
                    if voice_id in {
                        DEFAULT_VOICE_ID,
                        *EMOTION_CANDIDATES,
                        *STYLE_CANDIDATES,
                    }:
                        known_ids.add(voice_id)

    summary = {
        "request_shape": request_shape,
        "http_status": result.http_status,
        "provider_status_code": provider_status(payload),
        "accepted": result.http_status == 200 and provider_status(payload) == 0,
        "latency_ms": result.latency_ms,
        "group_counts": group_counts,
        "known_candidate_ids": sorted(known_ids),
    }
    if result.error_kind:
        summary["error"] = result.error_kind
    return summary, known_ids


def select_variant_voice(
    available_ids: set[str], env_name: str, candidates: tuple[str, ...]
) -> tuple[str | None, str | None]:
    configured = os.environ.get(env_name)
    if configured is not None:
        if not SAFE_PROVIDER_ID.fullmatch(configured):
            return None, "configured_voice_id_invalid"
        if configured not in available_ids:
            return None, "configured_voice_id_unavailable"
        return configured, None
    for candidate in candidates:
        if candidate in available_ids:
            return candidate, None
    return None, "documented_voice_variant_unavailable"


def provider_safe_control_text(source_text: str) -> str:
    """Break the provider's ASCII break-token shape without changing visible text."""

    return source_text.replace("<", f"<{ZERO_WIDTH_GUARD}")


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def validate_mp3(
    audio_path: Path, provider_extra: dict[str, Any], audio_bytes: bytes
) -> dict[str, Any]:
    if not audio_bytes or audio_path.stat().st_size != len(audio_bytes):
        raise SmokeFailure("audio_empty_or_size_mismatch")
    try:
        probe = subprocess.run(
            [
                "ffprobe",
                "-v",
                "error",
                "-select_streams",
                "a:0",
                "-show_entries",
                "stream=codec_name,sample_rate,channels,bit_rate:format=duration",
                "-of",
                "json",
                str(audio_path),
            ],
            capture_output=True,
            text=True,
            check=False,
        )
    except (OSError, ValueError):
        raise SmokeFailure("mp3_validator_unavailable") from None
    if probe.returncode != 0:
        raise SmokeFailure("mp3_invalid")
    try:
        probe_value = json.loads(probe.stdout)
        stream = probe_value["streams"][0]
        format_info = probe_value["format"]
        local = {
            "codec": stream["codec_name"],
            "sample_rate": int(stream["sample_rate"]),
            "bitrate": int(stream["bit_rate"]),
            "channel": int(stream["channels"]),
            "duration_ms": round(float(format_info["duration"]) * 1000),
        }
    except (KeyError, IndexError, TypeError, ValueError, json.JSONDecodeError):
        raise SmokeFailure("mp3_metadata_unreadable") from None

    if local["codec"] != "mp3" or any(
        local[key] != value
        for key, value in EXPECTED_AUDIO.items()
        if key != "format"
    ):
        raise SmokeFailure("mp3_metadata_mismatch")
    if local["duration_ms"] <= 0:
        raise SmokeFailure("mp3_duration_invalid")

    provider_values = {
        "format": provider_extra.get("audio_format"),
        "sample_rate": provider_extra.get("audio_sample_rate"),
        "bitrate": provider_extra.get("bitrate"),
        "channel": provider_extra.get("audio_channel"),
    }
    if provider_values != EXPECTED_AUDIO:
        raise SmokeFailure("provider_audio_metadata_mismatch")
    provider_size = safe_integer(provider_extra.get("audio_size"))
    if provider_size is not None and provider_size != len(audio_bytes):
        raise SmokeFailure("provider_audio_size_mismatch")

    return {
        "path": str(audio_path),
        "sha256": hashlib.sha256(audio_bytes).hexdigest(),
        "size_bytes": len(audio_bytes),
        "duration_ms": local["duration_ms"],
        "format": EXPECTED_AUDIO["format"],
        "sample_rate": local["sample_rate"],
        "bitrate": local["bitrate"],
        "channel": local["channel"],
    }


def synthesize_sample(
    api_base_url: str,
    api_key: str,
    output_dir: Path,
    sample_id: str,
    text: str,
    voice_id: str,
    speed: float,
    volume: float,
    purpose: str,
    transformation: str | None = None,
    source_text: str | None = None,
) -> dict[str, Any]:
    payload = {
        "model": MODEL,
        "text": text,
        "stream": False,
        "voice_setting": {
            "voice_id": voice_id,
            "speed": speed,
            "vol": volume,
            "pitch": 0,
        },
        "audio_setting": {
            "format": EXPECTED_AUDIO["format"],
            "sample_rate": EXPECTED_AUDIO["sample_rate"],
            "bitrate": EXPECTED_AUDIO["bitrate"],
            "channel": EXPECTED_AUDIO["channel"],
        },
    }
    result = post_json(f"{api_base_url}/v1/t2a_v2", api_key, payload)
    if result.http_status != 200 or result.payload is None:
        raise SmokeFailure(response_error_code(result))
    response = result.payload
    if provider_status(response) != 0:
        raise SmokeFailure("provider_failed")
    data = response.get("data")
    extra_info = response.get("extra_info")
    audio_hex = data.get("audio") if isinstance(data, dict) else None
    if (
        not isinstance(data, dict)
        or data.get("status") != 2
        or not isinstance(extra_info, dict)
        or not isinstance(audio_hex, str)
        or not audio_hex
        or len(audio_hex) % 2 != 0
        or not re.fullmatch(r"[0-9a-fA-F]+", audio_hex)
    ):
        raise SmokeFailure("audio_response_invalid")
    try:
        audio_bytes = bytes.fromhex(audio_hex)
    except ValueError:
        raise SmokeFailure("audio_response_invalid") from None

    audio_path = output_dir / f"{sample_id}.mp3"
    try:
        audio_path.write_bytes(audio_bytes)
    except OSError:
        raise SmokeFailure("audio_artifact_write_failed") from None
    audio = validate_mp3(audio_path, extra_info, audio_bytes)
    result_value: dict[str, Any] = {
        "id": sample_id,
        "purpose": purpose,
        "status": "generated",
        "manual_listen": "pending",
        "voice_id": voice_id,
        "speed": speed,
        "volume": volume,
        "pitch": 0,
        "text_sha256": sha256_text(source_text if source_text is not None else text),
        "unicode_characters": len(source_text if source_text is not None else text),
        "provider_usage_characters": safe_integer(extra_info.get("usage_characters")),
        "latency_ms": result.latency_ms,
        "audio": audio,
        "provider": {
            "trace_id": safe_provider_id(response.get("trace_id"), api_key),
        },
    }
    if transformation is not None:
        result_value["control_markup_transformation"] = transformation
        result_value["provider_text_sha256"] = sha256_text(text)
        result_value["provider_text_contains_ascii_break_control"] = bool(
            RAW_BREAK_CONTROL.search(text)
        )
        result_value["provider_text_contains_zero_width_guard"] = (
            f"<{ZERO_WIDTH_GUARD}break" in text
        )
    return result_value


def create_output_dir() -> Path:
    configured_root = os.environ.get("SENSEAUDIO_OUTPUT_DIR")
    if configured_root:
        root = Path(configured_root)
        root.mkdir(parents=True, exist_ok=True)
        return Path(tempfile.mkdtemp(prefix="senseaudio-smoke-", dir=root))
    return Path(tempfile.mkdtemp(prefix="senseaudio-smoke-"))


def write_evidence(output_dir: Path, evidence: dict[str, Any]) -> Path:
    evidence_path = output_dir / "evidence.json"
    temporary_path = output_dir / f".evidence.{os.getpid()}.tmp"
    temporary_path.write_text(
        json.dumps(evidence, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    temporary_path.replace(evidence_path)
    return evidence_path


def run_smoke(api_key: str) -> int:
    api_base_url = os.environ.get("SENSEAUDIO_API_BASE_URL", DEFAULT_API_BASE_URL).rstrip(
        "/"
    )
    output_dir = create_output_dir()

    explicit_result = post_json(
        f"{api_base_url}/v1/get_voice", api_key, {"voice_type": "all"}
    )
    empty_result = post_json(f"{api_base_url}/v1/get_voice", api_key, {})
    explicit_summary, available_ids = voice_response_summary(
        explicit_result, "voice_type_all", api_key
    )
    empty_summary, _ = voice_response_summary(empty_result, "empty_object", api_key)

    explicit_accepted = bool(explicit_summary["accepted"])
    empty_accepted = bool(empty_summary["accepted"])
    if explicit_accepted and empty_accepted:
        voice_difference = "both_requests_accepted"
    elif explicit_accepted:
        voice_difference = "explicit_all_accepted_empty_object_rejected"
    elif empty_accepted:
        voice_difference = "explicit_all_rejected_empty_object_accepted"
    else:
        voice_difference = "neither_request_accepted"

    voice_catalog: dict[str, Any] = {
        "explicit_all": explicit_summary,
        "empty_object": empty_summary,
        "observed_difference": voice_difference,
        "default_voice_id": DEFAULT_VOICE_ID,
        "default_voice_available": DEFAULT_VOICE_ID in available_ids,
    }
    missing: list[str] = []
    samples: list[dict[str, Any]] = []

    if not explicit_accepted:
        missing.append("voice_catalog_unavailable")
    else:
        emotion_voice, emotion_error = select_variant_voice(
            available_ids,
            "SENSEAUDIO_EMOTION_VOICE_ID",
            EMOTION_CANDIDATES,
        )
        style_voice, style_error = select_variant_voice(
            available_ids,
            "SENSEAUDIO_STYLE_VOICE_ID",
            STYLE_CANDIDATES,
        )
        voice_catalog["emotion_voice_id"] = emotion_voice
        voice_catalog["style_voice_id"] = style_voice
        if emotion_error:
            missing.append(emotion_error)
        if style_error:
            missing.append(style_error)
        if DEFAULT_VOICE_ID not in available_ids:
            missing.append("default_voice_unavailable")
        else:
            sample_specs: list[dict[str, Any]] = [
                {
                    "id": "zh-default",
                    "text": FIXED_SAMPLES["zh-default"],
                    "voice_id": DEFAULT_VOICE_ID,
                    "speed": 1.0,
                    "volume": 1.0,
                    "purpose": "fixed_chinese_default",
                },
                {
                    "id": "en-default",
                    "text": FIXED_SAMPLES["en-default"],
                    "voice_id": DEFAULT_VOICE_ID,
                    "speed": 1.0,
                    "volume": 1.0,
                    "purpose": "fixed_english_default",
                },
            ]
            if emotion_voice is not None:
                sample_specs.append(
                    {
                        "id": "emotion-variant",
                        "text": FIXED_SAMPLES["en-default"],
                        "voice_id": emotion_voice,
                        "speed": 1.0,
                        "volume": 1.0,
                        "purpose": "catalog_selected_emotion_variant",
                    }
                )
            if style_voice is not None:
                sample_specs.append(
                    {
                        "id": "style-variant",
                        "text": FIXED_SAMPLES["en-default"],
                        "voice_id": style_voice,
                        "speed": 1.0,
                        "volume": 1.0,
                        "purpose": "catalog_selected_style_variant",
                    }
                )
            sample_specs.extend(
                [
                    {
                        "id": "speed-override",
                        "text": FIXED_SAMPLES["en-default"],
                        "voice_id": DEFAULT_VOICE_ID,
                        "speed": 1.25,
                        "volume": 1.0,
                        "purpose": "fixed_speed_override",
                    },
                    {
                        "id": "volume-override",
                        "text": FIXED_SAMPLES["en-default"],
                        "voice_id": DEFAULT_VOICE_ID,
                        "speed": 1.0,
                        "volume": 1.5,
                        "purpose": "fixed_volume_override",
                    },
                ]
            )
            control_source = FIXED_SAMPLES["control-safe"]
            sample_specs.append(
                {
                    "id": "control-safe",
                    "text": provider_safe_control_text(control_source),
                    "source_text": control_source,
                    "voice_id": DEFAULT_VOICE_ID,
                    "speed": 1.0,
                    "volume": 1.0,
                    "purpose": "literal_break_markup_safe_transform",
                    "transformation": "insert U+200B after each ASCII '<'",
                }
            )

            stop_after = False
            for spec in sample_specs:
                if stop_after:
                    break
                try:
                    sample = synthesize_sample(
                        api_base_url,
                        api_key,
                        output_dir,
                        sample_id=spec["id"],
                        text=spec["text"],
                        voice_id=spec["voice_id"],
                        speed=spec["speed"],
                        volume=spec["volume"],
                        purpose=spec["purpose"],
                        transformation=spec.get("transformation"),
                        source_text=spec.get("source_text"),
                    )
                    samples.append(sample)
                except SmokeFailure as failure:
                    samples.append(
                        {
                            "id": spec["id"],
                            "purpose": spec["purpose"],
                            "status": "failed",
                            "manual_listen": "not_available",
                            "error_code": failure.code,
                        }
                    )
                    missing.append(f"sample_{spec['id']}_{failure.code}")
                    if failure.code in {
                        "auth_failed",
                        "rate_limited",
                        "transport_unknown",
                    }:
                        stop_after = True

    generated_count = sum(1 for sample in samples if sample.get("status") == "generated")
    if not samples:
        missing.append("no_audio_samples_generated")
    evidence = {
        "schema_version": 1,
        "status": "awaiting_manual_listen" if not missing else "blocked",
        "provider": "senseaudio",
        "endpoint": "default" if api_base_url == DEFAULT_API_BASE_URL else "custom_test_endpoint",
        "double_opt_in": {"run_flag": True, "api_key_present": True},
        "voice_catalog": voice_catalog,
        "samples": samples,
        "manual_listen": {
            "required": True,
            "performed": False,
            "script_plays_audio": False,
        },
        "generated_sample_count": generated_count,
        "blocked_reasons": sorted(set(missing)),
    }
    try:
        evidence_path = write_evidence(output_dir, evidence)
    except OSError:
        print("SenseAudio smoke FAILED: evidence_write_failed", file=sys.stderr)
        return 1

    print(f"SenseAudio smoke generated {generated_count} fixed sample(s)")
    print(f"Evidence: {evidence_path}")
    for sample in samples:
        audio = sample.get("audio")
        if isinstance(audio, dict) and isinstance(audio.get("path"), str):
            print(f"Audio {sample['id']}: {audio['path']}")
    print("Manual listening: REQUIRED; this tracer never plays audio")
    if missing:
        print("SenseAudio smoke BLOCKED: see evidence.json for safe status")
        return 2
    print("SenseAudio smoke AWAITING_MANUAL_LISTEN: technical checks completed")
    return 0


def main() -> int:
    if len(sys.argv) != 1:
        print("SenseAudio smoke BLOCKED: text arguments are not accepted", file=sys.stderr)
        return 2
    if os.environ.get("RUN_SENSEAUDIO_SMOKE") != "1":
        print("SenseAudio smoke SKIPPED: explicit opt-in is not enabled")
        return 0

    api_key = os.environ.get("SENSEAUDIO_API_KEY")
    if not api_key:
        print("SenseAudio smoke BLOCKED: the required credential is unavailable")
        return 2

    try:
        return run_smoke(api_key)
    except SmokeFailure as failure:
        print(f"SenseAudio smoke FAILED: {failure.code}", file=sys.stderr)
        return 1
    except (OSError, ValueError, TypeError):
        print("SenseAudio smoke FAILED: safe_runtime_error", file=sys.stderr)
        return 1
    except Exception:
        # Do not expose a traceback: it could include provider request context.
        print("SenseAudio smoke FAILED: unexpected_safe_failure", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
