# SenseAudio TTS API 调研与 Apple Books 语音设计输入

- 日期：2026-09-10
- 状态：研究完成；作为后续设计输入，不是已接受的 ADR
- 首个供应商：SenseAudio 开放平台
- 首个推荐模型：`sensenova-tts-2.0`
- 后续决策：[`ADR 0007：标注语音生成与供应商边界`](../adr/0007-annotation-speech-generation-boundary.md)
- 实施 Spec：[`Annotation Speech Implementation Spec`](2026-09-11-annotation-speech-implementation-spec.md)
- 本文事实来源：SenseAudio 官方文档；仓库边界来源见[项目上下文](../../CONTEXT.md)与 [ADR 0005](../adr/0005-headless-mainline-appkit-cutover.md)

## 1. 结论先行

SenseAudio 的基础调用路径已经明确：向 `https://api.senseaudio.cn/v1/t2a_v2` 发起带
`Authorization: Bearer <API_KEY>` 的 JSON `POST`，请求体传入模型、文本、音色和音频规格；
非流式接口返回 JSON，其中 `data.audio` 是十六进制（hex）编码的音频字节，需要解码后才能
保存为 `mp3`/`wav`/`pcm`/`flac` 文件。[快速接入指南](https://docs.senseaudio.cn/guides/account/quick-access)
和[语音合成 HTTP API](https://docs.senseaudio.cn/api-reference/endpoint/tts/synthesize)
都给出了这一流程。

对本项目最重要的产品边界是：文档没有定义独立的 `emotion` 或 `style` 请求字段。
`voice_setting` 当前明确的字段是 `voice_id`、`speed`、`vol`、`pitch` 和可选的
`latex_read`。情绪与场景风格主要通过不同的 `voice_id` 变体表达，例如同一音色会有
“平稳、开心、低落、严肃、内容剖析、开场介绍”等变体。[音色列表](https://docs.senseaudio.cn/guides/voice/catalog)
展示了这些变体，[TTS 参数页](https://docs.senseaudio.cn/api-reference/endpoint/tts/synthesize)
没有出现独立情感/风格字段。因此，产品层可以提供“情感/风格”选择，但第一版应把它们
实现为“筛选并解析到具体 `voice_id`”，不能把它们直接序列化成厂商尚未支持的参数。

建议第一阶段使用同步合成：高亮和笔记通常是短文本，一次请求得到完整音频最容易验证、
缓存和失败恢复。SSE 流式合成适合后续的边生成边播放；WebSocket 适合增量文本和多次任务，
不应成为第一版静态标注语音的必要依赖。[SSE 文档](https://docs.senseaudio.cn/api-reference/endpoint/tts/synthesize-stream)
和 [WebSocket 文档](https://docs.senseaudio.cn/api-reference/endpoint/tts/websocket)
均已记录在本文第 5 节。

## 2. 官方 API 合同

### 2.1 鉴权与基础地址

- 基础地址：`https://api.senseaudio.cn`
- 同步与 SSE：`POST /v1/t2a_v2`
- WebSocket：`wss://api.senseaudio.cn/ws/v1/t2a_v2`
- HTTP 请求头：

  ```http
  Authorization: Bearer <API_KEY>
  Content-Type: application/json
  ```

API Key 由 SenseAudio 控制台的 API Key 页面创建。密钥不应写进 Markdown、源码、提交记录、
日志或错误信息。[快速接入指南](https://docs.senseaudio.cn/guides/account/quick-access)只规定了
Bearer 传递方式，没有规定本项目的密钥持久化方案；密钥存储是后续设计决策，见第 9 节。

### 2.2 同步 HTTP 合成

最小可行请求：

```bash
curl --request POST \
  --url https://api.senseaudio.cn/v1/t2a_v2 \
  --header "Authorization: Bearer $SENSEAUDIO_API_KEY" \
  --header "Content-Type: application/json" \
  --data '{
    "model": "sensenova-tts-2.0",
    "text": "你好，这是来自 SenseAudio 的第一条语音。",
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
  }' > response.json

jq -r '.data.audio' response.json | xxd -r -p > output.mp3
```

请求字段：

| 字段 | 官方合同 | 设计含义 |
| --- | --- | --- |
| `model` | 当前可选 `senseaudio-tts-1.5-260319`、`sensenova-tts-2.0`；文档推荐后者 | 必须进入缓存 key；不要把模型写死在业务逻辑中 |
| `text` | 必填，最大 10000 字符；支持 `<break time=...>`，最小停顿 100ms | 发送前由标注内容选择器生成，并保留准确文本快照 |
| `voice_setting.voice_id` | 必填；必须是当前账号可调用的音色 | 情绪/风格最终落到此字段 |
| `voice_setting.speed` | 默认 `1`，范围 `[0.5, 2.0]` | 语速；步进值文档概览为 `0.01` |
| `voice_setting.vol` | 默认 `1`，范围 `[0.01, 10.0]` | 厂商字段名是 `vol`；产品层可以叫“音量” |
| `voice_setting.pitch` | 默认 `0`，范围 `[-12, 12]` | 声调，不等同于情绪字段 |
| `voice_setting.latex_read` | 默认 `false` | 数学公式朗读；使用前按模型验证 |
| `audio_setting.format` | `mp3`、`wav`、`pcm`、`flac`；`mp3` 推荐 | 首版建议只开放 `mp3`，避免播放/缓存组合爆炸 |
| `audio_setting.sample_rate` | `8000`、`16000`、`22050`、`24000`、`32000`、`44100`；`32000` 推荐 | 进入缓存 key和产物元数据 |
| `audio_setting.bitrate` | `32000`、`64000`、`128000`、`256000` | 对有损格式有效；仍应保留在请求快照中 |
| `audio_setting.channel` | `1` 或 `2`；文档示例/默认值为 `2` | 阅读语音可评估单声道，但首个 smoke 应先使用官方示例值 |
| `stream` | 同步必须为 `false` | 与 SSE 共用同一 HTTP endpoint，但协议不同 |
| `dictionary` | `original` + `replacement` 的多音字配置 | 仅在模型支持时开放；不要把普通注释文本当成词典 |

成功响应的关键结构：

```json
{
  "data": {
    "audio": "49443304...",
    "status": 2
  },
  "extra_info": {
    "audio_length": 2306,
    "audio_sample_rate": 32000,
    "audio_size": 36908,
    "bitrate": 128000,
    "audio_format": "mp3",
    "audio_channel": 2,
    "word_count": 24,
    "usage_characters": 30
  },
  "trace_id": "...",
  "base_resp": {
    "status_code": 0,
    "status_msg": "success"
  }
}
```

实现时必须：

1. 先检查 HTTP 状态码，再解析 JSON；
2. 检查 `base_resp.status_code == 0`、`data` 非空、`data.audio` 非空；
3. 对 `data.audio` 做严格 hex 解码，解码失败不能把文本响应误写成音频；
4. 先写临时文件，完成并验证后再原子改名为最终音频文件；
5. 保存 `trace_id`、`extra_info` 和规范化请求快照，便于排查和核算；
6. 失败响应可能是 HTTP `401` 的 `{ "code": "...", "message": "..." }` 形态，不能只依赖成功响应中的 `base_resp`。

`usage_characters` 是接口返回的 Unicode 码点统计；模型列表另说明 TTS 计费规则为“按万字符”，
其中一个汉字计 2 个字符，英文字母、标点、空格等计 1 个字符。因此，不能直接把
`usage_characters` 当成最终账单字符数；应原样保存供应商用量，并把费用计算留给供应商账单或
单独验证过的计费适配器。[模型列表](https://docs.senseaudio.cn/guides/account/model-list)

### 2.3 停顿、公式与多音字

- 停顿：文本中可使用 `<break time=500>`；`time` 单位为毫秒，最小值为 100。
- 公式：`latex_read=true` 时启用公式朗读；公式需要按文档要求用 `$$...$$` 包裹，反斜杠
  需要在 JSON 中转义。
- 多音字：`dictionary` 形如 `[{"original":"好干净","replacement":"[hao4]干净"}]`。
  WebSocket 文档明确说该配置要求使用 `senseaudio-tts-1.5-260319`；同步接口的字段表没有
  在同一位置重复这个限制，因此实现应按模型能力做校验，不能盲发。

### 2.4 官方机器可读描述

除 HTML 文档外，SenseAudio 当前还公开了同步和 SSE 的 OpenAPI 3.1 描述：

- [同步 TTS OpenAPI](https://docs.senseaudio.cn/api-reference/endpoint/tts/synthesize.openapi.json)
- [SSE TTS OpenAPI](https://docs.senseaudio.cn/api-reference/endpoint/tts/synthesize-stream.openapi.json)
- [WebSocket TTS AsyncAPI](https://docs.senseaudio.cn/api-reference/endpoint/tts/websocket.asyncapi.json)

这三份描述可作为未来 Rust serde 类型、契约测试和文档漂移检查的输入，但不应在没有
锁定版本/快照策略前直接生成并提交供应商代码。当前 OpenAPI 还确认了两个细节：

- `voice_setting.voice_id` 是唯一必填的嵌套音色字段；
- SSE 的 `stream_options.exclude_aggregated_audio` 说明为“是否在最后一个 chunk 中排除聚合音频”，
  默认 `false`。因此客户端按分片追加音频时建议显式传 `true`，避免把最后的聚合音频再写一遍。

## 3. 音色、情感与风格

### 3.1 音色查询接口

接口：`POST https://api.senseaudio.cn/v1/get_voice`，使用同样的 Bearer 鉴权。
文档示例发送 `{}`，但请求体 schema 又把 `voice_type` 标为必填，允许值为：
`system`、`voice_clone`、`voice_generation`、`all`。[查询可用音色 API](https://docs.senseaudio.cn/api-reference/endpoint/voice/list)

这是一个文档不一致点。建议本项目第一版显式发送：

```json
{ "voice_type": "all" }
```

并在真实账号 smoke 中确认；若服务端实际接受空对象，也可以在兼容层保留 fallback，但不应
把“示例为空对象”和“schema 标为必填”任意选一个当作已验证事实。

响应分为三组：

```json
{
  "system_voice": [
    {
      "voice_id": "female_0033_b",
      "voice_name": "嗲嗲台妹",
      "description": ["开心"],
      "created_time": "..."
    }
  ],
  "voice_cloning": [],
  "voice_generation": [],
  "base_resp": { "status_code": 0, "status_msg": "success" }
}
```

账号当前可调用的查询结果应是选择器的事实来源。静态[音色目录](https://docs.senseaudio.cn/guides/voice/catalog)
可用于展示场景、情绪和风格标签，但不能代替账号权限检查；官方目录也区分普通、VIP、SVIP
和自定义音色权限。

### 3.2 产品层如何表达情感/风格

SenseAudio 的目录把情绪直接写在变体旁边，例如：

- `female_0038_a`：平稳通用；`female_0038_b`：温柔讲解；`female_0038_d`：严肃告知；
- `male_0021_b`：低落；`male_0021_c`：开心；`male_0021_d`：生气；`male_0021_e`：深情；
- `female_0033_a` 到 `_f`：平稳、开心、撒娇、低落、委屈、生气。

这些是目录中的具体音色 ID，不代表厂商承诺了一个可由后缀解析的通用命名规则。
因此推荐以下内部语义：

```text
产品选择：音色 = “嗲嗲台妹”，情感 = “开心”，风格 = “阅读/讲解”
        ↓ 由音色目录/账号音色缓存解析
厂商请求：voice_setting.voice_id = "female_0033_b"
        + speed / vol / pitch 等连续参数
```

内部不要把 `emotion`、`style` 直接塞进 SenseAudio JSON。建议维护一个可更新的音色描述：

```json
{
  "provider": "senseaudio",
  "voice_id": "female_0033_b",
  "voice_name": "嗲嗲台妹",
  "emotion": "开心",
  "style": null,
  "scenario": ["阅读辅助", "有声读物"],
  "source": "senseaudio-catalog",
  "available": true
}
```

这里的 `emotion`/`style` 是本项目的展示和筛选元数据，不是 SenseAudio 的请求字段。
如果用户选择的音色没有对应的情感或风格，应显示不可用并要求选择其他变体，不要静默换成
相近音色。后续接入第二家供应商时，再由各 provider adapter 把产品层语义映射到各自的
能力；不能假设所有供应商都有同样的情感模型。

### 3.3 推荐的 UI 参数分层

第一版可分为两类控件：

| 层 | 控件 | 实际落点 |
| --- | --- | --- |
| 离散选择 | 音色、情感、风格、场景 | 解析为一个具体 `voice_id`；组合必须受可用音色目录约束 |
| 连续调节 | 语速、音量、声调 | `speed`、`vol`、`pitch`；前端先按官方范围校验 |

“音色”和“情感/风格”不能完全独立：厂商把部分风格直接做成了音色变体。UI 可以先选
基础音色，再筛选其变体；也可以直接把“音色 + 状态”作为一个可试听的选项，减少无效组合。

## 4. Apple Books 内容建模

当前 Rust 数据层的 [`Annotation`](../../src/models.rs) 同时保存 `selected_text` 和 `note`；
[`db.rs`](../../src/db.rs) 已按“有高亮或有笔记才算有效内容”过滤空壳标注，不应使用
Apple Books 的 `annotation_type` 判断内容类别。

建议为 TTS 明确一个文本选择模式，而不是在 provider 层猜测：

```text
Highlight        -> selected_text
Note             -> note
HighlightAndNote -> selected_text + 分隔停顿 + note
```

推荐的第一版行为：

- 高亮和笔记都存在时，默认只朗读高亮；用户显式选择后再朗读“高亮 + 笔记”；
- 只有笔记时允许直接朗读笔记；
- 两者都为空时不创建 TTS 请求；
- “高亮 + 笔记”的组合文本要在缓存 key 中保存完整快照，并由产品决定是否朗读“高亮/笔记”标签，
  或只用 `<break>` 分隔；
- 生成请求必须是用户主动触发的远程操作，因为高亮和笔记可能包含个人阅读记录。

TTS 不应悄悄进入当前 Agent Data Skill 的只读本地数据路径。现有[本地数据边界](../../CONTEXT.md)
要求远程 AI/网络操作单独明确；未来若让 Agent 使用 TTS，应设计为显式、可审计的网络能力，
而不是 `list`/`annotations`/`export` 的隐式副作用。

## 5. 同步、SSE 与 WebSocket 的选择

### 5.1 同步 HTTP：第一阶段推荐

调用一次、返回完整音频，适合“对一条高亮/笔记生成一个可缓存文件”。好处是：

- 实现只需要 HTTP JSON + hex 解码；
- 文件只有在完整响应成功后才提交；
- 最容易进行缓存去重和离线播放；
- 不需要管理长连接或播放端的增量缓冲。

限制是首个音频字节要等合成完成后才能播放。对短标注，这是可以接受的产品取舍。

### 5.2 SSE：第二阶段边生成边播放

SSE 仍然使用 `POST /v1/t2a_v2`，但必须传 `"stream": true`；响应类型是
`text/event-stream; charset=utf-8`。每个事件以 `data: ` 开头，后面是 JSON；
`data.audio` 是当前 chunk 的 hex 数据，`data.status=1` 表示合成中，`2` 表示结束，
`extra_info` 只在最后一个 chunk 返回。[SSE API](https://docs.senseaudio.cn/api-reference/endpoint/tts/synthesize-stream)

文档示例还提供：

```json
{ "stream_options": { "exclude_aggregated_audio": true } }
```

官方 SSE OpenAPI 对该字段的说明是“是否在最后一个 chunk 中排除聚合音频”，默认值为
`false`。客户端已经按 chunk 追加字节时建议使用 `true`；上线前仍需用真实响应验证最终
chunk 的边界，以及 `mp3` 分片拼接是否可被播放器接受。

SSE 实现不能按网络 `read()` 边界直接解析 JSON。要维护跨 chunk 的行缓冲，按 SSE 空行/换行
边界拆出完整 `data:` 事件，逐个解码并按顺序写入临时文件。网络中断、解析失败或服务端状态
非 0 时删除临时文件，不把部分音频放入缓存。

### 5.3 WebSocket：暂不作为第一版依赖

WebSocket 地址为 `wss://api.senseaudio.cn/ws/v1/t2a_v2`。文档定义的事件顺序是：

```text
connected_success
  -> task_start
  -> task_started
  -> task_continue (可多次)
  -> task_finish
  -> task_finished / task_failed
```

`task_continue` 支持分段文本，音频仍以 hex 字符串返回；连接在最后一次服务端事件后
120 秒无新事件时会自动断开。[WebSocket API](https://docs.senseaudio.cn/api-reference/endpoint/tts/websocket)

这更适合实时对话、增量输入或一条连接上连续合成多个任务。Apple Books 的静态标注生成
暂时没有足够理由承担连接状态、事件顺序、心跳和关闭处理的复杂度。

## 6. 推荐的仓库接入边界

当前 [`src/provider.rs`](../../src/provider.rs) 是 OpenAI-compatible LLM client，
[`src/cache.rs`](../../src/cache.rs) 是 LLM 结果缓存；不要把 SenseAudio 的二进制音频、SSE
解析和 TTS 计费语义塞进这两个已有模块。

建议未来新增一个独立的 TTS seam：

```text
src/tts.rs                 # provider-neutral request/result/capability
src/tts/senseaudio.rs      # SenseAudio HTTP、voice list、hex 解码
src/tts_cache.rs           # 音频文件 + 元数据；与 LLMCache 分开
```

第一版可保持同步 HTTP 实现；SSE 后续作为同一个 provider 的另一种执行方式，不应让 UI
直接知道 SenseAudio 的 JSON 字段。

推荐的领域请求至少表达：

```text
SynthesisRequest {
  provider,
  model,
  source: { asset_id, annotation_id, content_mode },
  text_snapshot,
  resolved_voice_id,
  emotion_label?,
  style_label?,
  speed,
  volume,              // 映射为 SenseAudio 的 vol
  pitch,
  latex_read,
  dictionary?,
  audio: { format, sample_rate, bitrate, channel }
}
```

`emotion_label` 和 `style_label` 是产品层选择的可追溯标签；`resolved_voice_id` 才是
SenseAudio 请求的事实。这样可以保留用户当时选择的语义，也能在供应商目录更新后发现
映射失效，而不是只剩一个无法解释的音色 ID。

`SynthesisResult` 建议至少包含：

```text
AudioArtifact {
  local_path,
  format,
  byte_size,
  duration_ms?,
  sample_rate?,
  bitrate?,
  channel?,
  provider,
  model,
  voice_id,
  request_fingerprint,
  usage_characters?,
  trace_id?,
  generated_at
}
```

未来 AppKit 应通过既有的 Rust/CLI 机器边界使用这类能力，而不是维护第二套 Apple Books
查询或直接复制 SenseAudio payload。是否在 AppKit 首期开放远程 TTS，需要单独更新能力边界；
现有 [ADR 0006](../adr/0006-appkit-initial-capability-boundary.md) 明确 AppKit 首期不复制
`enrich` 等远程 AI 能力。

## 7. 音频缓存与去重

不能复用当前 `LLMCache`：它的 key 只由书籍和高亮文本构成，值是解释/标签/问题，无法表达
音色、情感映射、语速、格式或模型变化。

建议 TTS 缓存的规范化 fingerprint 至少覆盖：

```text
provider
model
asset_id
annotation_id
content_mode
text_snapshot
resolved_voice_id
speed
vol
pitch
latex_read
dictionary
format
sample_rate
bitrate
channel
```

其中 `annotation_id` 不是唯一充分条件，必须同时保留 `text_snapshot`：笔记被修改、组合模式
变化或 provider 映射变化时应产生新的 fingerprint。缓存项应保存音频路径和生成元数据，
音频本体放在专门的 TTS 输出/缓存目录，不把大段 hex JSON 持久化进配置文件。

缓存提交规则：

1. 请求前查完整 fingerprint；命中且文件存在、非空、格式正确时直接返回；
2. 生成时写 `<fingerprint>.tmp`；
3. 只有 API 成功、hex 解码成功、文件长度大于 0 且元数据可接受时，原子改名并写 metadata；
4. 任意网络、解析、取消或服务端错误都清理 `.tmp`，不污染缓存；
5. 同一 fingerprint 的并发请求应合并或加本地锁，避免重复计费；
6. 改变任何音色/情绪映射、参数、模型或音频规格都必须生成新的 key。

## 8. 错误、重试与隐私边界

TTS 是按字符计费的外部副作用，不能直接照搬当前 LLM provider 的自动重试策略。
文档没有给出幂等键或任务查询接口，尤其要区分：

| 情况 | 推荐处理 |
| --- | --- |
| 本地缺 API Key、空文本、参数越界 | 发请求前失败；不重试、不计缓存失败 |
| HTTP 4xx、`base_resp.status_code != 0` | 记录可操作错误和 `trace_id`；不自动重试；401 要提示密钥 |
| 建连失败且可以合理判断请求未发送 | 可由调用方显式重试；默认次数要低 |
| 超时/断线但无法判断服务端是否已处理 | 标记“结果未知”，不要自动重放同一个付费请求；提供用户明确的再次生成动作 |
| 收到部分 SSE/WebSocket 音频后断开 | 删除临时文件，不能把部分文件当作成功结果 |
| 音色被撤销或不在当前套餐 | 刷新可用音色，标记选择失效；不要静默替换音色 |
| 用户取消 | 关闭请求、清理临时文件，并提示供应商可能已经产生计费，除非真实账单证明不会计费 |

日志只记录 provider、model、voice_id、请求 fingerprint、长度统计、HTTP 状态和 trace ID；
默认不记录高亮/笔记原文、API Key 或完整请求体。生成前 UI 应明确这是一次会把选定文本
发送到 SenseAudio 的网络操作。

## 9. 当前开放决策

本文不替代后续 ADR。下面的事项会改变实现边界，后续设计时需要明确决策者和验证方式：

| 决策 | 当前建议 | 需要确认的证据/选择 |
| --- | --- | --- |
| 密钥存储 | CLI 首版优先环境变量；GUI 以后优先 macOS Keychain，避免新增明文配置 | 是否需要复用当前 `Config.api_key` 兼容行为 |
| 首版传输 | 同步 HTTP，生成完整文件后播放 | 一条真实高亮的延迟是否可接受 |
| 高亮+笔记 | 默认高亮；用户显式选择合并朗读 | 是否朗读“高亮/笔记”标签、使用何种停顿 |
| 情感/风格 | 作为音色目录上的筛选和映射，最终只发送 `voice_id` | 是否允许用户自定义映射；是否需要场景 preset |
| 默认音频 | 先用官方示例：mp3、32000Hz、128kbps、双声道 | 阅读场景是否改为单声道/更低码率 |
| 缓存位置 | 独立 TTS cache/artifact 目录，metadata 与音频分离 | 是否随 Markdown 导出，还是仅供本机播放 |
| 导出形式 | 后续可在 Markdown 中链接本地音频 artifact | Obsidian embed、相对链接或独立播放列表 |
| 远程能力边界 | 用户主动触发；Agent Data Skill 默认不调用 | 是否新建显式 `tts` skill/capability 和确认开关 |
| 流式支持 | 第二阶段 SSE；暂不需要 WebSocket | 是否需要边生成边播放、是否接受流式播放器复杂度 |
| 重试策略 | 不做盲目自动重试；对未知结果要求显式重试 | 是否能从供应商获得幂等或消费记录接口 |

## 10. 建议的实施顺序与验收

### Phase 0：真实 API spike

使用环境变量提供 API Key，不提交密钥：

1. 调用 `/v1/get_voice`，测试 `voice_type=all`，确认账号返回的音色和套餐权限；
2. 用 `male_0004_a` 或当前账号实际可用音色调用同步 TTS；
3. 将 `data.audio` hex 解码为 `output.mp3`，用 macOS 播放器验证可播放；
4. 对 `speed`、`vol`、`pitch` 各做一个边界值和一个中间值请求；
5. 记录响应中的 `trace_id`、`extra_info` 和真实延迟；不把文本或密钥写入日志；
6. 验证模型 1.5 的 `dictionary`/公式能力，确认是否值得进入第一版。

### Phase 1：单条标注的同步生成

- 新增独立 TTS provider seam，不修改 LLM provider 语义；
- 先支持一条高亮或一条笔记、`mp3` 和官方默认音频规格；
- 先落地 TTS 专用缓存和原子文件提交；
- 提供一个明确的 CLI/GUI 动作，显示发送文本和目标 provider；
- 补齐缺 API Key、401、空文本、越界参数、hex 解码失败和部分文件清理测试。

### Phase 2：产品参数与批量

- 接入动态音色查询和可试听的音色变体；
- 支持高亮/笔记/合并三种内容模式；
- 增加情感/风格到 `voice_id` 的可解释映射；
- 批量生成时限制并发，按 fingerprint 去重并显示估算/实际用量；
- 确定本地音频与 Markdown/Obsidian 的关联方式。

### Phase 3：流式体验

- 只有在同步播放体验不足时引入 SSE；
- 实现真正的 SSE 行缓冲、chunk 顺序、`status=2` 收尾、聚合音频防重复和断线清理；
- 只有需要增量文本或多任务长连接时再评估 WebSocket；
- 用真实音频播放器和网络中断测试验证“边播边写”和最终缓存文件都正确。

## 11. 仍需现场验证的事实

本次只阅读了官方文档和本地仓库，没有 API Key，因此没有向 SenseAudio 发起真实请求。
以下内容不能当作已经通过运行态验证：

- 当前账号可用的具体 `voice_id` 和套餐等级；
- `/v1/get_voice` 对 `voice_type` 缺省值的实际行为；
- `sensenova-tts-2.0` 各音色变体的实际音质，以及 `latex_read`/dictionary 的模型兼容性；
- 同步响应的真实 HTTP 超时、限流和错误码集合；
- 真实 SSE 响应是否严格按 `exclude_aggregated_audio` 省略最终聚合 chunk，以及分片拼接后的文件行为；
- 供应商的实际计费字符数与 `extra_info.usage_characters` 的对应关系；
- 音频中断、取消、重复请求的账单行为。

这些应由 Phase 0 的受控 smoke 产生证据，再决定是否写入后续 ADR 或实现合同。

## 12. 官方资料索引

- [快速接入指南](https://docs.senseaudio.cn/guides/account/quick-access)
- [语音合成介绍](https://docs.senseaudio.cn/guides/tts/overview)
- [语音合成 HTTP（同步）](https://docs.senseaudio.cn/api-reference/endpoint/tts/synthesize)
- [同步 TTS OpenAPI](https://docs.senseaudio.cn/api-reference/endpoint/tts/synthesize.openapi.json)
- [语音合成 HTTP 流式（SSE）](https://docs.senseaudio.cn/api-reference/endpoint/tts/synthesize-stream)
- [SSE TTS OpenAPI](https://docs.senseaudio.cn/api-reference/endpoint/tts/synthesize-stream.openapi.json)
- [语音合成 WebSocket](https://docs.senseaudio.cn/api-reference/endpoint/tts/websocket)
- [WebSocket TTS AsyncAPI](https://docs.senseaudio.cn/api-reference/endpoint/tts/websocket.asyncapi.json)
- [查询可用音色 API](https://docs.senseaudio.cn/api-reference/endpoint/voice/list)
- [音色目录](https://docs.senseaudio.cn/guides/voice/catalog)
- [模型列表与计费概览](https://docs.senseaudio.cn/guides/account/model-list)
