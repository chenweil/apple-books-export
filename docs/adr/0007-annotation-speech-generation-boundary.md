# ADR 0007: 标注语音生成与供应商边界

- 状态：已接受
- 日期：2026-09-10
- 关联：[`ADR 0005`](0005-headless-mainline-appkit-cutover.md)、[`ADR 0006`](0006-appkit-initial-capability-boundary.md)、[`SenseAudio TTS API 调研`](../plans/2026-09-10-senseaudio-tts-api-research.md)
- 实施 Spec：[`Annotation Speech Implementation Spec`](../plans/2026-09-11-annotation-speech-implementation-spec.md)

## 背景

Apple Books 的一条 Annotation 可以同时包含高亮原文和个人笔记。SenseAudio 可以把文本
合成为音频，但它把情感和风格主要编码在具体 `voice_id` 变体中，并没有为当前接口提供
独立的 `emotion` 或 `style` 请求字段。TTS 还会把本地阅读内容发送给外部供应商并产生费用，
不能成为现有只读数据路径或 Markdown 导出的隐式副作用。

首个供应商是 SenseAudio，但它不是永久唯一供应商。若由 CLI、AppKit 或其他入口分别维护
文本选择、供应商参数、缓存和音频文件规则，会重新产生多套事实源。

## 决策

### 权威边界与首版入口

- TTS 语义由 Rust 核心统一拥有，包括 Annotation 内容选择、Voice Profile 解析、供应商调用、
  Cached Speech Clip 和 Exported Speech Clip 的生命周期；
- 首版用户入口是 Rust CLI，并提供 Machine JSON Protocol 边界；未来 AppKit 消费同一 Rust
  合同，不直接复制 SenseAudio 请求或 Apple Books 数据规则；
- Read-only TUI 和 Agent Data Skill 首版不增加 TTS，继续保持当前只读和本地数据边界；
- 首版只实现同步 HTTP 合成。SSE 只有在真实延迟证明同步体验不足时再加入，WebSocket 暂不进入范围；
- 生成和播放是两个独立动作。Machine JSON 生成只返回生成结果，不得隐式播放；人类入口可以
  显式播放已有 Cached Speech Clip。

### Speech Clip 与内容身份

- 一个 Speech Clip 只朗读 Annotation 的一个内容部分：高亮或个人笔记；
- 同一 Annotation 的高亮和笔记分别生成、缓存和导出。连续播放是播放层编排，不创建第三份
  合并音频；
- 首版一次操作只生成一个 Speech Clip，批量、章节和整本生成以后通过编排增加；
- 机器入口使用 `asset_id + annotation_id + content_kind` 选择内容。显示序号只可作为人类 CLI
  的便利入口，不能成为持久身份；
- Machine `speech generate` 必须显式提供 `asset_id`、`annotation_id` 和
  `content_kind=highlight|note`。人类 CLI 可以使用书籍和 Annotation 的显示序号，但解析后仍
  进入同一个稳定身份合同；
- Speech Text 只做首尾空白和换行规范化，不自动改写、总结或删除原文；
- 单个 Speech Text 超过 provider 的 10000 字符上限时，本地返回 `SPEECH_TEXT_TOO_LONG`；
  首版不截断、不总结，也不自动拆成多个 Speech Clip；
- 规范化将 CRLF 转为 LF、去除边界空白并保留内部换行和段落。Annotation 内容始终作为普通
  文本处理；`<break>` 等 provider 控制语法必须转义，只有产品明确创建的控制标记可以生效；
- 首版不自动检测、翻译或改写语言。上限内的 Unicode Speech Text 可以进入 provider adapter；
  供应商不支持的语言以稳定 provider 错误返回；
- 缓存还必须包含准确的文本快照，使 Annotation 内容变化能够产生新的生成身份。

### Voice Profile 与供应商能力

- 用户先选择音色，再从该音色实际支持的情感和风格中选择；最终必须解析为供应商能够生成的
  具体音色 ID；
