#!/bin/bash
# UI 回归验证。把探针和真实源码一起编译(排除 main.swift 的顶层代码),
# 因此断言的是实现本身,不是重建出来的约束副本。
set -euo pipefail
cd "$(dirname "$0")/.."

SOURCES=$(find Sources/BooksExporter -name '*.swift' ! -name 'main.swift')
OUT=$(mktemp -d)/verify-ui

swiftc -o "$OUT" $SOURCES Scripts/verify-ui.swift
"$OUT"
