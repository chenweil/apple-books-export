# Annotation Speech Implementation Spec

- 状态：已确认，待实现
- 日期：2026-09-11
- 决策依据：[`ADR 0007`](../adr/0007-annotation-speech-generation-boundary.md)
- API 依据：[`SenseAudio TTS API 调研`](2026-09-10-senseaudio-tts-api-research.md)
- 领域语言：[`CONTEXT.md`](../../CONTEXT.md)
- 首个实现目标：Rust Headless Mainline 的 `speech` CLI 与 Machine JSON Protocol

## 1. 目标

让用户对 Apple Books Annotation 的高亮或个人笔记单独生成、缓存、播放并导出 MP3，支持
选择音色及其可用情感/风格变体，并调节语速、音量和声调。首个 Speech Provider 是
SenseAudio，产品合同不直接暴露其 HTTP JSON。

完成态必须同时满足：

- 一次生成只处理一个 `highlight` 或 `note` Speech Clip；
- 人类 CLI 可用显示序号，Machine 调用只用稳定 ID；
- 只有显式 `speech generate` 可以发送 Annotation 内容并产生付费请求；
- 相同请求复用缓存，未知结果不会自动重试；
- Cached Speech Clip 与 Exported Speech Clip 生命周期分离；
- Markdown/Obsidian 只链接已导出且校验有效的 active 音频；
- TUI、Agent Data Skill 和 AppKit 首期不获得 TTS 调用能力；
- 现有 `list`、`annotations`、`export` 和 `doctor` Machine JSON 合同保持兼容。

## 2. 非目标

- 不实现章节、整本书或后台批量生成；
- 不实现高亮与笔记的合并音频；
- 不实现 SSE、WebSocket 或边生成边播放；
- 不实现语音克隆、文生音色或自动选音色；
- 不自动翻译、口语化改写、总结或截断 Speech Text；
- 不让 Markdown 导出、播放、缓存修复或 Agent Data Skill 隐式调用 provider；
- 不开放 MP3/32000Hz/128kbps/双声道以外的用户音频规格；
- 不把 Voice Profile 假定为跨 Speech Provider 可移植。

## 3. 模块边界

建议新增以下 Rust 模块；实际拆分可以微调，但责任不能合并回 LLM provider/cache：

```text
src/speech.rs                 # 领域类型、use case、公共入口
src/speech/provider.rs        # SpeechProvider seam
src/speech/senseaudio.rs      # SenseAudio 请求、响应、音色和错误映射
src/speech/store.rs           # config、catalog、clip、attempt、lock、LRU
src/speech/export.rs          # Speech Export Manifest 与音频导出
src/speech/machine.rs         # Speech Receipt、warning 和错误 details
```

现有模块的职责变化：

- [`src/db.rs`](../../src/db.rs)：继续是 Annotation 内容事实源，不保存 speech 状态；
- [`src/models.rs`](../../src/models.rs)：可导出必要公共 speech 类型，但不混入 SenseAudio DTO；
- [`src/provider.rs`](../../src/provider.rs)：继续只负责 OpenAI-compatible LLM，不复用；
- [`src/cache.rs`](../../src/cache.rs)：继续只负责 LLM 结果，不保存音频；
- [`src/machine.rs`](../../src/machine.rs)：复用 envelope、schema 和稳定错误约定；
- [`src/exporter.rs`](../../src/exporter.rs)：只读取 Speech Export Manifest 并渲染有效链接，
  不生成或复制音频；
- [`src/main.rs`](../../src/main.rs)：只做 clap 参数解析、human/machine 分流和 use case 组装。

依赖方向：

```text
CLI / future AppKit
        ↓
Speech use cases
   ↙            ↘
DB read core     Speech store / export manifest
        ↓
SpeechProvider trait
        ↓
SenseAudio adapter
```

SenseAudio adapter 不能依赖 CLI、Markdown exporter 或 Apple Books SQLite。

## 4. 领域合同

### 4.1 内容选择

```rust
enum SpeechContentKind {
    Highlight,
    Note,
}
```

Machine 请求必须提供：

```text
asset_id + annotation_id + content_kind
```

解析规则：

- `highlight` 只读取 `Annotation.selected_text`；
- `note` 只读取 `Annotation.note`；
- 目标字段缺失或 trim 后为空，返回 `SPEECH_CONTENT_UNAVAILABLE`；
- `annotation_id` 不属于给定 `asset_id`，返回 `INVALID_ANNOTATION_ID`；
- 不读取 `annotation_type`，也不使用 Machine DTO 的 `type` 推断 content kind；
- 文本超过 10000 Unicode scalar values 时，在联网前返回 `SPEECH_TEXT_TOO_LONG`。

