#!/usr/bin/env bash
#
# Real macOS smoke test for the read-only OpenTUI browser.
#
# Drives the real Bun/OpenTUI app inside a dedicated tmux pty against the real
# Rust backend and the real Apple Books library, then asserts that the rendered
# terminal frames contain live data. It never opens a GUI and never writes to
# the user's data: the browser only runs `list --json` and `annotations --json`.
#
# Requirements: macOS with Full Disk Access granted to the terminal running this
# script, Bun, tmux, and a built apple-books-exporter binary.
#
# Usage: bash scripts/tui-smoke.sh
#
# Note: every shell variable is written as ${VAR} because the assertions are
# interleaved with full-width CJK punctuation, and `$VAR（` is not a safe
# expansion under a multibyte locale.

set -euo pipefail

REPOSITORY_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BINARY_NAME="apple-books-exporter"
SOCKET="books-tui-smoke"
SESSION="browser"
WIDE_COLUMNS=140
WIDE_ROWS=45
COMPACT_COLUMNS=50
READY_TIMEOUT_SECONDS=60
LOAD_TIMEOUT_SECONDS=20

WORK_DIR=""
FAILURES=0
CHECKS=0

cleanup() {
  if [[ -n "${WORK_DIR}" ]]; then
    tmux -L "${SOCKET}" kill-server >/dev/null 2>&1 || true
    # Only ever remove the mktemp directory this script created.
    case "${WORK_DIR}" in
      "${TMPDIR:-/tmp}"/tui-smoke.*) rm -rf "${WORK_DIR}" ;;
    esac
  fi
}
trap cleanup EXIT

fail() {
  local code="$1"
  local message="$2"
  local remediation="$3"
  printf '%s: %s\n' "${code}" "${message}" >&2
  printf '处理：%s\n' "${remediation}" >&2
  exit 1
}

note() {
  printf '\n── %s\n' "$1"
}

pass() {
  CHECKS=$((CHECKS + 1))
  printf '  PASS  %s\n' "$1"
}

reject() {
  CHECKS=$((CHECKS + 1))
  FAILURES=$((FAILURES + 1))
  printf '  FAIL  %s\n' "$1"
  if [[ -n "${2:-}" ]]; then
    printf '%s\n' "$2" | sed 's/^/        /'
  fi
}

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    fail \
      "SMOKE_TOOL_UNAVAILABLE" \
      "真实冒烟需要 $1，但当前环境没有该命令。" \
      "安装 $1 后重试 scripts/tui-smoke.sh。"
  fi
}

resolve_backend() {
  local candidate

  if [[ -n "${APPLE_BOOKS_EXPORTER_BIN:-}" ]]; then
    printf '%s\n' "${APPLE_BOOKS_EXPORTER_BIN}"
    return 0
  fi

  for candidate in \
    "${REPOSITORY_ROOT}/target/release/${BINARY_NAME}" \
    "${REPOSITORY_ROOT}/target/debug/${BINARY_NAME}"; do
    if [[ -x "${candidate}" ]]; then
      printf '%s\n' "${candidate}"
      return 0
    fi
  done

  if candidate="$(command -v "${BINARY_NAME}" 2>/dev/null)"; then
    printf '%s\n' "${candidate}"
    return 0
  fi

  return 1
}

capture() {
  tmux -L "${SOCKET}" capture-pane -p -t "${SESSION}"
}

pane_is_dead() {
  local state
  state="$(tmux -L "${SOCKET}" list-panes -t "${SESSION}" -F '#{pane_dead}' 2>/dev/null || printf '1')"
  [[ "${state}" == "1" ]]
}

wait_for_pattern() {
  local pattern="$1"
  local limit="$2"
  local waited=0
  local frame

  while (( waited < limit )); do
    frame="$(capture 2>/dev/null || printf '')"
    if printf '%s' "${frame}" | grep -Fq -- "${pattern}"; then
      return 0
    fi
    if pane_is_dead; then
      printf 'pane exited while waiting for: %s\n' "${pattern}" >&2
      return 1
    fi
    sleep 1
    waited=$((waited + 1))
  done

  printf 'timed out after %ss waiting for: %s\n' "${limit}" "${pattern}" >&2
  return 1
}

send() {
  tmux -L "${SOCKET}" send-keys -t "${SESSION}" "$@"
}

