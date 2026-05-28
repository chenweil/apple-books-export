<script lang="ts">
  let { book, onclick = () => {} } = $props();
</script>

<button class="book-card" {onclick}>
  <div class="icon">📖</div>
  <div class="info">
    <div class="title">{book.title}</div>
    <div class="author">{book.author}</div>
  </div>
  <div class="count">{book.note_count} 笔记</div>
</button>

<style>
  .book-card {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 16px;
    background: var(--card-bg);
    border: 1px solid var(--border);
    border-radius: 8px;
    cursor: pointer;
    width: 100%;
    text-align: left;
    transition: background 0.15s;
  }
  .book-card:hover { background: var(--bg-primary); }
  .icon { font-size: 24px; flex-shrink: 0; }
  .info { flex: 1; min-width: 0; }
  .title {
    font-size: 14px;
    font-weight: 500;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .author {
    font-size: 12px;
    color: var(--text-secondary);
    margin-top: 2px;
  }
  .count {
    font-size: 12px;
    color: var(--accent);
    white-space: nowrap;
  }
</style>