### 4.2 Speech Text

规范化顺序固定为：

1. `CRLF` 与单独 `CR` 转为 `LF`；
2. 删除整个字符串边界的 Unicode whitespace；
3. 保留内部空格、换行和段落；
4. 不删除 URL、emoji、标点、括号、代码或脚注；
5. 不翻译、不改写、不总结；
6. 将 provider 控制标记视为普通内容，不能让 Annotation 中的 `<break>` 等文本直接控制请求。

SenseAudio 未公开普通文本中控制标记的可靠 escape 规则，因此实现前的 provider spike 必须
验证一种不产生控制效果的编码方式。验证完成前，该事实保持在 adapter 内，不能通过删除原文
或静默插入停顿绕过。

缓存、日志、receipt、attempt history 和 export manifest 不保存 Speech Text，只保存：

- `text_sha256`；
- `unicode_characters`；
- `estimated_billing_characters`。

计费字符估算由 provider adapter 的版本化 estimator 产生，receipt 同时记录
`billing_estimator_version`。SenseAudio 首版按调研时官方规则实现，但字段名称必须保留
“estimated”；不能把估算值或 `extra_info.usage_characters` 宣称为最终账单，也不在程序中
硬编码人民币价格。

### 4.3 Voice Profile

产品层结构：

```text
VoiceProfile {
  provider: "senseaudio",
  model: "sensenova-tts-2.0",
  voice_id: "male_0004_a",
  emotion_label?: string,
  style_label?: string,
  speed: 1.0,
  volume: 1.0,
  pitch: 0,
  audio: {
    format: "mp3",
    sample_rate: 32000,
    bitrate: 128000,
    channel: 2
  }
}
```

校验规则：

- `speed`：`[0.5, 2.0]`，最多两位小数，内部规范化为 `speed_x100`；
- `volume`：`[0.01, 10.0]`，最多两位小数，内部规范化为 `volume_x100`，adapter 映射为
  SenseAudio `vol`；
- `pitch`：整数 `[-12, 12]`；
- 所有数值拒绝 NaN、infinity 和隐式四舍五入；
- `voice_id` 必须是精确 ID，不按名称或标签模糊匹配；
- `emotion_label` 和 `style_label` 是 provider-owned 展示元数据，不进入 SenseAudio 请求；
- 当前 Voice Catalog 无法取得时，Profile 可以保存为 `unverified`；
- `unverified` Profile 可以参与 clip ID 计算并命中已有 cache/export，但不能创建 Speech Attempt；
  只有即将调用 provider 时才必须刷新或验证 voice availability；
- 默认 Profile 使用上面的值；`male_0004_a` 不可用时返回
  `SPEECH_VOICE_UNAVAILABLE`，不自动选择其他音色。

### 4.4 Clip 与 Attempt 身份

`clip_id` 是 canonical fingerprint payload 的 SHA-256 小写 hex。payload 必须使用版本化、
字段顺序固定的类型，禁止对无序 map 直接序列化：

```text
fingerprint_version = 1
speech_text_policy_version = 1
provider
model
asset_id
annotation_id
content_kind
normalized_speech_text
voice_id
speed_x100            # 50..200
volume_x100           # 1..1000
pitch
audio_format
sample_rate
bitrate
channel
```

情感/风格展示标签不进入 fingerprint；实际影响声音的是 resolved `voice_id`。CLI 输入的
`speed` 和 `volume` 必须能精确表示为百分之一单位，禁止浮点四舍五入制造相同显示值、不同
fingerprint。若新增影响 provider payload 的字段，或改变文本规范化/控制标记 escape 规则，
必须升级对应 policy/fingerprint version。

`attempt_id` 是每次真实 provider 请求的独立 opaque ID：

- cache hit 和 export rehydration 不创建 attempt；
- `--regenerate` 保持同一 `clip_id`，创建新 `attempt_id`；
- receipt 中 `attempt_id` 可为空；
- attempt history 不保存 Speech Text、API Key 或音频。

## 5. CLI 合同

### 5.1 命令族

```text
apple-books-exporter speech voices
apple-books-exporter speech profile show
apple-books-exporter speech profile set
apple-books-exporter speech profile reset
apple-books-exporter speech generate
apple-books-exporter speech play
apple-books-exporter speech export
apple-books-exporter speech cache status
apple-books-exporter speech cache clear
apple-books-exporter speech history clear
```

