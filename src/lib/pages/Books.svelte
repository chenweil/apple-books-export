<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import BookCard from "../components/BookCard.svelte";
  import LoadingSpinner from "../components/LoadingSpinner.svelte";

  interface Book {
    asset_id: string;
    title: string;
    author: string;
    note_count: number;
  }

  let books = $state<Book[]>([]);
  let loading = $state(true);
  let search = $state("");
  let page = $state(1);
  let prevSearch = $state("");
  const PAGE_SIZE = 20;

  let filtered = $derived(
    books.filter(b =>
      b.title.toLowerCase().includes(search.toLowerCase()) ||
      b.author.toLowerCase().includes(search.toLowerCase())
    )
  );
  let totalPages = $derived(Math.max(1, Math.ceil(filtered.length / PAGE_SIZE)));
  let paged = $derived(filtered.slice((page - 1) * PAGE_SIZE, page * PAGE_SIZE));

  // 搜索时自动回到第一页
  $effect(() => {
    if (search !== prevSearch) {
      prevSearch = search;
      page = 1;
    }
  });

  async function loadBooks() {
    loading = true;
    try {
      books = await invoke("get_books");
    } catch (e) {
      console.error(e);
    }
    loading = false;
  }

  $effect(() => { loadBooks(); });
</script>

<div class="page">
  <div class="header">
    <h2>书籍列表</h2>
    <div class="search-box">
      <input
        type="text"
        placeholder="搜索书籍..."
        bind:value={search}
      />
      <button class="btn-refresh" onclick={loadBooks}>刷新</button>
    </div>
  </div>

  {#if loading}
    <LoadingSpinner message="加载书籍列表..." />
  {:else if books.length === 0}
    <div class="empty">
      <p>未找到有笔记的书籍</p>
      <p class="hint">请确保 Apple Books 中有高亮或笔记</p>
    </div>
  {:else}
    <div class="list">
      {#each paged as book}
        <BookCard {book} />
      {/each}
    </div>

    <div class="pagination">
      <button disabled={page <= 1} onclick={() => page--}>◄</button>
      <span>第 {page}/{totalPages} 页</span>
      <button disabled={page >= totalPages} onclick={() => page++}>►</button>
    </div>
  {/if}
</div>

<style>
  .page { padding: 24px; }
  .header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 20px; }
  h2 { font-size: 20px; color: var(--text-primary); }
  .search-box { display: flex; gap: 8px; }
  input {
    padding: 8px 12px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-secondary);
    color: var(--text-primary);
    font-size: 14px;
    width: 200px;
  }
  .btn-refresh {
    padding: 8px 16px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-secondary);
    color: var(--text-primary);
    cursor: pointer;
    font-size: 14px;
  }
  .list { display: flex; flex-direction: column; gap: 8px; }
  .pagination {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 16px;
    margin-top: 20px;
    font-size: 14px;
    color: var(--text-secondary);
  }
  .pagination button {
    padding: 6px 12px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg-secondary);
    cursor: pointer;
    color: var(--text-primary);
  }
  .pagination button:disabled { opacity: 0.3; cursor: not-allowed; }
  .empty { text-align: center; padding: 60px 0; color: var(--text-secondary); }
  .hint { font-size: 13px; margin-top: 8px; }
</style>