result_count() {
  capture | grep -o '搜索结果 ([0-9]*)' | head -1 | grep -o '[0-9]*' || printf '0'
}

# Returns 0 when the tmux pane's process tree holds no GUI or script-host
# process. The tree is a point-in-time snapshot, so call it at more than one
# moment to widen the window it covers.
assert_no_gui_process() {
  local when="$1"
  local pane_pid process_tree

  pane_pid="$(tmux -L "${SOCKET}" list-panes -t "${SESSION}" -F '#{pane_pid}')"
  process_tree="$(ps -ax -o pid=,ppid=,comm= | python3 -c '
import sys

root = int(sys.argv[1])
children = {}
for line in sys.stdin:
    parts = line.split(None, 2)
    if len(parts) < 3:
        continue
    pid, ppid, comm = int(parts[0]), int(parts[1]), parts[2].strip()
    children.setdefault(ppid, []).append((pid, comm))

rows = []
queue = [root]
while queue:
    current = queue.pop(0)
    for pid, comm in children.get(current, []):
        rows.append(f"{pid} {comm}")
        queue.append(pid)
print("\n".join(rows))
' "${pane_pid}")"

  if [[ -z "${process_tree}" ]]; then
    reject "进程树可枚举（${when}）" "pane_pid=${pane_pid} 没有子进程"
    return 1
  fi

  printf '%s\n' "${process_tree}" | sed 's/^/        /'
  if printf '%s\n' "${process_tree}" | grep -Eq -- '\.app/|osascript|Automator|/usr/bin/open'; then
    reject "进程树里没有 GUI 或脚本宿主进程（${when}）" "${process_tree}"
    return 1
  fi
  return 0
}

# ---------------------------------------------------------------------------
# Preflight
# ---------------------------------------------------------------------------

[[ "$(uname -s)" == "Darwin" ]] || fail \
  "UNSUPPORTED_PLATFORM" \
  "OpenTUI 真实冒烟只在 macOS 上运行。" \
  "在有 Full Disk Access 的 macOS 机器上重跑。"

require_command bun
require_command tmux
require_command python3

[[ -d "${REPOSITORY_ROOT}/tui/node_modules" ]] || fail \
  "TUI_DEPENDENCIES_MISSING" \
  "tui/node_modules 缺失，OpenTUI 无法启动。" \
  "先运行 bun install --cwd tui。"

EXPORTER="$(resolve_backend)" || fail \
  "BACKEND_UNAVAILABLE" \
  "找不到可执行的 ${BINARY_NAME} binary。" \
  "运行 cargo build --release，或设置 APPLE_BOOKS_EXPORTER_BIN。"

[[ -x "${EXPORTER}" ]] || fail \
  "BACKEND_UNAVAILABLE" \
  "后端 binary 不可执行：${EXPORTER}" \
  "重新构建或设置 APPLE_BOOKS_EXPORTER_BIN。"

WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/tui-smoke.XXXXXX")"

# The TUI spawns the backend itself with piped stdio, so the backend's argv and
# stderr never reach this script. Wrap the binary in a shim that records every
# invocation; that log is what proves the browser stayed read-only.
BACKEND_LOG="${WORK_DIR}/backend-argv.log"
BACKEND_SHIM="${WORK_DIR}/backend-shim"
cat >"${BACKEND_SHIM}" <<EOF
#!/usr/bin/env bash
printf '%s\n' "\$*" >>"${BACKEND_LOG}"
exec "${EXPORTER}" "\$@"
EOF
chmod +x "${BACKEND_SHIM}"

printf 'Apple Books TUI 真实冒烟\n'
printf '  仓库     %s\n' "${REPOSITORY_ROOT}"
printf '  后端     %s（经 argv 记录 shim 包装）\n' "${EXPORTER}"
printf '  工作目录 %s\n' "${WORK_DIR}"

# ---------------------------------------------------------------------------
# Baseline: read the real library through the same machine protocol the TUI uses
# ---------------------------------------------------------------------------

note "读取真实书库基线"

if ! "${EXPORTER}" list --json >"${WORK_DIR}/books.json" 2>"${WORK_DIR}/list.err"; then
  cat "${WORK_DIR}/list.err" >&2
  fail \
    "BACKEND_LIST_FAILED" \
    "后端无法读取真实书库（通常是缺少 Full Disk Access）。" \
    "在 系统设置 → 隐私与安全性 → 完全磁盘访问权限 中授权当前终端，然后重跑。"