每个需要机器响应的叶命令支持 `--json`；成功 JSON 只写 stdout，失败 JSON 只写 stderr。

### 5.2 Voices

```bash
apple-books-exporter speech voices [--refresh] [--json]
```

- 未过期时读取 24 小时 Voice Catalog 缓存；
- 过期时刷新，`--refresh` 强制刷新；
- 刷新失败且存在旧目录时，人类输出和 JSON 均标记 `stale=true`、`fetched_at` 与 warning；
- stale catalog 可以展示，不能单独证明当前账号仍有生成权限；
- 调用 `/v1/get_voice` 显式发送 `{"voice_type":"all"}`；
- 响应按 `system_voice`、`voice_cloning`、`voice_generation` 转成 provider-neutral
  `system`、`cloned`、`generated` 来源类型；
- human 输出按 `voice_name` 分组，并同时显示每个具体 `voice_id` 的情感/风格标签；用户选择的
  仍是唯一具体 ID，不生成目录中不存在的标签组合。

### 5.3 Profile

```bash
apple-books-exporter speech profile show [--json]

apple-books-exporter speech profile set \
  [--provider senseaudio] \
  [--model sensenova-tts-2.0] \
  --voice-id male_0004_a \
  [--speed 1.0] [--volume 1.0] [--pitch 0] \
  [--json]

apple-books-exporter speech profile reset [--json]
```

- Profile 文件只保存非秘密字段和 `api_key_env` 名称；
- SenseAudio 默认 `api_key_env` 为 `SENSEAUDIO_API_KEY`；
- `set` 做本地范围校验，并在 catalog 可用时校验 `voice_id`；
- 无法验证时保存 `verification_status=unverified` 并返回 warning；
- `reset` 恢复默认 Profile，不创建网络请求。

### 5.4 Generate

Machine 形式：

```bash
apple-books-exporter speech generate \
  --asset-id BOOK_ID \
  --annotation-id ANNOTATION_ID \
  --content highlight \
  [--voice-id VOICE_ID] [--speed 1.0] [--volume 1.0] [--pitch 0] \
  [--export-root BOOK_EXPORT_DIRECTORY] \
  [--regenerate] \
  --json
```

Human 形式：

```bash
apple-books-exporter speech generate BOOK_INDEX \
  --annotation ANNOTATION_INDEX \
  --content note \
  [--voice-id VOICE_ID] [--speed 1.0] [--volume 1.0] [--pitch 0] \
  [--export-root BOOK_EXPORT_DIRECTORY] \
  [--regenerate]
```

约束：

- `--json` 只接受稳定 ID，不接受 positional book index 或 annotation index；
- human 模式不接受 `--asset-id`/`--annotation-id`；
- 冲突参数返回 `INVALID_ARGUMENT`，不能忽略其中一组；
- 未提供 override 时使用全局 Profile；
- human 输出显示 content kind、voice ID 和字符估算，不输出完整 Speech Text；
- 普通生成优先 cache，然后从显式 `--export-root` 或已验证 locator 尝试 export rehydration，
  最后才允许 provider 请求；
- 无有效 cache 且存在 unknown gate 时，普通生成返回 `SPEECH_RESULT_UNKNOWN`；
- `--regenerate` 是唯一可以越过 unknown gate 或替换有效 cache 的入口。

### 5.5 Play

```bash
apple-books-exporter speech play --clip-id CLIP_ID \
  [--export-root BOOK_EXPORT_DIRECTORY] [--json]
```

- human 模式验证音频后调用 macOS `afplay`；
- `--json` 不启动播放器，只返回将被播放的有效路径和来源；
- 查找顺序：Speech Cache Entry → 显式 `--export-root` → 已验证 export locator 中 checksum 匹配的
  Active Exported Speech Clip；
- 校验不匹配的用户文件不作为 clip fallback，但用户仍可自行用系统播放器打开；
- 找不到时返回 `SPEECH_CLIP_NOT_FOUND`；
- 任何 play 路径都不得调用 Speech Provider。

### 5.6 Export

```bash
apple-books-exporter speech export \
  --clip-id CLIP_ID \
  --output BOOK_EXPORT_DIRECTORY \
  [--overwrite] \
  [--json]
```

