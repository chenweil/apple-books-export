#!/bin/bash
# UI 回归验证。把探针和真实源码一起编译(排除 main.swift 的顶层代码),
# 因此断言的是实现本身,不是重建出来的约束副本。
set -euo pipefail
cd "$(dirname "$0")/.."

SOURCES=$(find Sources/BooksExporter -name '*.swift' ! -name 'main.swift')
OUT=$(mktemp -d)/verify-ui

# -swift-version 5 is pinned rather than left to the toolchain default. The
# probe is compiled with raw swiftc, not through SwiftPM, so it would otherwise
# inherit whatever language mode the installed toolchain happens to default to.
# That produced a failure nobody could reproduce locally: Swift 6.3 accepted
# code that Swift 5.10 -- the version on the macos-14 runner -- rejected, and
# the only warning came from CI. Pinning the mode removes one source of
# divergence between the two toolchains the project supports.
swiftc -swift-version 5 -o "$OUT" $SOURCES Scripts/verify-ui.swift
"$OUT"