fi

if ! python3 - "${WORK_DIR}/books.json" >"${WORK_DIR}/baseline.txt" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    payload = json.load(handle)

books = [book for book in payload["books"] if book["note_count"] > 0]
if len(books) < 2:
    sys.exit("need at least two annotated books on this machine")

first, second = books[0], books[1]


def field(value):
    return value.replace("\n", " ").strip()


print(len(payload["books"]))
print(field(first["asset_id"]))
print(field(first["title"]))
print(field(first["author"]))
print(field(second["asset_id"]))
print(field(second["title"]))
print(field(second["author"]))
print(field(first["title"])[:3])
PY
then
  fail \
    "BACKEND_LIST_UNUSABLE" \
    "list --json 载荷不足以支撑冒烟（需要有标注的书籍少于两本）。" \
    "在 Apple Books 中添加更多带标注/高亮的书后重跑。"
fi

TOTAL_BOOKS="$(sed -n '1p' "${WORK_DIR}/baseline.txt")"
FIRST_ASSET_ID="$(sed -n '2p' "${WORK_DIR}/baseline.txt")"
FIRST_TITLE="$(sed -n '3p' "${WORK_DIR}/baseline.txt")"
FIRST_AUTHOR="$(sed -n '4p' "${WORK_DIR}/baseline.txt")"
SECOND_ASSET_ID="$(sed -n '5p' "${WORK_DIR}/baseline.txt")"
SECOND_TITLE="$(sed -n '6p' "${WORK_DIR}/baseline.txt")"
SECOND_AUTHOR="$(sed -n '7p' "${WORK_DIR}/baseline.txt")"
SEARCH_QUERY="$(sed -n '8p' "${WORK_DIR}/baseline.txt")"

# The list panel is 36 columns wide and the detail panel wraps, so a long real
# title or author is truncated on screen. Assert on a prefix that fits instead
# of requiring the whole string to be rendered.
TITLE_PREFIX="$(printf '%s' "${FIRST_TITLE}" | cut -c1-12)"
AUTHOR_PREFIX="$(printf '%s' "${FIRST_AUTHOR}" | cut -c1-12)"
SECOND_AUTHOR_PREFIX="$(printf '%s' "${SECOND_AUTHOR}" | cut -c1-12)"
SECOND_TITLE_PREFIX="$(printf '%s' "${SECOND_TITLE}" | cut -c1-12)"

printf '  书籍总数 %s\n' "${TOTAL_BOOKS}"
printf '  首本书   %s / %s / asset_id=%s\n' "${FIRST_TITLE}" "${FIRST_AUTHOR}" "${FIRST_ASSET_ID}"
printf '  第二本   %s / %s / asset_id=%s\n' "${SECOND_TITLE}" "${SECOND_AUTHOR}" "${SECOND_ASSET_ID}"
printf '  搜索词   %s\n' "${SEARCH_QUERY}"
printf '  断言前缀 %s / %s / %s / %s\n' \
  "${TITLE_PREFIX}" "${AUTHOR_PREFIX}" "${SECOND_TITLE_PREFIX}" "${SECOND_AUTHOR_PREFIX}"

# ---------------------------------------------------------------------------
# Launch in a dedicated tmux pty
# ---------------------------------------------------------------------------

note "在专用 tmux pty 中启动 TUI（${WIDE_COLUMNS}x${WIDE_ROWS}）"

tmux -L "${SOCKET}" kill-server >/dev/null 2>&1 || true
tmux -L "${SOCKET}" new-session -d -s "${SESSION}" \
  -x "${WIDE_COLUMNS}" -y "${WIDE_ROWS}" -c "${REPOSITORY_ROOT}"
tmux -L "${SOCKET}" set-option -g remain-on-exit on
tmux -L "${SOCKET}" send-keys -t "${SESSION}" \
  "APPLE_BOOKS_EXPORTER_BIN='${BACKEND_SHIM}' bun run --cwd tui start 2>'${WORK_DIR}/tui.err'; printf 'TUI_EXIT=%s\n' \"\$?\"; sleep 600" Enter