- `--output` 指向已选择书籍的导出根目录，不是 `assets/audio/` 本身；
- 输入 cache entry 必须存在并通过 checksum/format 验证；
- 目标文件名为 `highlight-<prefix>.mp3` 或 `note-<prefix>.mp3`；
- prefix 初始为完整 clip ID 前 12 位；检测到 manifest 中存在不同 full clip ID 时延长；
- 相同 clip、相同 checksum 直接复用；
- 同路径内容不同默认 `SPEECH_OUTPUT_FILE_EXISTS`，只有 `--overwrite` 可以替换；
- 成功后将该 annotation/content kind 的 clip 标为 active，保留旧 variant 文件；
- manifest 成功提交后更新非权威 export locator；locator 更新失败返回 warning，不回滚已经
  成功提交且自包含的导出目录；
- 不修改已有 Markdown。

### 5.7 Cache 与 History

```bash
apple-books-exporter speech cache status [--json]
apple-books-exporter speech cache clear [--json]
apple-books-exporter speech history clear [--json]
```

- `cache status` 返回 budget、used bytes、entry count、locked count 与 corrupt count；
- `cache clear` 删除可淘汰 Cached Speech Clip，跳过正在生成、播放、导出或持锁 entry；
- receipt 返回 `removed` 与 `skipped`；
- `history clear` 只删除 attempt history，不清缓存、unknown gate 或用户导出；
- 正常维护自动删除超过 90 天的 attempt history。

## 6. Machine JSON Protocol

### 6.1 Generate receipt

```json
{
  "schema_version": 1,
  "receipt": {
    "operation": "generate",
    "clip_id": "<64-char sha256>",
    "attempt_id": "<opaque-id-or-null>",
    "source": "provider",
    "provider_called": true,
    "asset_id": "...",
    "annotation_id": "annotation-41",
    "content_kind": "highlight",
    "text_sha256": "<sha256>",
    "unicode_characters": 42,
    "estimated_billing_characters": 70,
    "billing_estimator_version": "senseaudio-docs-2026-09-10",
    "profile": {
      "provider": "senseaudio",
      "model": "sensenova-tts-2.0",
      "voice_id": "male_0004_a",
      "emotion_label": "平稳",
      "style_label": null,
      "speed": 1.0,
      "volume": 1.0,
      "pitch": 0
    },
    "audio": {
      "path": "/absolute/path/audio.mp3",
      "sha256": "...",
      "format": "mp3",
      "size_bytes": 12345,
      "duration_ms": 4200,
      "sample_rate": 32000,
      "bitrate": 128000,
      "channel": 2
    },
    "provider": {
      "trace_id": "...",
      "usage_characters": 70
    },
    "warnings": []
  }
}
```

`source` 可选值：

- `cache`：复用现有 Speech Cache Entry；
- `export_rehydration`：从有效 Exported Speech Clip 恢复缓存；
- `provider`：创建了 Speech Attempt。

只有 `source=provider` 时 `provider_called=true` 且 `attempt_id` 必填。receipt 永远不包含
Speech Text、API Key、音频 hex 或供应商完整原始响应。

### 6.2 Error envelope

复用现有 envelope，并新增可选 `details`：

```json
{
  "schema_version": 1,
  "error": {
    "code": "SPEECH_RESULT_UNKNOWN",
    "message": "The provider may have processed the request.",
    "remediation": "Retry with --regenerate only if another billed generation is acceptable.",
    "details": {
      "provider": "senseaudio",
      "provider_code": null,
      "trace_id": null,
      "attempt_id": "...",
      "outcome": "unknown"
    }
  }
}
```

首版稳定错误码：

| Code | 含义 |
| --- | --- |
| `INVALID_ASSET_ID` | 书籍身份不存在或缺失 |
| `INVALID_ANNOTATION_ID` | Annotation 不存在或不属于该书 |
| `INVALID_ARGUMENT` | human/machine 参数冲突或枚举无效 |
| `SPEECH_CONTENT_UNAVAILABLE` | 目标高亮或笔记为空 |
| `SPEECH_TEXT_TOO_LONG` | 规范化文本超过 10000 字符 |
| `SPEECH_PROFILE_INVALID` | Profile 数值或结构无效 |
| `SPEECH_VOICE_UNAVAILABLE` | voice ID 当前不可用 |
| `SPEECH_AUTH_FAILED` | API Key 缺失或认证失败 |
| `SPEECH_RATE_LIMITED` | provider 限流 |
| `SPEECH_PROVIDER_FAILED` | provider 明确失败 |
| `SPEECH_RESULT_UNKNOWN` | 请求可能已处理但没有可接受结果 |
| `SPEECH_IN_PROGRESS` | 同 clip writer 锁等待超时 |
| `SPEECH_AUDIO_INVALID` | provider 成功响应无法形成有效 MP3 |
| `SPEECH_ARTIFACT_COMMIT_FAILED` | provider 已成功但本地原子提交失败 |
| `SPEECH_CACHE_CORRUPT` | cache metadata/audio/checksum 不一致 |
| `SPEECH_STORAGE_UNAVAILABLE` | Speech 根目录不可写或空间不足 |
| `SPEECH_OUTPUT_FILE_EXISTS` | 导出目标冲突且未允许覆盖 |
| `SPEECH_EXPORT_MANIFEST_INVALID` | manifest 结构、身份或路径不可信 |
| `SPEECH_CLIP_NOT_FOUND` | cache 与 active export 都没有有效 clip |

