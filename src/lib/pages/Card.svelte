<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import ProgressBar from "../components/ProgressBar.svelte";
  import SearchableSelect from "../components/SearchableSelect.svelte";
  import { selectedBookIndex } from "../stores.svelte";

  interface Book { asset_id: string; title: string; author: string; note_count: number; }
  interface Annotation { selected_text: string | null; note: string | null; }
  interface CacheEntry { highlight: string; explanation: string; tags: string[]; question: string; }

  let books = $state<Book[]>([]);
  let selectedIndex = $state(1);
  let style = $state("dark");
  let outputDir = $state("~/cards/");
  let generating = $state(false);
  let resultMsg = $state("");
  let progress = $state(0);
  let total = $state(0);
  let message = $state("");
  let annotations = $state<Annotation[]>([]);
  let cacheEntries = $state<CacheEntry[]>([]);
  let selectedNoteIdx = $state<number | null>(null);

  $effect(() => { invoke<Book[]>("get_books").then(b => books = b); });

  // 从全局状态同步选中书籍
  $effect(() => {
    const idx = selectedBookIndex.value;
    if (idx !== null && idx > 0 && idx <= books.length) {
      selectedIndex = idx;
    }
  });

  // 加载笔记列表
  $effect(() => {
    if (books.length > 0 && selectedIndex > 0) {
      const book = books[selectedIndex - 1];
      invoke<Annotation[]>("get_annotations", { assetId: book.asset_id }).then(anns => {
        annotations = anns.filter(a => a.selected_text && a.selected_text.trim());
      });
      invoke<CacheEntry[]>("get_cache_entries", { bookId: book.asset_id }).then(entries => {
        cacheEntries = entries;
      });
    }
  });

  $effect(() => {
    const unlistenProgress = listen("progress", (e: any) => {
      progress = e.payload.current;
      total = e.payload.total;
      message = e.payload.message;
    });
    const unlistenComplete = listen("complete", (e: any) => {
      const r = e.payload;
      resultMsg = `生成完成: 成功 ${r.success} 张, 失败 ${r.failed} 张`;
      generating = false;
    });
    return () => {
      unlistenProgress.then(fn => fn());
      unlistenComplete.then(fn => fn());
    };
  });

  // 检查笔记是否有 AI 增强
  function hasAI(text: string): boolean {
    return cacheEntries.some(e => e.highlight === text && e.explanation);
  }

  // 生成单张卡片
  async function generateSingle(idx: number) {
    generating = true;
    resultMsg = "";
    try {
      await invoke("generate_cards_cmd", {
        bookIndex: selectedIndex,
        style,
        outputDir,
        mode: "single",
        singleIndex: idx + 1,
      });
      resultMsg = "生成成功";
    } catch (e: any) {
      resultMsg = `生成失败: ${e}`;
    }
    generating = false;
  }

  // 生成全部
  async function generateAll() {
    generating = true;
    resultMsg = "";
    progress = 0;
    try {
      await invoke("generate_cards_cmd", {
        bookIndex: selectedIndex,
        style,
        outputDir,
        mode: "all",
      });
    } catch (e: any) {
      resultMsg = `生成失败: ${e}`;
      generating = false;
    }
  }
</script>

<div class="page">
  <h2>卡片生成</h2>

  <div class="form">
    <label>
      书籍
      <SearchableSelect
        items={books.map((b, i) => ({ label: `${b.title} — ${b.author}`, value: i + 1 }))}
        bind:value={selectedIndex}
        placeholder="搜索书名或作者..."
      />
    </label>

    <label>
      样式
      <div class="radio-group">
        <label><input type="radio" bind:group={style} value="dark" /> Dark</label>
        <label><input type="radio" bind:group={style} value="light" /> Light</label>
        <label><input type="radio" bind:group={style} value="minimal" /> Minimal</label>
      </div>
    </label>

    <label>
      输出目录
      <input type="text" bind:value={outputDir} />
    </label>

    <button class="btn-primary" onclick={generateAll} disabled={generating}>
      {generating ? "生成中..." : "生成全部"}
    </button>
  </div>

  {#if generating || progress > 0}
    <div class="progress-section">
      <ProgressBar value={progress} max={total} />
      <p class="msg">{message}</p>
    </div>
  {/if}

  {#if resultMsg}
    <p class="result">{resultMsg}</p>
  {/if}

  <!-- 笔记列表 -->
  {#if annotations.length > 0}
    <div class="notes-section">
      <h3>笔记列表 ({annotations.length} 条)</h3>
      <div class="notes-list">
        {#each annotations as ann, idx}
          <div class="note-item">
            <div class="note-content">
              <span class="note-idx">#{idx + 1}</span>
              <span class="note-text">{ann.selected_text}</span>
              {#if hasAI(ann.selected_text!)}
                <span class="badge ai">AI</span>
              {/if}
              {#if ann.note}
                <span class="badge note">笔记</span>
              {/if}
            </div>
            <button class="btn-generate" onclick={() => generateSingle(idx)} disabled={generating}>
              生成卡片
            </button>
          </div>
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .page { padding: 24px; }
  h2 { font-size: 20px; margin-bottom: 20px; color: var(--text-primary); }
  h3 { font-size: 16px; margin-bottom: 16px; color: var(--text-primary); }
  .form { display: flex; flex-direction: column; gap: 16px; max-width: 480px; margin-bottom: 24px; }
  label { display: flex; flex-direction: column; gap: 6px; font-size: 14px; color: var(--text-secondary); }
  input[type="text"] { padding: 8px 12px; border: 1px solid var(--border); border-radius: 8px; background: var(--bg-secondary); color: var(--text-primary); font-size: 14px; }
  .radio-group { display: flex; gap: 16px; }
  .radio-group label { flex-direction: row; align-items: center; gap: 4px; }
  .btn-primary { padding: 10px 24px; background: var(--accent); color: white; border: none; border-radius: 8px; cursor: pointer; font-size: 14px; align-self: flex-start; }
  .btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }
  .progress-section { margin-top: 24px; }
  .msg { font-size: 13px; color: var(--text-secondary); margin-top: 8px; }
  .result { margin-top: 16px; font-size: 14px; color: var(--text-primary); }

  .notes-section { margin-top: 24px; }
  .notes-list { display: flex; flex-direction: column; gap: 8px; }
  .note-item { display: flex; justify-content: space-between; align-items: center; padding: 12px; background: var(--card-bg, var(--bg-secondary)); border: 1px solid var(--border); border-radius: 8px; gap: 12px; }
  .note-content { display: flex; align-items: center; gap: 8px; flex: 1; min-width: 0; }
  .note-idx { font-size: 12px; color: var(--text-secondary); font-weight: 600; min-width: 30px; }
  .note-text { font-size: 13px; color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; }
  .badge { font-size: 10px; padding: 2px 6px; border-radius: 4px; font-weight: 500; }
  .badge.ai { background: rgba(0,122,255,0.15); color: var(--accent, #007aff); }
  .badge.note { background: rgba(255,149,0,0.15); color: #ff9500; }
  .btn-generate { padding: 6px 12px; background: var(--bg-secondary); border: 1px solid var(--border); border-radius: 6px; cursor: pointer; font-size: 12px; color: var(--text-primary); white-space: nowrap; }
  .btn-generate:hover { background: var(--accent); color: white; border-color: var(--accent); }
  .btn-generate:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