if ! wait_for_pattern "Apple Books · 终端浏览" "${READY_TIMEOUT_SECONDS}"; then
  printf '%s\n' "--- TUI stderr ---" >&2
  cat "${WORK_DIR}/tui.err" >&2 || true
  printf '%s\n' "--- last frame ---" >&2
  capture >&2 || true
  fail \
    "TUI_START_FAILED" \
    "OpenTUI 浏览器没有在 ${READY_TIMEOUT_SECONDS}s 内渲染出标题栏。" \
    "确认 bun 与 tui 依赖可用、后端 binary 可执行；必要时手动运行 bun run --cwd tui start 观察。"
fi

pass "TUI 在真实终端内启动并渲染标题栏，没有 GUI 窗口"

# ---------------------------------------------------------------------------
# Wide layout: real books and real annotations
# ---------------------------------------------------------------------------

note "宽屏布局：真实书籍列表与标注详情"

wait_for_pattern "搜索结果 (${TOTAL_BOOKS})" "${LOAD_TIMEOUT_SECONDS}" || true
FRAME="$(capture)"

if printf '%s' "${FRAME}" | grep -Fq -- "搜索结果 (${TOTAL_BOOKS})"; then
  pass "列表载入全部 ${TOTAL_BOOKS} 本真实书籍"
else
  reject "列表载入全部 ${TOTAL_BOOKS} 本真实书籍" \
    "$(printf '%s' "${FRAME}" | grep -o '搜索结果 ([0-9]*)' | head -1)"
fi

if printf '%s' "${FRAME}" | grep -Fq -- "${TITLE_PREFIX}"; then
  pass "列表显示真实书名：${FIRST_TITLE}"
else
  reject "列表显示真实书名：${FIRST_TITLE}" "${FRAME}"
fi

if printf '%s' "${FRAME}" | grep -Fq -- "作者：${AUTHOR_PREFIX}"; then
  pass "详情面板显示真实作者：${FIRST_AUTHOR}"
else
  reject "详情面板显示真实作者：${FIRST_AUTHOR}" "${FRAME}"
fi

# Placeholder text is （无正文）/（无笔记）, so requiring a non-empty value after
# the colon proves a real annotation body reached the screen.
if printf '%s' "${FRAME}" | grep -Eq -- '(正文|笔记)：[^（[:space:]]'; then
  pass "详情面板显示真实标注文本（正文或笔记正文）"
else
  reject "详情面板显示真实标注文本（正文或笔记正文）" "${FRAME}"
fi

if printf '%s' "${FRAME}" | grep -Eq -- '位置：epubcfi\('; then
  pass "详情面板显示真实 EPUB CFI 位置"
else
  reject "详情面板显示真实 EPUB CFI 位置" "${FRAME}"
fi

if printf '%s' "${FRAME}" | grep -Eq -- '时间：[0-9]{4}-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2} UTC'; then
  pass "详情面板显示真实标注时间"
else
  reject "详情面板显示真实标注时间" "${FRAME}"
fi

# ---------------------------------------------------------------------------
# Read-only guarantee
# ---------------------------------------------------------------------------

note "只读保证：只调用只读子命令，没有 GUI 子进程"

# The shim log is the only reliable record of which CLI subcommands the browser
# ran. Assert the exact set, so an empty log and an unexpected write command
# both fail instead of passing silently.
if [[ -s "${BACKEND_LOG}" ]]; then
  SUBCOMMANDS="$(awk '{print $1}' "${BACKEND_LOG}" | sort -u | tr '\n' ' ')"
  printf '%s\n' "$(cat "${BACKEND_LOG}")" | sed 's/^/        /'
  if [[ "${SUBCOMMANDS}" == "annotations list " ]]; then
    pass "浏览器只调用了只读子命令（${SUBCOMMANDS}）"
  else
    reject "浏览器只调用只读子命令（实际：${SUBCOMMANDS}）" "$(cat "${BACKEND_LOG}")"
  fi
else
  reject "后端 argv 日志非空（否则无法证明只读）" \
    "shim ${BACKEND_SHIM} 没有记录到任何后端调用"
fi

if assert_no_gui_process "浏览器启动后"; then
  pass "启动后进程树里没有 GUI 或脚本宿主进程"
fi

# ---------------------------------------------------------------------------
# Selection: the detail view follows the selected book
# ---------------------------------------------------------------------------

note "选择切换：详情跟随选中书籍"

send Down
if wait_for_pattern "作者：${SECOND_AUTHOR_PREFIX}" "${LOAD_TIMEOUT_SECONDS}"; then
  pass "按 ↓ 切到第二本后，详情切换为 ${SECOND_TITLE} / ${SECOND_AUTHOR}"