provider 成功但 MP3 校验或本地提交失败时，不允许普通 `generate` 自动重放；该 clip 进入
需要显式 `--regenerate` 的阻塞状态，`details.outcome` 标明
`provider_succeeded_artifact_missing`。

## 7. 本地状态与文件 schema

### 7.1 根目录

```text
~/Library/Application Support/books-exporter/speech/
├── config.json
├── exports.json                         # 非权威 export-root locator 投影
├── voices/
│   └── senseaudio.json
├── clips/
│   └── <clip_id>/
│       ├── state.json                   # 原子 current pointer / unknown gate
│       └── versions/
│           └── <audio_sha256>/
│               ├── metadata.json
│               └── audio.mp3
├── attempts/
│   └── YYYY-MM-DD/
│       └── <attempt_id>.json
├── locks/
│   └── <clip_id>.lock
└── tmp/
```

不得根据当前工作目录、仓库路径或 `--config` 改变 Speech 根目录。测试通过显式注入的临时
根目录隔离，不能写真实 Application Support。

### 7.2 Clip state

`state.json` 至少记录：

```text
schema_version
clip_id
asset_id
annotation_id
content_kind
text_sha256
current_cache_status      # ready | absent | corrupt
current_audio_sha256?
latest_attempt_id?
latest_attempt_status?    # succeeded | failed | unknown | cancelled_before_send
latest_error_code?
generation_blocked        # 无 cache 的 unknown/provider-succeeded-no-artifact 时为 true
updated_at
```

- 初次 attempt 成功：先原子放置不可变 version 目录，再原子提交指向它的 ready state；
- 初次 attempt unknown：提交 state-only unknown gate；
- regenerate 期间保留旧 ready audio；
- regenerate 失败/unknown：旧 cache 仍 ready，state 记录 latest attempt warning；
- regenerate 成功：原子放置新 version 后切换 `current_audio_sha256`；旧 version 只在 pointer
  提交成功后进入垃圾回收；
- `history clear` 不清 `generation_blocked`；
- 只有 `--regenerate` 成功或显式清除 unknown gate 才解除阻塞。

version 目录一旦可见就不可原地修改。崩溃可能留下未被 `state.json` 引用的 version；维护过程
可以报告并在确认没有 lock/reference 后回收，但不得根据目录时间猜测 current version。

### 7.3 Attempt history

Attempt metadata 至少记录：

```text
schema_version
attempt_id
clip_id
provider
model
voice_id
started_at
finished_at?
status
unicode_characters
estimated_billing_characters
provider_usage_characters?
product_error_code?
provider_code?
trace_id?
```

允许状态：`cancelled_before_send`、`provider_failed`、`succeeded`、`unknown`、
`provider_succeeded_artifact_missing`。不保存原文、请求体、响应体、API Key 或音频。

### 7.4 Cache budget

- 默认 budget：1 GiB，可由非秘密 speech config 调整；
- provider 调用前验证根目录可写并保留至少 128 MiB 安全余量；
- 写入前先清理已过 budget 且可淘汰的旧 entry；
- 新 entry 接受后再执行 LRU，使总量回到 budget；
- 当前 clip 以及生成、播放、导出或持锁 entry 不可淘汰；
- 无法获得安全空间时，在 provider 调用前返回 `SPEECH_STORAGE_UNAVAILABLE`；
- 所有临时文件必须位于同一文件系统，保证 rename 原子性。

### 7.5 Export locator projection

`exports.json` 只记录 `asset_id`、最近验证过的绝对 export root、manifest schema/digest 和
`last_verified_at`，用于定位可能被用户导出的 clip。它是可丢弃的便利投影，不是 Active
Exported Speech Clip 的事实源：

- `speech export` 成功提交 manifest 后更新 locator；
- `speech play` 和 rehydration 必须重新读取目标根的 manifest 并验证 clip ID、relative path
  containment 和 checksum；
