<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import BookCard from "../components/BookCard.svelte";
  import LoadingSpinner from "../components/LoadingSpinner.svelte";
  import { navigateToPageWithBook } from "../stores.svelte";

  interface Book {
    asset_id: string;
    title: string;
    author: string;
    note_count: number;
  }

  type SortField = 'default' | 'title' | 'note_count';
  type SortOrder = 'asc' | 'desc';

  let books = $state<Book[]>([]);
  let loading = $state(true);
  let search = $state("");
  let page = $state(1);
  let prevSearch = $state("");
  let sortField = $state<SortField>('default');
  let sortOrder = $state<SortOrder>('desc');
  const PAGE_SIZE = 20;

  let filtered = $derived(
    books.filter(b =>
      b.title.toLowerCase().includes(search.toLowerCase()) ||
      b.author.toLowerCase().includes(search.toLowerCase())
    )
  );

  // 排序后的列表
  let sorted = $derived(() => {
    if (sortField === 'default') {
      return filtered;
    }
    const sorted = [...filtered];
    sorted.sort((a, b) => {
      let cmp = 0;
      if (sortField === 'title') {
        cmp = a.title.localeCompare(b.title, 'zh-CN');
      } else if (sortField === 'note_count') {
        cmp = a.note_count - b.note_count;
      }
      return sortOrder === 'asc' ? cmp : -cmp;
    });
    return sorted;
  });

  let totalPages = $derived(Math.max(1, Math.ceil(sorted().length / PAGE_SIZE)));
  let paged = $derived(sorted().slice((page - 1) * PAGE_SIZE, page * PAGE_SIZE));

  // 搜索或排序变化时自动回到第一页
  $effect(() => {
    if (search !== prevSearch) {
      prevSearch = search;
      page = 1;
    }
  });

  $effect(() => {
    page = 1;
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

  function handleAction(action: string, bookIndex: number) {
    // bookIndex 是 0-based，转换为 1-based
    navigateToPageWithBook(action, bookIndex + 1);
  }

  function toggleSort(field: SortField) {
    if (sortField === field) {
      // 切换排序方向
      sortOrder = sortOrder === 'asc' ? 'desc' : 'asc';
    } else {
      sortField = field;
      sortOrder = 'desc';
    }
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

  <div class="sort-bar">
    <span class="sort-label">排序：</span>
    <button
      class="sort-btn"
      class:active={sortField === 'default'}
      onclick={() => { sortField = 'default'; }}
    >
      默认
    </button>
    <button
      class="sort-btn"
      class:active={sortField === 'title'}
      onclick={() => toggleSort('title')}
    >
      书名 {sortField === 'title' ? (sortOrder === 'asc' ? '↑' : '↓') : ''}
    </button>
    <button
      class="sort-btn"
      class:active={sortField === 'note_count'}
      onclick={() => toggleSort('note_count')}
    >
      笔记数 {sortField === 'note_count' ? (sortOrder === 'asc' ? '↑' : '↓') : ''}
    </button>
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
      {#each paged as book, idx}
        <BookCard {book} bookIndex={(page - 1) * PAGE_SIZE + idx} onAction={handleAction} />
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
  .header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px; }
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
  .sort-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 16px;
    font-size: 13px;
  }
  .sort-label {
    color: var(--text-secondary);
  }
  .sort-btn {
    padding: 4px 12px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg-secondary);
    color: var(--text-secondary);
    cursor: pointer;
    font-size: 13px;
    transition: all 0.15s;
  }
  .sort-btn:hover {
    background: var(--bg-primary);
    color: var(--text-primary);
  }
  .sort-btn.active {
    background: var(--accent);
    border-color: var(--accent);
    color: white;
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
