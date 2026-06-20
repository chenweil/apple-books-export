<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import SearchableSelect from "../components/SearchableSelect.svelte";

  interface Book { asset_id: string; title: string; author: string; note_count: number; }
  interface CacheEntry { key: string; highlight: string; explanation: string; tags: string[]; updated: string; }

  let books = $state<Book[]>([]);
  let selectedIndex = $state(1);
  let entries = $state<CacheEntry[]>([]);
  let stats = $state({ total: 0, cached: 0, uncached: 0 });
  let showConfirm = $state(false);

  async function loadCache() {
    if (books.length === 0) return;
    const book = books[selectedIndex - 1];
    entries = await invoke("get_cache_entries", { bookId: book.asset_id });
    stats = await invoke("get_cache_stats", { bookId: book.asset_id });
  }

  $effect(() => {
    invoke<Book[]>("get_books").then(b => { books = b; });
  });

  $effect(() => {
    if (books.length > 0 && selectedIndex > 0) loadCache();
  });

  async function doClear() {
    const book = books[selectedIndex - 1];
    await invoke("clear_cache_for_book", { bookId: book.asset_id });
    showConfirm = false;
    loadCache();
  }
</script>

<div class="page">
  <h2>缓存管理</h2>

  <label class="select-book">
    书籍
    <SearchableSelect
      items={books.map((b, i) => ({ label: `${b.title} — ${b.author}`, value: i + 1 }))}
      bind:value={selectedIndex}
      placeholder="搜索书名或作者..."
    />
  </label>

  <div class="stats">
    <div class="stat">
      <span class="num">{stats.total}</span>
      <span class="label">总笔记</span>
    </div>
    <div class="stat">
      <span class="num">{stats.cached}</span>
      <span class="label">已缓存 ({stats.total > 0 ? Math.round(stats.cached / stats.total * 100) : 0}%)</span>
    </div>
    <div class="stat">
      <span class="num">{stats.uncached}</span>
      <span class="label">未缓存</span>
    </div>
  </div>

  <div class="entries">
    {#each entries as entry}
      <div class="entry">
        <div class="highlight">{entry.highlight.slice(0, 60)}...</div>
        <div class="meta">
          {#each entry.tags as tag}
            <span class="tag">{tag}</span>
          {/each}
          <span class="date">{entry.updated}</span>
        </div>
      </div>
    {/each}
    {#if entries.length === 0}
      <p class="empty">暂无缓存</p>
    {/if}
  </div>

  <button class="btn-danger" onclick={() => showConfirm = true}>清空缓存</button>
</div>

{#if showConfirm}
  <ConfirmDialog
    title="清空缓存"
    message="确定要清空该书的所有 LLM 缓存吗？此操作不可撤销。"
    onConfirm={doClear}
    onCancel={() => showConfirm = false}
  />
{/if}

<style>
  .page { padding: 24px; }
  h2 { font-size: 20px; margin-bottom: 20px; color: var(--text-primary); }
  .select-book { display: flex; flex-direction: column; gap: 6px; font-size: 14px; color: var(--text-secondary); max-width: 300px; margin-bottom: 20px; }
  .stats { display: flex; gap: 24px; margin-bottom: 24px; }
  .stat { display: flex; flex-direction: column; align-items: center; }
  .num { font-size: 28px; font-weight: 600; color: var(--accent); }
  .label { font-size: 12px; color: var(--text-secondary); }
  .entries { display: flex; flex-direction: column; gap: 8px; margin-bottom: 20px; }
  .entry { padding: 12px; background: var(--card-bg); border: 1px solid var(--border); border-radius: 8px; }
  .highlight { font-size: 14px; color: var(--text-primary); margin-bottom: 6px; }
  .meta { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; }
  .tag { font-size: 11px; padding: 2px 8px; background: rgba(0,122,255,0.1); color: var(--accent); border-radius: 4px; }
  .date { font-size: 11px; color: var(--text-secondary); }
  .empty { color: var(--text-secondary); font-size: 14px; }
  .btn-danger { padding: 8px 20px; background: #ff3b30; color: white; border: none; border-radius: 8px; cursor: pointer; font-size: 14px; }
</style>