- locator 路径不存在、manifest 不匹配或 checksum 失败时视为 stale，不猜测新路径；
- 用户移动导出目录后可以通过 `--export-root` 提供新位置；验证成功后刷新 locator；
- 不递归扫描 home、Documents、iCloud 或其他目录寻找 manifest；
- manifest 保持自包含，移动后的目录仍可由显式路径独立验证。

## 8. SenseAudio adapter

### 8.1 Voice Catalog

```http
POST https://api.senseaudio.cn/v1/get_voice
Authorization: Bearer $SENSEAUDIO_API_KEY
Content-Type: application/json

{"voice_type":"all"}
```

adapter 将三组供应商音色转换为 provider-neutral catalog entry：

```text
provider
source_type            # system | cloned | generated
voice_id
voice_name
emotion_label?
style_label?
description[]
created_time?
```

不从 `voice_id` 后缀猜测情感；标签来自官方目录/响应描述和受版本管理的映射。静态映射不能
把当前账号没有返回的 voice 标记为 available。

### 8.2 同步合成

```http
POST https://api.senseaudio.cn/v1/t2a_v2
Authorization: Bearer $SENSEAUDIO_API_KEY
Content-Type: application/json
```

请求固定：

```json
{
  "model": "sensenova-tts-2.0",
  "text": "<provider-safe speech text>",
  "stream": false,
  "voice_setting": {
    "voice_id": "male_0004_a",
    "speed": 1.0,
    "vol": 1.0,
    "pitch": 0
  },
  "audio_setting": {
    "format": "mp3",
    "sample_rate": 32000,
    "bitrate": 128000,
    "channel": 2
  }
}
```

响应处理顺序：

1. 检查 HTTP status；
2. 解析 HTTP error 或成功 envelope；
3. 要求 `base_resp.status_code == 0`；
4. 要求 `data.audio` 非空；
5. 严格 hex 解码；
6. 写入同文件系统临时目录；
7. 验证文件非空、MP3 可解析、声明格式/采样率/码率/声道不矛盾；
8. 记录 trace ID 和 provider usage；
9. 原子放置不可变 version，并原子切换 clip state 的 current pointer。

不得把 `response.json` 或原始 hex 持久化。日志只保留 ID、状态、长度和 trace，不输出 text
或 Authorization header。

### 8.3 Provider 错误映射

- 无环境变量：`SPEECH_AUTH_FAILED`，并保证 mock server 调用数为 0；
- HTTP 401：`SPEECH_AUTH_FAILED`；
- HTTP 429 或明确限流 code：`SPEECH_RATE_LIMITED`；
- HTTP 4xx/5xx 且 provider 明确返回失败：`SPEECH_PROVIDER_FAILED`；
- 请求可能到达后发生 timeout、连接断开或无法判定响应：`SPEECH_RESULT_UNKNOWN`；
- HTTP/API 成功但 hex/MP3 无效：`SPEECH_AUDIO_INVALID`，outcome 为
  `provider_succeeded_artifact_missing`；
- provider 成功但文件提交失败：`SPEECH_ARTIFACT_COMMIT_FAILED`，同样阻止普通自动重放。

provider code、trace ID、attempt ID 和 outcome 进入 error `details`；原始 body 不进入稳定协议。

## 9. 生成状态流

```text
resolve annotation/content
        ↓
normalize + validate Speech Text
        ↓
resolve + local-validate Voice Profile
        ↓
compute clip_id
        ↓
acquire cross-process clip lock
        ↓
valid cache? ── yes ──> receipt(source=cache)
        │ no
valid export via explicit root / locator? ── yes ──> rehydrate ──> receipt(source=export_rehydration)
        │ no
unknown gate? ── yes and no --regenerate ──> SPEECH_RESULT_UNKNOWN
        │ no
provider voice availability + storage + API key preflight
        ↓
create attempt record
        ↓
call SenseAudio synchronously
   ┌────┼───────────────┐
 success explicit fail  uncertain
   │       │             │
validate  record fail   record unknown gate
   │
stage immutable audio version + metadata
   ↓
atomic version placement
   ↓
atomic current state switch
   ↓
LRU maintenance
   ↓
receipt(source=provider)
```

同一 clip 的第二个进程等待现有 lock。首个完成后重新检查 state：

- ready：返回 cache hit；
- failed：返回相同稳定失败，不自动调用 provider；
- unknown：返回 `SPEECH_RESULT_UNKNOWN`；
- 等待超时：返回 `SPEECH_IN_PROGRESS`。