- 不存在的音色、情感或风格组合必须显示为不可用，不能静默替换为近似音色；
- 首版保留 `emotion` 和 `style` 两个产品维度，但它们的具体值由 Speech Provider 定义，
  暂不建立未经多供应商证据支持的公共枚举；
- Machine 调用只提交已解析的具体 `voice_id`，不按音色名称、情感或风格标签做模糊匹配；
  GUI 等人类入口负责把受约束的选择解析成该 ID。`speed`、`volume` 和 `pitch` 可以作为明确的
  Profile 覆盖项传入；
- Voice Profile 提供一个全局默认值，每次生成可以临时覆盖；首版不引入每本书或每条
  Annotation 的持久 Profile；
- 初始默认 Profile 为 `sensenova-tts-2.0`、`male_0004_a`、`speed=1`、`vol=1`、
  `pitch=0`、MP3/32000Hz。使用前必须通过当前 Voice Catalog 验证；不可用时要求用户选择，
  不能自动换成目录中的第一个音色；
- Voice Catalog 在本地缓存 24 小时，允许手动刷新；供应商拒绝当前音色时先刷新目录，再要求
  用户选择可用音色；
- `speech profile set` 先做本地范围和结构校验。无法取得当前 Voice Catalog 时允许保存
  Unverified Voice Profile，但真正创建 Speech Attempt 前必须完成音色可用性验证；
- 建立薄的 TTS provider seam。产品层统一 Speech Clip、Voice Profile 和生成结果，
  SenseAudio adapter 独立拥有其请求、响应、音色目录和参数映射。

### 远程授权、缓存与导出

- 只有用户明确执行“生成语音”时才把选定内容发送给 Speech Provider；浏览、读取、播放缓存、
  Markdown 导出和 Agent Data Skill 都不得隐式触发远程生成；
- 未来批量生成必须单独展示范围、片段数量和字符估算，并由用户显式启动；
- 生成结果先成为应用管理的 Cached Speech Clip，用于试听和重复播放。缓存采用总容量预算和
  最近最少使用淘汰，并提供手动清理；默认预算为 1 GiB，且允许配置；
- 用户执行导出后，音频成为 Exported Speech Clip，不再受缓存淘汰影响；
- 与书籍 Markdown 一起导出的音频复制到该书导出目录的 `assets/audio/`，Markdown 使用
  相对链接，不依赖当前机器的中央缓存路径；
- 导出文件名包含 `content_kind` 和短 fingerprint。相同 fingerprint 的有效产物直接复用；
  不同内容或 Voice Profile 不覆盖已有文件；
- 同一个 Annotation content kind 可以保留多个 Exported Speech Clip，但只有一个 Active
  Exported Speech Clip。新导出的变体成为 active；旧文件保持用户所有，不再由 Markdown
  自动链接，也不被应用静默删除；
- 书籍导出目录在 `assets/audio/manifest.json` 保存 Speech Export Manifest，记录完整 clip ID、
  Annotation 身份、content kind、active 状态、相对文件路径和校验信息，不保存原文或密钥；
- Exported Speech Clip 文件名为 `highlight-<short-fingerprint>.mp3` 或
  `note-<short-fingerprint>.mp3`。短 fingerprint 默认使用完整 clip ID 的前 12 位；若 manifest
  检测到前缀冲突则增加长度，不能覆盖另一 clip；
- 普通 Markdown/Obsidian 导出只链接已经存在的 Exported Speech Clip。它不会复制仅由应用
  管理的缓存，也不会为了补齐链接而调用 Speech Provider；没有导出语音时不写占位链接；
- CLI 首版通过环境变量取得 API Key；未来 AppKit 使用 macOS Keychain。配置文件只保存
  密钥引用，不保存明文密钥；
- 人类 CLI 在单条生成前显示内容类型、音色和估算字符数。明确执行生成命令本身就是授权，
  不增加第二次确认；这一规则不延伸到未来批量生成；
- 用量同时保留本地 Unicode 字符数、按当前 provider 规则得到的计费字符估算，以及供应商响应
  返回的实际用量字段。程序不硬编码货币价格，最终金额以供应商账单为准。