else
  reject "按 ↓ 切到第二本后，详情切换为 ${SECOND_TITLE} / ${SECOND_AUTHOR}" "$(capture)"
fi

FRAME="$(capture)"
if printf '%s' "${FRAME}" | grep -Fq -- "${SECOND_TITLE_PREFIX}"; then
  pass "第二本书名在终端帧中出现：${SECOND_TITLE}"
else
  reject "第二本书名在终端帧中出现：${SECOND_TITLE}" "${FRAME}"
fi

# ---------------------------------------------------------------------------
# Detail focus and back
# ---------------------------------------------------------------------------

note "Enter 聚焦详情 / Esc 返回列表"

send Enter
sleep 1
FRAME="$(capture)"
if printf '%s' "${FRAME}" | grep -Fq -- "↑↓ 滚动  Esc 返回列表  q 退出"; then
  pass "Enter 后页脚切换到详情滚动模式"
else
  reject "Enter 后页脚切换到详情滚动模式" "$(printf '%s' "${FRAME}" | tail -3)"
fi

send Escape
sleep 1
FRAME="$(capture)"
if printf '%s' "${FRAME}" | grep -Fq -- "Enter 详情  Tab 切换  q 退出"; then
  pass "Esc 后页脚回到列表浏览模式"
else
  reject "Esc 后页脚回到列表浏览模式" "$(printf '%s' "${FRAME}" | tail -3)"
fi

# ---------------------------------------------------------------------------
# Compact layout
# ---------------------------------------------------------------------------

note "紧凑布局：${COMPACT_COLUMNS} 列单屏切换"

tmux -L "${SOCKET}" resize-window -t "${SESSION}" -x "${COMPACT_COLUMNS}" -y "${WIDE_ROWS}"
sleep 2

FRAME="$(capture)"
if printf '%s' "${FRAME}" | grep -Fq -- "${SECOND_TITLE_PREFIX}"; then
  pass "紧凑模式列表屏仍显示真实书籍"
else
  reject "紧凑模式列表屏仍显示真实书籍" "${FRAME}"
fi

if printf '%s' "${FRAME}" | grep -Fq -- "标注详情"; then
  reject "紧凑列表屏隐藏标注详情面板" "${FRAME}"
else
  pass "紧凑列表屏隐藏标注详情面板"
fi

if printf '%s' "${FRAME}" | grep -Fq -- "搜索  "; then
  pass "紧凑列表屏保留搜索框"
else
  reject "紧凑列表屏保留搜索框" "${FRAME}"
fi

send Enter
if wait_for_pattern "↑↓ 滚动  Esc 返回列表  q 退出" "${LOAD_TIMEOUT_SECONDS}"; then
  pass "紧凑模式 Enter 切到详情屏"
else
  reject "紧凑模式 Enter 切到详情屏" "$(capture)"
fi

FRAME="$(capture)"
if printf '%s' "${FRAME}" | grep -Fq -- "标注详情"; then
  pass "紧凑详情屏显示标注详情面板"
else
  reject "紧凑详情屏显示标注详情面板" "${FRAME}"
fi

if printf '%s' "${FRAME}" | grep -Fq -- "搜索结果 ("; then
  reject "紧凑详情屏隐藏书籍列表面板" "${FRAME}"
else
  pass "紧凑详情屏隐藏书籍列表面板"
fi

send Escape
sleep 1
FRAME="$(capture)"
if printf '%s' "${FRAME}" | grep -Fq -- "搜索结果 (" && ! printf '%s' "${FRAME}" | grep -Fq -- "标注详情"; then
  pass "紧凑模式 Esc 回到列表屏"
else
  reject "紧凑模式 Esc 回到列表屏" "${FRAME}"
fi

# ---------------------------------------------------------------------------
# Search
#
# The query is intentionally persistent: Esc only hands focus back to the list,
# and the always-visible search bar plus the 搜索结果 (N) title keep the active
# filter obvious. The query is cleared by editing the text.
# ---------------------------------------------------------------------------

note "搜索过滤（查询词跨 Esc 保留，需退格清除）"

tmux -L "${SOCKET}" resize-window -t "${SESSION}" -x "${WIDE_COLUMNS}" -y "${WIDE_ROWS}"
sleep 1

send "/"
sleep 1
send -l -- "${SEARCH_QUERY}"