## 10. Speech Export Manifest

路径：

```text
<book-export-directory>/assets/audio/manifest.json
```

建议 schema：

```json
{
  "schema_version": 1,
  "asset_id": "BOOK_ID",
  "records": [
    {
      "annotation_id": "annotation-41",
      "content_kind": "highlight",
      "active_clip_id": "<full clip id>",
      "clips": [
        {
          "clip_id": "<full clip id>",
          "relative_path": "assets/audio/highlight-ab12cd34ef56.mp3",
          "sha256": "...",
          "size_bytes": 12345,
          "format": "mp3",
          "exported_at": "2026-09-11T00:00:00Z"
        }
      ]
    }
  ]
}
```

manifest 规则：

- `asset_id` 必须与正在导出的书一致；
- `relative_path` 必须规范化并保持在书籍导出根目录内；
- 同一 `annotation_id + content_kind` 只有一个 `active_clip_id`；
- 旧 clip 可保留在 `clips`，但普通 Markdown 只链接 active；
- 不保存原文、API Key、绝对路径或供应商原始响应；
- speech export 先验证并原子放置 MP3，再原子替换 manifest；
- manifest 损坏时 speech export 失败，不重建或覆盖；
- 主 Markdown 导出遇到损坏 manifest、缺失文件或 checksum mismatch 时省略链接并返回 warning，
  不让整本导出失败；
- orphan audio 没有 manifest 引用，不会被 Markdown 自动链接；后续维护命令可以报告但不自动删除。

Markdown 渲染：

```md
> 高亮内容

[▶ 播放高亮语音](assets/audio/highlight-ab12cd34ef56.mp3)
```

Obsidian 渲染：

```md
**笔记**：我的笔记

![[assets/audio/note-ef56ab789012.mp3]]
```

链接必须紧跟对应内容部分；高亮和笔记不能共享一个链接。

## 11. 验证计划

### 11.1 单元测试

- Speech Text：CRLF、边界 whitespace、保留段落、超长文本和控制标记；
- fingerprint：同输入稳定、任一有效参数变化产生新 clip ID、标签变化不改变 ID；
- Profile：所有边界值、越界值、unverified 状态和固定音频规格；
- Voice Catalog：24 小时 freshness、stale fallback、精确 ID、无后缀推断；
- hex/audio：奇数 hex、非法字符、空数据、非 MP3、metadata mismatch；
- manifest：路径穿越、错误 asset ID、多个 active、短 hash 冲突、checksum mismatch；
- LRU：锁定/播放/导出 entry 不淘汰，budget 收敛，history 不随 cache 删除。

### 11.2 Machine CLI 集成测试

延续 [`tests/machine_cli.rs`](../../tests/machine_cli.rs) 的 fixture 方式，至少覆盖：

- human 与 machine 参数互斥；
- `asset_id + annotation_id + content_kind` 正确选择高亮和笔记；
- 成功 stdout 只有一个 JSON，stderr 为空；
- 失败 stdout 为空，stderr 是带 schema 的稳定错误；
- receipt 不出现 Annotation 原文、API Key 或音频 hex；
- 缓存命中时 mock provider 调用数为 0；
- rehydration 时 provider 调用数为 0；
- 同 clip 并发请求的 provider 调用数恰好为 1；
- unknown 后普通 generate 的 provider 调用数保持不变；
- `--regenerate` 创建新 attempt，失败时旧 cache checksum 不变；
- play/export/cache clear 永远不创建 provider 请求；
- Markdown export 缺音频时成功并携带 warning；
- export path 不能逃出用户选择的书籍目录。

### 11.3 必须证明敏感性的负向控制

每类新 guard 至少保留一个已知违反输入，证明检查真的会失败：

| Guard | 负向控制 |
| --- | --- |
| content selection | note-only/highlight-only fixture 请求不存在的另一侧 |
| no implicit network | mock server 计数器在 play/export/Markdown export 中必须保持 0 |
| single flight | 两个并发 generate 对同一 clip，故意延迟 provider 响应 |
| unknown gate | provider 接收请求后断开，随后普通 generate 不得产生第二个请求 |
| cache integrity | 成功后修改一个音频字节，play/export 必须拒绝 |
| atomic cache | 在音频完成和目录 rename 之间注入失败，最终 entry 不得存在 |
| export containment | manifest 使用 `../` relative path，导出不得读取或写出根目录 |
| manifest authority | 损坏 JSON 后 speech export 不得覆盖或猜测重建 |
| secret safety | 缺 key、401 和 debug 日志中均不得出现 Authorization 值 |

