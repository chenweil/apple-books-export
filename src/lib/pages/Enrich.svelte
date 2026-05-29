<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import ProgressBar from "../components/ProgressBar.svelte";
  import Toast from "../components/Toast.svelte";
  import SearchableSelect from "../components/SearchableSelect.svelte";

  interface Book { asset_id: string; title: string; author: string; note_count: number; }

  let books = $state<Book[]>([]);
  let selectedIndex = $state(1);
  let mode = $state<"incremental" | "all" | "single">("incremental");
  let singleIndex = $state(1);
  let force = $state(false);
  let processing = $state(false);
  let progress = $state(0);
  let total = $state(0);
  let message = $state("");
  let toastMsg = $state("");
  let result = $state<{ success: number; failed: number } | null>(null);

  $effect(() => { invoke<Book[]>("get_books").then(b => books = b); });

  $effect(() => {
    const unlistenProgress = listen("progress", (e: any) => {
      progress = e.payload.current;
      total = e.payload.total;
      message = e.payload.message;
    });
    const unlistenError = listen("error", (e: any) => {
      toastMsg = e.payload.message;
    });
    const unlistenComplete = listen("complete", (e: any) => {
      result = e.payload;
      processing = false;
    });
    return () => {
      unlistenProgress.then(fn => fn());
      unlistenError.then(fn => fn());
      unlistenComplete.then(fn => fn());
    };
  });

  async function startEnrich() {
    processing = true;
    progress = 0;
    result = null;
    try {
      await invoke("enrich_book_cmd", {
        bookIndex: selectedIndex,
        force,
        mode,
        singleIndex: mode === "single" ? singleIndex : null,
      });
    } catch (e: any) {
      toastMsg = `处理失败: ${e}`;
      processing = false;
    }
  }
</script>

<div class="page">
  <h2>AI 增强</h2>

  <div class="form">
    <label>
      书籍
      <SearchableSelect
        items={books.map((b, i) => ({ label: `${b.title} — ${b.author}`, value: i + 1 }))}
        bind:value={selectedIndex}
        placeholder="搜索书名或作者..."
      />
    </label>

    <div class="mode">
      <label><input type="radio" bind:group={mode} value="incremental" /> 增量</label>
      <label><input type="radio" bind:group={mode} value="all" /> 全量</label>
      <label><input type="radio" bind:group={mode} value="single" /> 单条</label>
      {#if mode === "single"}
        <input type="number" bind:value={singleIndex} min="1" style="width: 60px;" />
      {/if}
    </div>

    <label class="checkbox">
      <input type="checkbox" bind:checked={force} /> 强制刷新（忽略缓存）
    </label>

    <button class="btn-primary" onclick={startEnrich} disabled={processing}>
      {processing ? "处理中..." : "开始处理"}
    </button>
  </div>

  {#if processing || progress > 0}
    <div class="progress-section">
      <ProgressBar value={progress} max={total} />
      <p class="msg">{message}</p>
      {#if message.includes("正在调用 LLM")}
        <p class="hint">LLM API 响应通常需要 10-15 秒，请耐心等待...</p>
      {/if}
    </div>
  {/if}

  {#if result}
    <div class="result">
      完成: 成功 {result.success} 条, 失败 {result.failed} 条
    </div>
  {/if}
</div>

{#if toastMsg}
  <Toast message={toastMsg} type="error" onclose={() => toastMsg = ""} />
{/if}

<style>
  .page { padding: 24px; }
  h2 { font-size: 20px; margin-bottom: 20px; color: var(--text-primary); }
  .form { display: flex; flex-direction: column; gap: 16px; max-width: 480px; }
  label { display: flex; flex-direction: column; gap: 6px; font-size: 14px; color: var(--text-secondary); }
  select, input { padding: 8px 12px; border: 1px solid var(--border); border-radius: 8px; background: var(--bg-secondary); color: var(--text-primary); font-size: 14px; }
  .mode { display: flex; gap: 16px; align-items: center; font-size: 14px; color: var(--text-secondary); }
  .mode label { flex-direction: row; align-items: center; gap: 4px; }
  .checkbox { flex-direction: row; align-items: center; gap: 6px; }
  .btn-primary { padding: 10px 24px; background: var(--accent); color: white; border: none; border-radius: 8px; cursor: pointer; font-size: 14px; }
  .btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }
  .progress-section { margin-top: 24px; }
  .msg { font-size: 13px; color: var(--text-secondary); margin-top: 8px; }
  .hint { font-size: 12px; color: var(--text-tertiary, #888); margin-top: 4px; font-style: italic; }
  .result { margin-top: 16px; padding: 12px; background: rgba(52,199,89,0.1); border-radius: 8px; font-size: 14px; color: #34c759; }
</style>
