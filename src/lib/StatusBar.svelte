<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  let bookCount = $state(0);
  let cacheCount = $state(0);
  let lastSync = $state("--:--");

  async function refresh() {
    try {
      const books: any[] = await invoke("get_books");
      bookCount = books.length;
      let total = 0;
      for (const book of books) {
        const stats: any = await invoke("get_cache_stats", { bookId: book.asset_id });
        total += stats.cached;
      }
      cacheCount = total;
      const now = new Date();
      lastSync = `${now.getHours().toString().padStart(2, "0")}:${now.getMinutes().toString().padStart(2, "0")}`;
    } catch {}
  }

  $effect(() => { refresh(); });
</script>

<footer class="status-bar">
  <span>📚 {bookCount} 本书</span>
  <span>💾 缓存 {cacheCount} 条</span>
  <span>🕐 最后同步 {lastSync}</span>
  <span class="version">v0.3.2</span>
</footer>

<style>
  .status-bar {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 4px 16px;
    font-size: 12px;
    color: var(--text-secondary);
    background: var(--bg-primary);
    border-top: 1px solid var(--border);
    -webkit-user-select: none;
  }
  .version { margin-left: auto; }
</style>
