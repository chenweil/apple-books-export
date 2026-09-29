# Annotation Speech 验收状态（issue #29 聚合合同锁）

本文件记录 #21–#29 合并后的**实际**验证状态，区分「已被自动化证明」与「仍然 pending」。
它的唯一目的是让 #30（真实供应商验收）知道还剩什么，**不从本地 mock 推断任何真实
provider 或人工听感结论**。

## 1. 已被自动化证明（本地，全部 hermetic）

以下保证由 `cargo test` 覆盖，使用进程内 TCP mock provider、注入 HOME 隔离的 Speech
状态根、移除全部代理变量、secret canary 断言。**这些证明的是本地合同，不是供应商行为。**

| 边界 | 覆盖位置 |
| --- | --- |
| 每个叶命令有人类 help 与版本化 Machine JSON 信封 | `tests/speech_contract_lock_cli.rs::every_speech_leaf_command_has_human_help_and_a_json_flag`、`::every_speech_success_receipt_is_versioned_and_keeps_streams_separate`、`::every_speech_leaf_command_fails_with_a_versioned_error_envelope` |
| 稳定错误码白名单、错误码不随运行变化 | `src/speech/machine.rs::every_speech_error_code_is_in_the_documented_stable_set`、`::the_stable_set_has_no_duplicates`、`::normalize_recorded_failure_code` 归一化断言 |
| 内容选择：高亮/笔记是两个独立 clip，另一侧稳定失败 | `tests/speech_generate_cli.rs`、`::content_selection_refuses_the_side_an_annotation_does_not_have` |
| 无隐式网络：只有显式 generate 与 voices 联网 | `::only_explicit_generation_and_voice_browsing_may_contact_the_provider`（缓存命中、play、export、cache/history 维护均断言 0 连接） |
| single-flight：同 clip 并发恰好一次 provider 调用 | `tests/speech_generate_cli.rs::concurrent_generations_of_one_clip_produce_exactly_one_provider_call`、等待方复用首个终态 |
| unknown gating：不自动重放 | `::an_uncertain_outcome_records_an_unknown_gate_and_never_replays`、provider 成功后产物缺失的阻塞态 |
| 缓存完整性：改一个字节即拒绝 | `::a_tampered_entry_is_a_stable_code_rather_than_a_varying_one`、`tests/speech_play_cli.rs`、`tests/speech_export_cli.rs` |
| 原子状态：提交失败不留半成品 | `::a_commit_failure_returns_artifact_commit_failed_and_gates_the_clip`、`::an_interrupted_export_never_commits_a_manifest_pointing_at_a_missing_file` |
| 导出包含性：路径不得逃出导出根 | `::a_manifest_traversal_path_never_escapes_the_export_root` |
| 清单权威性：损坏清单不重建、不猜测 | `::a_corrupt_manifest_is_never_rebuilt_or_guessed` |
| 秘密安全：状态与输出都不含 API Key | `::no_speech_state_or_command_output_ever_contains_the_api_key`、`::error_details_carry_diagnostics_without_source_text_or_secrets` |
| 既有的 list/annotations/export/doctor/TUI/Skill 不获得隐式 Speech 行为 | `::existing_read_only_commands_gain_no_implicit_speech_behavior`、`::human_read_only_commands_create_no_speech_state`；另见 `bash tests/headless_mainline.sh` 与 `bash skills/apple-books-export-rust/tests/contract.sh` |

## 2. 仍然 pending（#30 必须执行，本文件不代为结论）

以下**没有**被本次改动验证。它们要么需要真实供应商与真实凭据，要么需要人的耳朵。
mock 覆盖了请求形状与本地状态机，**不覆盖**供应商是否真的接受这些请求、返回的音频是否
真的是期望的声音。

1. **真实 SenseAudio 生成**：固定非用户文本下的中英文合成是否成功、返回的 hex 是否形成
   可解析 MP3、usage/duration/trace 是否与本地记录一致。
2. **人工听感**：音色、情感变体、风格变体、语速与音量是否产生**可感知且合理**的差异。
   这一项自动化无法替代。
3. **账号权限与真实音色目录**：当前账号实际可见的 `voice_id` 集合、默认音色
   `male_0004_a` 是否始终可用、24 小时 Voice Catalog 缓存与 `--refresh` 在真实限流下的
   行为。
4. **真实限流与鉴权失败**：`SPEECH_RATE_LIMITED` 与 `SPEECH_AUTH_FAILED` 的真实供应商
   响应形状（当前由 mock 构造）。
5. **计费一致性**：`estimated_billing_characters` 与供应商账单的实际关系。程序不硬编码
   货币价格，最终金额以供应商账单为准，因此这一项**结构性地**无法在本地证明。
6. **真实存储与空间行为**：1 GiB 预算、128 MiB 安全余量与 LRU 在真实磁盘上的表现
   （当前用注入的临时根与 `APPLE_BOOKS_SPEECH_MIN_FREE_BYTES` 覆盖）。
7. **macOS 播放器**：`afplay` 实际播放与 `SPEECH_PLAYBACK_FAILED` 的真实触发条件
   （human 路径由可注入的 `AudioPlayer` seam 单元测试覆盖，未在真实播放器上执行）。
8. **AppKit 消费**：未来 AppKit 直接消费同一份 Machine JSON 合同——本次**没有**任何
   AppKit 验证。

已知的真实供应商契约证据（#20，2026-09-11 固定文本 smoke，含人工试听）记录在
[`2026-09-11-senseaudio-contract-smoke.md`](2026-09-11-senseaudio-contract-smoke.md)。
它验证的是**供应商 API 形状**，不是本 CLI 的 Speech 功能；两者不可互相推断。该文件
同时记录了一处仍未做的对照：真实 provider break control 的并排试听
（`comparison_performed=false`）。

## 3. 本次聚合验收修复的缺陷

见提交 `test(speech): lock the stable Machine JSON error code set (#29)`：

- `SPEECH_PLAYBACK_FAILED` 由 #26 引入并实际会发出，但从未进入实施 spec 6.2 的稳定码
  表——机器消费者无法从文档得知它存在。已补录进文档，并加入代码白名单强制。
- 跨进程排队等待方从磁盘 attempt 记录还原终态错误码时，缺少对白名单的归一化入口，
  篡改或过期的记录可能让同一 clip 在不同运行返回不同 `code`。已把
  `stable_failure_code` 提升为公开的 `normalize_recorded_failure_code` 并对其加断言。

## 4. 本次执行的检查

```bash
cargo build
cargo test                       # 355 passed（含 #21–#28 的 340 个既有测试）
cargo check --all-targets
bash tests/headless_mainline.sh
bash skills/apple-books-export-rust/tests/contract.sh
```

`bash tests/senseaudio_contract.sh` 同样保持绿色，但它是 #20 的供应商 API 形状合同，
使用本地 mock，**不构成**本次 Speech 功能的真实供应商验证。