### 身份与本地状态

- `clip_id` 是完整规范化 Speech Clip 请求的 SHA-256，标识逻辑内容与 Voice Profile；
- 每次真实供应商调用拥有独立 `attempt_id`。`--regenerate` 为同一个 `clip_id` 创建新的
  Speech Attempt，而不是制造新的逻辑 clip；
- Speech 配置、Voice Catalog、metadata 和 Cached Speech Clip 存放在用户级
  `~/Library/Application Support/books-exporter/speech/`，不跟随当前工作目录、`--config`
  或 Markdown 导出目录；
- 该目录只保存非秘密配置和应用管理状态；API Key 继续来自环境变量或未来的 Keychain。

### 请求失败与重试

- 发送请求前发现缺少密钥、空文本、无效音色或参数越界时直接失败，不接触供应商；
- 请求一旦可能到达供应商，超时、断线或未知结果不得自动重放，因为重复请求可能重复计费；
- 错误结果保留供应商 trace ID 和不含原文/密钥的诊断信息，由用户显式选择是否再次生成；
- 只有能够证明请求尚未发送的本地建连失败，未来才可以考虑受限的自动重试。

### 并发、取消与接受证据

- 生成按 `clip_id` 使用跨进程锁。同一个 clip 已在生成时，后来的调用等待首个 Speech Attempt；
  首个成功后返回 cache hit，失败或 Unknown Speech Result 则返回相同终态，不发起第二个请求；
- 等待跨进程锁必须有上限。超时返回稳定的 `SPEECH_IN_PROGRESS`，并在可用时携带当前
  `attempt_id`；
- 请求发送前取消是普通取消，不产生 Unknown Speech Result。请求可能到达 provider 后发生
  取消、超时或断线时，记录 Unknown Speech Result；普通 `generate` 不会越过该状态，只有
  显式 `--regenerate` 才创建新的 Speech Attempt；
- 一个 Speech Cache Entry 由 `clip_id` 目录中的原子 current state 与一个不可变的已验证音频
  version 组成。先在同一文件系统的临时目录写完音频和 metadata，再将 version 目录原子放置，
  最后原子切换 current state；
- 接受一个 Speech Cache Entry 至少要求：HTTP/API 成功、hex 解码成功、文件非空、音频格式
  可解析，并且供应商 metadata 与本地文件不矛盾；任一检查失败都不得提交最终目录；
- `play` 和 `export` 遇到损坏缓存时返回 `SPEECH_CACHE_CORRUPT`，不调用 provider。只有用户
  显式执行 `generate` 时，才可以隔离损坏 entry 并重新生成；
- `--regenerate` 开始时不删除当前有效 Speech Cache Entry。新 attempt 只有成功并通过完整验证
  后才把 current state 原子切换到新 version；失败或 unknown 不影响旧音频的播放和导出；
- 没有现有 cache entry 的 Unknown Speech Result 作为 clip 级阻塞状态持续保留，不随 90 天
  attempt history 到期。只有显式 `--regenerate` 或用户明确清除该状态后才允许新 attempt；
- Speech Attempt History 仅保留不含原文、密钥和音频的 metadata，默认保留 90 天，不随音频
  LRU 淘汰。`speech cache clear` 和 `speech history clear` 是两个不同动作。

### CLI 与 Machine JSON 合同

- 产品级命令族使用 `speech`，不把 CLI 命名绑定到 TTS 技术或 SenseAudio。其下承载
  `voices`、`profile`、`generate`、`play`、`export` 和 `cache` 等职责，具体参数在实施 spec
  中确定；
- Machine JSON 延续现有约定：成功 JSON 只写 stdout，失败 JSON 只写 stderr，进程以非零
  状态表示失败；
- Speech 成功响应使用独立的 `schema_version: 1` 和结构化 Speech Receipt。这是新增响应类型，
  不升级或破坏现有 list、annotations、export、doctor 响应；
- Speech Receipt 至少标识 clip、asset、annotation、content kind、文本摘要、估算字符数、
  provider、model、解析后的 voice、缓存命中情况、本地音频元数据和 provider trace ID；
