#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPOSITORY_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
BINARY_NAME="apple-books-exporter"

usage() {
  printf 'Usage: %s [--print-path]\n' "$0" >&2
}

fail() {
  local code="$1"
  local message="$2"
  local remediation="$3"
  printf '%s: %s\n' "$code" "$message" >&2
  printf '处理：%s\n' "$remediation" >&2
  exit 1
}

if [[ "${1:-}" == "--print-path" ]]; then
  if [[ "$#" -ne 1 ]]; then
    usage
    exit 2
  fi
elif [[ "$#" -ne 0 ]]; then
  usage
  exit 2
fi

if [[ "$(uname -s)" != "Darwin" ]]; then
  fail \
    "UNSUPPORTED_PLATFORM" \
    "Apple Books 数据源仅支持 macOS。" \
    "请在 macOS 上运行 Apple Books Exporter。"
fi

resolve_binary() {
  local candidate

  if [[ -n "${APPLE_BOOKS_EXPORTER_BIN:-}" ]]; then
    printf '%s\n' "$APPLE_BOOKS_EXPORTER_BIN"
    return 0
  fi

  for candidate in \
    "$REPOSITORY_ROOT/target/release/$BINARY_NAME" \
    "$REPOSITORY_ROOT/target/debug/$BINARY_NAME"; do
    if [[ -x "$candidate" ]]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done

  if candidate="$(command -v "$BINARY_NAME" 2>/dev/null)" && [[ -x "$candidate" ]]; then
    printf '%s\n' "$candidate"
    return 0
  fi

  if [[ -x "$SCRIPT_DIR/$BINARY_NAME" ]]; then
    printf '%s\n' "$SCRIPT_DIR/$BINARY_NAME"
    return 0
  fi

  return 1
}

if ! EXPORTER="$(resolve_binary)"; then
  fail \
    "BACKEND_UNAVAILABLE" \
    "找不到可执行的 $BINARY_NAME binary。" \
    "设置 APPLE_BOOKS_EXPORTER_BIN，构建仓库 release/debug binary，或将兼容 binary 放入 PATH。"
fi

if [[ ! -x "$EXPORTER" ]]; then
  fail \
    "BACKEND_UNAVAILABLE" \
    "指定的 binary 不存在或不可执行：$EXPORTER" \
    "检查 APPLE_BOOKS_EXPORTER_BIN，或重新安装可执行 binary。"
fi

if ! command -v file >/dev/null 2>&1; then
  fail \
    "VALIDATION_TOOL_UNAVAILABLE" \
    "当前 macOS 缺少 file 命令，无法验证 binary 架构。" \
    "恢复 macOS 的 file 工具后重试。"
fi

FILE_DESCRIPTION="$(file -Lb "$EXPORTER" 2>/dev/null || true)"
if [[ "$FILE_DESCRIPTION" != *"Mach-O"* ]]; then
  fail \
    "BINARY_INCOMPATIBLE" \
    "binary 不是 macOS Mach-O 可执行文件：$EXPORTER" \
    "使用与当前 macOS 匹配的 arm64 或 x86_64 apple-books-exporter binary。"
fi

case "$(uname -m)" in
  arm64|aarch64)
    EXPECTED_ARCH="arm64"
    ;;
  x86_64|amd64)
    EXPECTED_ARCH="x86_64"
    ;;
  *)
    fail \
      "BINARY_INCOMPATIBLE" \
      "不支持验证当前 CPU 架构：$(uname -m)" \
      "使用 macOS arm64 或 x86_64 环境运行兼容 binary。"
    ;;
esac

if [[ "$FILE_DESCRIPTION" != *"$EXPECTED_ARCH"* ]]; then
  fail \
    "BINARY_INCOMPATIBLE" \
    "binary 架构与当前 macOS CPU 不匹配：$EXPORTER" \
    "使用与当前 CPU 架构匹配的 apple-books-exporter binary。"
fi

if ! HELP_OUTPUT="$("$EXPORTER" --help 2>&1)"; then
  fail \
    "BINARY_INCOMPATIBLE" \
    "binary 无法执行 --help：$EXPORTER" \
    "重新构建或安装与当前 macOS CPU 架构兼容的 binary。"
fi

for command_name in list annotations export doctor; do
  if ! printf '%s\n' "$HELP_OUTPUT" | grep -Eq "^[[:space:]]*${command_name}([[:space:]]|$)"; then
    fail \
      "UNSUPPORTED_BINARY" \
      "binary 的 --help 未声明所需命令：$command_name" \
      "请构建或安装支持 Machine JSON Protocol 的 apple-books-exporter binary。"
  fi
done

if [[ "${1:-}" == "--print-path" ]]; then
  printf '%s\n' "$EXPORTER"
else
  printf 'validated: %s\n' "$EXPORTER"
fi
