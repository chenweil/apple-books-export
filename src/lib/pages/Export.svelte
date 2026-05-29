<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import ProgressBar from "../components/ProgressBar.svelte";
  import SearchableSelect from "../components/SearchableSelect.svelte";

  interface Book { asset_id: string; title: string; author: string; note_count: number; }
  interface Annotation { asset_id: string; selected_text: string | null; note: string | null; annotation_type: number; }

  let books = $state<Book[]>([]);
  let selectedIndex = $state(1);
  let format = $state("obsidian");
  let outputDir = $state("~/Documents/books/");
  let exporting = $state(false);
  let progress = $state(0);
  let total = $state(0);
  let message = $state("");
  let resultPath = $state("");
  let preview = $state("");

  $effect(() => {
    invoke<Book[]>("get_books").then(b => { books = b; });
  });

  // Update preview when book changes
  $effect(() => {
    if (books.length > 0 && selectedIndex > 0 && selectedIndex <= books.length) {
      const book = books[selectedIndex - 1];
      invoke<Annotation[]>("get_annotations", { assetId: book.asset_id }).then(annotations => {
        const lines: string[] = [];
        if (format === "obsidian") {
          lines.push("---");
          lines.push("type: llm-note");
          lines.push(`book: ${book.title}`);
          lines.push("---");
        }
        lines.push("");
        lines.push(`# ${book.title}`);
        lines.push(`作者: ${book.author}`);
        lines.push("");
        lines.push("## 高亮与标注");
        lines.push("");
        const highlights = annotations.filter(a => a.selected_text);
        for (const ann of highlights.slice(0, 5)) {
          lines.push(`> ${ann.selected_text!.slice(0, 100)}${ann.selected_text!.length > 100 ? "..." : ""}`);
          if (ann.note) lines.push(`笔记: ${ann.note}`);
          lines.push("");
        }
        if (highlights.length > 5) {
          lines.push(`... 还有 ${highlights.length - 5} 条`);
        }
        preview = lines.join("\n");
      });
    }
  });

  $effect(() => {
    const unlisten = listen("progress", (event: any) => {
      progress = event.payload.current;
      total = event.payload.total;
      message = event.payload.message;
    });
    return () => { unlisten.then(fn => fn()); };
  });

  async function doExport() {
    exporting = true;
    progress = 0;
    resultPath = "";
    try {
      resultPath = await invoke("export_book_cmd", {
        bookIndex: selectedIndex,
        outputDir: outputDir.replace("~", "/Users/chenweilong"),
        format,
      });
    } catch (e: any) {
      message = `导出失败: ${e}`;
    }
    exporting = false;
  }
</script>

<div class="page">
  <h2>导出笔记</h2>

  <div class="form">
    <label>
      选择书籍
      <SearchableSelect
        items={books.map((b, i) => ({ label: `${b.title} — ${b.author}`, value: i + 1 }))}
        bind:value={selectedIndex}
        placeholder="搜索书名或作者..."
      />
    </label>

    <label>
      输出格式
      <div class="radio-group">
        <label><input type="radio" bind:group={format} value="obsidian" /> Obsidian</label>
        <label><input type="radio" bind:group={format} value="markdown" /> Markdown</label>
      </div>
    </label>

    <label>
      输出目录
      <input type="text" bind:value={outputDir} placeholder="~/Documents/books/" />
    </label>

    <button class="btn-primary" onclick={doExport} disabled={exporting}>
      {exporting ? "导出中..." : "导出 Markdown"}
    </button>
  </div>

  {#if preview}
    <div class="preview-section">
      <h3>预览</h3>
      <pre class="preview">{preview}</pre>
    </div>
  {/if}

  {#if exporting || progress > 0}
    <div class="progress-section">
      <ProgressBar value={progress} max={total} />
      <p class="message">{message}</p>
    </div>
  {/if}

  {#if resultPath}
    <div class="result">
      导出成功: <code>{resultPath}</code>
    </div>
  {/if}
</div>

<style>
  .page { padding: 24px; }
  h2 { font-size: 20px; margin-bottom: 20px; color: var(--text-primary); }
  h3 { font-size: 16px; margin-bottom: 8px; color: var(--text-primary); }
  .form { display: flex; flex-direction: column; gap: 16px; max-width: 480px; }
  label { display: flex; flex-direction: column; gap: 6px; font-size: 14px; color: var(--text-secondary); }
  select, input[type="text"] {
    padding: 8px 12px; border: 1px solid var(--border); border-radius: 8px;
    background: var(--bg-secondary); color: var(--text-primary); font-size: 14px;
  }
  .radio-group { display: flex; gap: 16px; }
  .radio-group label { flex-direction: row; align-items: center; gap: 4px; }
  .btn-primary {
    padding: 10px 24px; background: var(--accent); color: white; border: none;
    border-radius: 8px; cursor: pointer; font-size: 14px; font-weight: 500;
  }
  .btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }
  .preview-section { margin-top: 24px; }
  .preview {
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 16px;
    font-family: "SF Mono", "Menlo", monospace;
    font-size: 12px;
    line-height: 1.6;
    max-height: 300px;
    overflow-y: auto;
    color: var(--text-primary);
    white-space: pre-wrap;
    word-break: break-word;
  }
  .progress-section { margin-top: 24px; }
  .message { font-size: 13px; color: var(--text-secondary); margin-top: 8px; }
  .result { margin-top: 16px; padding: 12px; background: rgba(52,199,89,0.1); border-radius: 8px; font-size: 14px; color: #34c759; }
</style>