临时故障注入或测试变异完成后必须恢复原状态，不把 suppression、skip 或弱化断言作为绿色结果。

### 11.4 真实 SenseAudio smoke

真实测试必须双重 opt-in：

```text
RUN_SENSEAUDIO_SMOKE=1
SENSEAUDIO_API_KEY=<secret>
```

使用固定非用户文本，不读取 Apple Books 数据。至少验证：

- `voice_type=all` 的真实请求行为；
- 默认 voice 可用或正确返回 unavailable；
- 中文和英文固定短句；
- 默认、一个情感变体和一个风格变体；
- 一个语速代表值和一个音量代表值；
- response hex 可形成可解析 MP3；
- trace ID、usage、duration 和本地 checksum 进入 receipt；
- 人工试听确认音色/情感/风格及连续参数具有合理可感知效果；
- 原始文本和 API Key 不进入日志或持久 metadata。

正常 CI 不运行真实 smoke，不依赖网络、余额或个人账号。

### 11.5 回归检查

实现完成后至少运行：

```bash
cargo test
cargo check --all-targets
cargo build --release
bash tests/headless_mainline.sh
bash skills/apple-books-export-rust/tests/contract.sh
```

若 TUI 共享的 CLI help、binary 路径或 Machine JSON 代码有变化，再运行 TUI test/typecheck；
不得把 Rust focused test 通过描述为 AppKit、真实 provider 或整条发布链已验证。

## 12. 实施切片

### Slice 1：领域类型与纯函数

- `SpeechContentKind`、Speech Text normalization、Voice Profile validation；
- canonical fingerprint v1 与 clip ID；
- Machine receipt/error/warning 类型；
- 不接网络、不写真实用户目录。

完成证据：单元测试包含每个范围边界和 fingerprint 负向控制。

### Slice 2：本地 store、lock 与状态

- 注入式 Speech root；
- config/catalog/clip/attempt schema；
- 不可变 version + 原子 current pointer、cross-process lock、LRU 和 90 天 history；
- unknown gate、旧 cache 保留和清理 skipped receipt。

完成证据：临时目录集成测试、并发 mock 和故障注入通过。

### Slice 3：SenseAudio adapter

- voice list、同步 TTS、严格 response/error mapping；
- hex decode、MP3 validation、trace/usage；
- API Key env lookup 和日志脱敏；
- 完成控制标记 escape 的真实 provider spike。

完成证据：mock contract test 通过；真实 smoke 仍保持 opt-in。

### Slice 4：Speech CLI 与 Machine JSON

- nested clap commands；
- human/machine 参数分流；
- generate/cache/rehydrate/unknown/regenerate 流程；
- play 的 human `afplay` 与 machine path-only 语义。

完成证据：扩展 `tests/machine_cli.rs`，证明 stdout/stderr、schema、stable codes 和 no implicit network。

### Slice 5：Export Manifest 与 Markdown

- speech export 原子复制和 manifest v1；
- active variant、冲突保护、短 hash collision；
- Markdown/Obsidian 链接和结构化 warning；
- malformed/missing manifest 的降级行为。

完成证据：path traversal、checksum mismatch、用户 Markdown 不被 speech export 修改等测试通过。

### Slice 6：验收与文档

- CLI help 与用户配置说明；
- mock 全量回归；
- opt-in SenseAudio smoke；
- 人工听感矩阵；
- 准确记录已验证、未验证和 provider/runtime 边界。

完成证据：第 11 节检查均有结果，真实 smoke 未运行时必须明确标记而不是推断通过。

## 13. 首版完成条件

首版只有在以下条件全部满足时才完成：

- ADR 0007 的每项边界都有实现或明确测试；
- 一个真实 fixture Annotation 的 highlight 和 note 可以分别生成不同 clip；
- Voice Profile 的 voice、情感/风格映射、speed、volume、pitch 可追溯到 receipt；
- 同请求 cache hit、export rehydration 和 single-flight 均不会产生额外 provider 调用；
- unknown、provider-success/artifact-failure 和取消后不会自动重放；
- cache、history、export manifest 和用户文件生命周期互不越权；
- Machine JSON 没有原文、密钥或音频 hex；
- Markdown/Obsidian 链接只引用有效 active export；
- 现有 Headless Mainline、TUI/Skill 数据合同没有回归；
- 真实 provider smoke 和人工听感是否执行被准确报告。