- Speech Receipt 不重复 Annotation 原文，不嵌入音频 hex，也不暴露供应商原始成功响应；
- 机器错误保留现有 `code`、`message`、`remediation` envelope，并增加可选 `details`，用于记录
  speech provider、供应商错误码、trace ID 和 `failed`/`unknown` 结果语义；
- 普通 `generate` 命中有效缓存时直接返回该 Cached Speech Clip。只有显式
  `--regenerate` 才再次发起付费请求并原子替换缓存版本；任何 Exported Speech Clip 保持不变；
- 导出目标与当前 clip 内容完全相同时直接复用；同一路径内容不同时默认返回受保护的文件冲突，
  只有显式 `--overwrite` 才允许替换。

### 用户命令与导出呈现

- `speech profile show|set|reset` 管理全局 Voice Profile，并支持 Machine JSON 输出；这些命令
  只读写非秘密配置，不保存 API Key；
- `speech voices` 默认使用 24 小时 Voice Catalog 缓存，过期时刷新；`--refresh` 强制刷新。
  刷新失败时可以展示带获取时间的旧目录，但旧目录不是当前账号权限的保证；
- 人类 `speech play` 使用 macOS 系统播放器播放已有音频。Machine JSON 播放入口只返回状态
  和路径，不产生声音；
- 普通 Markdown 在对应高亮或笔记后写相对播放链接；Obsidian 格式在同一位置写相对音频嵌入。
  同一 Annotation 有两个 Exported Speech Clip 时，每个链接只跟随自己的内容部分；
- `speech export` 只把 Cached Speech Clip 原子复制到书籍导出目录的 `assets/audio/`，不修改
  已有 Markdown。下一次常规 Markdown/Obsidian 导出负责发现 Exported Speech Clip 并写链接；
- 普通 Markdown/Obsidian 导出发现 active 音频缺失或校验不匹配时继续完成主要文档导出，省略
  对应链接，并在机器 receipt 中返回结构化 warning；它不修复文件、不修改 manifest，也不调用
  provider；
- `speech export` 先将音频复制到同一导出文件系统的临时路径并验证，再原子放置最终音频，最后
  原子替换 Speech Export Manifest。中断可以留下未被 manifest 引用的孤立音频，但不能提交
  指向缺失文件的 active 记录；
- `speech export` 遇到损坏 manifest 时返回 `SPEECH_EXPORT_MANIFEST_INVALID`，不覆盖、不猜测，
  也不根据文件名或修改时间重建身份；
- 首版只接受 MP3、32000Hz、128kbps、双声道作为用户可见音频规格；供应商可能支持的其他
  编码组合暂不进入产品合同；
- Speech 领域错误使用稳定代码；供应商原始 code 和 trace ID 只放在可选 `details`。首版代码集
  至少包含 `INVALID_ANNOTATION_ID`、`SPEECH_CONTENT_UNAVAILABLE`、`SPEECH_PROFILE_INVALID`、
  `SPEECH_VOICE_UNAVAILABLE`、`SPEECH_AUTH_FAILED`、`SPEECH_RATE_LIMITED`、
  `SPEECH_PROVIDER_FAILED`、`SPEECH_RESULT_UNKNOWN`、`SPEECH_IN_PROGRESS`、
  `SPEECH_CACHE_CORRUPT`、`SPEECH_STORAGE_UNAVAILABLE`、`SPEECH_OUTPUT_FILE_EXISTS`、
  `SPEECH_EXPORT_MANIFEST_INVALID` 和 `SPEECH_CLIP_NOT_FOUND`。

### 播放与本地回退

- `speech play --clip-id` 优先播放 checksum 验证通过的 Speech Cache Entry；缓存不存在时，可以
  使用 checksum 与完整 clip ID 匹配的 Active Exported Speech Clip；两者都不存在时返回
  `SPEECH_CLIP_NOT_FOUND`，不调用 provider；
