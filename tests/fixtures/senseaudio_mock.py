#!/usr/bin/env python3
"""Minimal in-process SenseAudio contract mock.

The mock deliberately records only request shapes, safe numeric metadata, and
hashes. It never writes request text, authorization values, or response audio
hex to disk.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any


VOICE_IDS = {
    "male_0004_a",
    "male_0028_a",
    "female_0033_b",
    "female_0038_b",
}
RAW_BREAK_CONTROL = re.compile(r"<break(?:\s|>)", re.IGNORECASE)


class MockState:
    def __init__(self, audio_path: Path, expected_key: str, summary_path: Path) -> None:
        self.audio_bytes = audio_path.read_bytes()
        self.expected_key = expected_key
        self.summary_path = summary_path
        self.voice_requests: list[dict[str, Any]] = []
        self.tts_requests: list[dict[str, Any]] = []
        self.lock = threading.Lock()

    def write_summary(self) -> None:
        summary = {
            "voice_requests": self.voice_requests,
            "tts_requests": self.tts_requests,
        }
        temporary = self.summary_path.with_name(self.summary_path.name + ".tmp")
        temporary.write_text(
            json.dumps(summary, ensure_ascii=False, sort_keys=True), encoding="utf-8"
        )
        temporary.replace(self.summary_path)

    def append_voice_request(self, record: dict[str, Any]) -> None:
        with self.lock:
            self.voice_requests.append(record)
            self.write_summary()

    def append_tts_request(self, record: dict[str, Any]) -> None:
        with self.lock:
            self.tts_requests.append(record)
            self.write_summary()


class SenseAudioMockHandler(BaseHTTPRequestHandler):
    state: MockState

    def log_message(self, _format: str, *_args: Any) -> None:
        return

    def _request_json(self) -> dict[str, Any]:
        try:
            content_length = int(self.headers.get("Content-Length", "0"))
            body = self.rfile.read(content_length)
            value = json.loads(body)
        except (ValueError, json.JSONDecodeError):
            return {}
        return value if isinstance(value, dict) else {}

    def _authorized(self) -> bool:
        expected = f"Bearer {self.state.expected_key}"
        return self.headers.get("Authorization") == expected

    def _send_json(self, status: int, payload: dict[str, Any]) -> None:
        encoded = json.dumps(payload, ensure_ascii=False, separators=(",", ":")).encode(
            "utf-8"
        )
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)

    def do_POST(self) -> None:  # noqa: N802 - stdlib handler API
        payload = self._request_json()
        authorized = self._authorized()

        if self.path == "/v1/get_voice":
            if not authorized:
                status = 401
                response: dict[str, Any] = {
                    "code": "unauthorized",
                    "message": "mock authorization failed",
                }
            elif payload == {"voice_type": "all"}:
                status = 200
                response = {
                    "system_voice": [{"voice_id": voice_id} for voice_id in sorted(VOICE_IDS)],
                    "voice_cloning": [],
                    "voice_generation": [],
                    "base_resp": {"status_code": 0, "status_msg": "success"},
                }
            elif payload == {}:
                status = 400
                response = {
                    "code": "voice_type_required",
                    "message": "mock requires voice_type",
                }
            else:
                status = 400
                response = {"code": "invalid_request", "message": "mock request shape"}

            self.state.append_voice_request(
                {
                    "body_shape": (
                        "voice_type_all"
                        if payload == {"voice_type": "all"}
                        else "empty_object"
                        if payload == {}
                        else "other"
                    ),
                    "voice_type": payload.get("voice_type"),
                    "authorization_valid": authorized,
                    "http_status": status,
                }
            )
            self._send_json(status, response)
            return

        if self.path == "/v1/t2a_v2":
            voice_setting = payload.get("voice_setting")
            audio_setting = payload.get("audio_setting")
            text = payload.get("text")
            text_value = text if isinstance(text, str) else ""
            voice_values = voice_setting if isinstance(voice_setting, dict) else {}
            audio_values = audio_setting if isinstance(audio_setting, dict) else {}
            record = {
                "request_keys": sorted(payload.keys()),
                "authorization_valid": authorized,
                "stream": payload.get("stream"),
                "voice_id": voice_values.get("voice_id"),
                "speed": voice_values.get("speed"),
                "volume": voice_values.get("vol"),
                "pitch": voice_values.get("pitch"),
                "audio_format": audio_values.get("format"),
                "sample_rate": audio_values.get("sample_rate"),
                "bitrate": audio_values.get("bitrate"),
                "channel": audio_values.get("channel"),
                "text_length": len(text_value),
                "text_sha256": hashlib.sha256(text_value.encode("utf-8")).hexdigest(),
                "raw_break_control": bool(RAW_BREAK_CONTROL.search(text_value)),
                "zero_width_guard": "<\u200bbreak" in text_value,
                "has_product_control_fields": any(
                    key in payload or key in voice_values for key in ("emotion", "style")
                ),
            }
            self.state.append_tts_request(record)

            if not authorized:
                self._send_json(
                    401,
                    {"code": "unauthorized", "message": "mock authorization failed"},
                )
                return
            if (
                sorted(payload.keys())
                != ["audio_setting", "model", "stream", "text", "voice_setting"]
                or payload.get("model") != "sensenova-tts-2.0"
                or payload.get("stream") is not False
                or voice_values.get("voice_id") not in VOICE_IDS
                or audio_values
                != {"bitrate": 128000, "channel": 2, "format": "mp3", "sample_rate": 32000}
            ):
                self._send_json(
                    400,
                    {"code": "invalid_request", "message": "mock request contract"},
                )
                return

            # The hex representation exists only in this response buffer. It
            # is never written to the request summary or an artifact file.
            self._send_json(
                200,
                {
                    "data": {"audio": self.state.audio_bytes.hex(), "status": 2},
                    "extra_info": {
                        "audio_length": 250,
                        "audio_sample_rate": 32000,
                        "audio_size": len(self.state.audio_bytes),
                        "bitrate": 128000,
                        "audio_format": "mp3",
                        "audio_channel": 2,
                        "usage_characters": len(text_value),
                    },
                    "trace_id": "mock-trace-001",
                    "base_resp": {"status_code": 0, "status_msg": "success"},
                },
            )
            return

        self._send_json(404, {"code": "not_found", "message": "mock path"})


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--port-file", type=Path, required=True)
    parser.add_argument("--summary-file", type=Path, required=True)
    parser.add_argument("--audio-file", type=Path, required=True)
    parser.add_argument("--expected-key", required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    args.port_file.parent.mkdir(parents=True, exist_ok=True)
    args.summary_file.parent.mkdir(parents=True, exist_ok=True)
    state = MockState(args.audio_file, args.expected_key, args.summary_file)
    state.write_summary()

    class Server(ThreadingHTTPServer):
        allow_reuse_address = True

    server = Server(("127.0.0.1", 0), SenseAudioMockHandler)
    SenseAudioMockHandler.state = state
    args.port_file.write_text(str(server.server_port), encoding="ascii")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
