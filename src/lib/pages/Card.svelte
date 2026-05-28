<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  interface Book { asset_id: string; title: string; author: string; note_count: number; }

  let books = $state<Book[]>([]);
  let selectedIndex = $state(1);
  let style = $state("dark");
  let outputDir = $state("~/cards/");
  let generating = $state(false);
  let resultMsg = $state("");

  $effect(() => { invoke<Book[]>("get_books").then(b => books = b); });

  async function generateAll() {
    generating = true;
    resultMsg = "";
    // Card generation will be wired up once the command is implemented
    resultMsg = "卡片生成功能待实现";
    generating = false;
  }
</script>

<div class="page">
  <h2>卡片生成</h2>

  <div class="form">
    <label>
      书籍
      <select bind:value={selectedIndex}>
        {#each books as book, i}
          <option value={i + 1}>{book.title}</option>
        {/each}
      </select>
    </label>

    <label>
      样式
      <div class="radio-group">
        <label><input type="radio" bind:group={style} value="dark" /> Dark</label>
        <label><input type="radio" bind:group={style} value="light" /> Light</label>
        <label><input type="radio" bind:group={style} value="minimal" /> Minimal</label>
      </div>
      <span class="hint">自定义模板将在后续版本支持</span>
    </label>

    <label>
      输出目录
      <input type="text" bind:value={outputDir} />
    </label>

    <button class="btn-primary" onclick={generateAll} disabled={generating}>
      {generating ? "生成中..." : "生成全部"}
    </button>
  </div>

  {#if resultMsg}
    <p class="result">{resultMsg}</p>
  {/if}
</div>

<style>
  .page { padding: 24px; }
  h2 { font-size: 20px; margin-bottom: 20px; color: var(--text-primary); }
  .form { display: flex; flex-direction: column; gap: 16px; max-width: 480px; }
  label { display: flex; flex-direction: column; gap: 6px; font-size: 14px; color: var(--text-secondary); }
  select, input { padding: 8px 12px; border: 1px solid var(--border); border-radius: 8px; background: var(--bg-secondary); color: var(--text-primary); font-size: 14px; }
  .radio-group { display: flex; gap: 16px; }
  .radio-group label { flex-direction: row; align-items: center; gap: 4px; }
  .hint { font-size: 12px; color: var(--text-secondary); }
  .btn-primary { padding: 10px 24px; background: var(--accent); color: white; border: none; border-radius: 8px; cursor: pointer; font-size: 14px; }
  .btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }
  .result { margin-top: 16px; font-size: 14px; color: var(--text-primary); }
</style>