- 用户拥有并可以自行修改 Exported Speech Clip，但校验不匹配的文件不能继续作为该 clip ID 的
  已验证播放回退；用户仍可通过系统播放器按普通文件打开；
- 显式 `generate` 发现缓存已淘汰但存在 checksum 匹配的 Exported Speech Clip 时，执行 Speech
  Cache Rehydration，并返回 `provider_called=false`、`cache_source=export`。该动作不创建
  Speech Attempt；
- rehydration 只复制并重新验证用户导出文件，不把导出目录直接作为可淘汰缓存使用。

### 存储预算与淘汰

- 调用 provider 前检查缓存根目录可写、可用空间和安全余量；空间明显不足时本地失败，不能先
  付费生成再发现无法落盘；
- 新 Speech Cache Entry 接受后执行 LRU，使总量回到配置预算内；当前 entry 以及正在生成、
  播放、导出或持有锁的 entry 不参与淘汰；
- 手动 `speech cache clear` 同样跳过正在生成、播放、导出或持有锁的 entry，并在 Speech
  Receipt 中报告 `skipped`，不终止正在进行的操作；
- 若没有足够的可淘汰空间，返回 `SPEECH_STORAGE_UNAVAILABLE`，保留所有已接受且正在使用的 entry。

### 验证边界

- 常规自动化测试使用本地 provider mock，覆盖请求映射、错误、原子提交、缓存、Machine JSON
  和无隐式网络行为；
- 真实 SenseAudio 验收是显式 opt-in smoke，只使用固定的非用户文本和环境变量密钥，不在普通
  CI 中运行，不读取真实 Apple Books 内容；
- 真实 smoke 至少覆盖中文与英文固定文本、默认音色、一个情感变体、一个风格变体，以及语速
  和音量的代表值；自动检查请求映射和 MP3 可解析，人工试听确认声音选择和控制有可感知且合理
  的效果。

## 后果

- 第一版优先得到单片段、可缓存、可恢复的同步生成闭环，不同时承担批量、流式播放和长连接状态；
- 同一 Annotation 最多有高亮和笔记两类独立 Speech Clip，播放层可连续播放但不会重复付费
  生成合并版本；
- 一个 `clip_id` 可以对应多个历史 Speech Attempt，但任一时刻至多接受一个当前 Cached
  Speech Clip；
- 同一 `clip_id` 的并发调用只允许一个 provider writer，等待者只能观察并复用该 writer 的
  已接受结果；
- Voice Profile 在不同供应商之间不保证可移植。切换供应商时，调用方必须重新验证或选择
  可用音色和控制值；
- Cached Speech Clip 与 Exported Speech Clip 有不同所有权和清理规则，缓存清理不能删除
  用户导出的音频；
- 生成、播放和导出是三个可分别重试的动作；播放失败不触发重新生成，导出失败不删除缓存；
- TTS Machine JSON Protocol 是新的跨边界合同，字段、错误和生成结果需要遵循现有 schema
  演进规则；
- 供应商响应变化被限制在 provider adapter 内；机器消费者依赖 Speech Receipt 和稳定错误码，
  不依赖 SenseAudio 的 JSON 结构；
- `speech export` 不会局部编辑用户可能已经修改过的 Markdown；音频链接仍由常规导出流程统一生成；
- 缺失或被用户修改的导出音频不会阻塞主要 Markdown 内容，但机器消费者可以通过 warning
  发现语音链接被省略；
- 远程 TTS 是 Local Data Boundary 的显式例外，但只限用户主动选择的内容和目标供应商。

## 非目标与后续决策

- 本 ADR 不实现代码，也不确定所有 CLI flag 的最终拼写、网络 timeout 数值或播放器 UI；
- 本 ADR 不提供章节/整本有声书、自动生成、语音克隆或后台预生成；
- 本 ADR 不授权 AppKit、TUI 或 Agent Data Skill 在首版调用远程 TTS；
- 本 ADR 已关闭首版的产品边界和持久状态语义；精确 CLI、JSON、文件 schema、失败路径和测试
  见关联 implementation spec，实施不得静默重新打开上述边界。