if wait_for_pattern "搜索  ${SEARCH_QUERY}" "${LOAD_TIMEOUT_SECONDS}"; then
  pass "搜索词进入搜索框：${SEARCH_QUERY}"
else
  reject "搜索词进入搜索框：${SEARCH_QUERY}" "$(capture)"
fi
sleep 1

FILTERED_COUNT="$(result_count)"
if (( FILTERED_COUNT > 0 && FILTERED_COUNT < TOTAL_BOOKS )); then
  pass "搜索把结果从 ${TOTAL_BOOKS} 缩小到 ${FILTERED_COUNT}"
else
  reject "搜索把结果从 ${TOTAL_BOOKS} 缩小到 ${FILTERED_COUNT}" \
    "$(capture | grep -o '搜索结果 ([0-9]*)' | head -1)"
fi

FRAME="$(capture)"
if printf '%s' "${FRAME}" | grep -Fq -- "${TITLE_PREFIX}"; then
  pass "搜索结果保留匹配书籍：${FIRST_TITLE}"
else
  reject "搜索结果保留匹配书籍：${FIRST_TITLE}" "${FRAME}"
fi

send Escape
sleep 1
FRAME="$(capture)"
if printf '%s' "${FRAME}" | grep -Fq -- "搜索  ${SEARCH_QUERY}"; then
  pass "Esc 之后搜索框仍显示查询词，过滤状态可见"
else
  reject "Esc 之后搜索框仍显示查询词，过滤状态可见" "$(printf '%s' "${FRAME}" | sed -n '5p')"
fi

# The real signal that Esc handed focus back to the list: a following "/" must
# re-focus the search box instead of being typed into the query as a literal.
send "/"
sleep 1
FRAME="$(capture)"
if printf '%s' "${FRAME}" | grep -Fq -- "搜索  ${SEARCH_QUERY}" && ! printf '%s' "${FRAME}" | grep -Fq -- "搜索  ${SEARCH_QUERY}/"; then
  pass "Esc 把焦点交回列表：随后的 / 重新聚焦搜索框而非插入字面量斜杠"
else
  reject "Esc 把焦点交回列表：随后的 / 重新聚焦搜索框而非插入字面量斜杠" \
    "$(printf '%s' "${FRAME}" | sed -n '5p')"
fi

# Clear the query: the previous step already re-focused the search box, so
# pressing "/" again would type a literal slash into the query instead.
QUERY_LENGTH="$(printf '%s' "${SEARCH_QUERY}" | wc -m | tr -d ' ')"
for (( i = 0; i < QUERY_LENGTH; i++ )); do
  send BSpace
done

if wait_for_pattern "搜索结果 (${TOTAL_BOOKS})" "${LOAD_TIMEOUT_SECONDS}"; then
  pass "退格清空查询后恢复 ${TOTAL_BOOKS} 本书籍"
else
  reject "退格清空查询后恢复 ${TOTAL_BOOKS} 本书籍" \
    "$(capture | grep -o '搜索结果 ([0-9]*)' | head -1)"
fi

# ---------------------------------------------------------------------------
# Clean quit
# ---------------------------------------------------------------------------

note "干净退出"

# Second snapshot, so the no-GUI claim spans the whole interactive session
# rather than only the moment right after startup.
if assert_no_gui_process "退出前"; then
  pass "退出前进程树里仍没有 GUI 或脚本宿主进程"
fi

send Escape
sleep 1
send "q"

if wait_for_pattern "TUI_EXIT=0" "${LOAD_TIMEOUT_SECONDS}"; then
  pass "q 退出 TUI，进程退出码 0"
else
  reject "q 退出 TUI，进程退出码 0" "$(capture | tail -3)"
fi

if [[ -s "${WORK_DIR}/tui.err" ]]; then
  printf '%s\n' "TUI stderr:" >&2
  cat "${WORK_DIR}/tui.err" >&2
fi

# ---------------------------------------------------------------------------

printf '\n%s\n' "----------------------------------------"
printf '通过 %s / %s\n' "$((CHECKS - FAILURES))" "${CHECKS}"
if (( FAILURES > 0 )); then
  fail \
    "TUI_SMOKE_FAILED" \
    "OpenTUI 真实冒烟有 ${FAILURES} 项断言失败。" \
    "对照上面的失败帧定位。"
fi

printf 'OpenTUI 真实冒烟全部通过：纯终端渲染真实 Apple Books 数据，无 GUI、无写入\n'
