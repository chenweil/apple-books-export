<script lang="ts">
  let { book, bookIndex, onAction } = $props<{
    book: { asset_id: string; title: string; author: string; note_count: number };
    bookIndex: number;
    onAction?: (action: string, index: number) => void;
  }>();
</script>

<div class="book-card">
  <div class="icon">📖</div>
  <div class="info">
    <div class="title">{book.title}</div>
    <div class="author">{book.author}</div>
  </div>
  <div class="actions">
    <span class="count">{book.note_count} 笔记</span>
    <button class="action-btn" onclick={() => onAction?.('export', bookIndex)}>导出</button>
    <button class="action-btn" onclick={() => onAction?.('enrich', bookIndex)}>AI增强</button>
    <button class="action-btn" onclick={() => onAction?.('card', bookIndex)}>图片卡片</button>
    <button class="action-btn" onclick={() => onAction?.('coach', bookIndex)}>陪读</button>
    <button class="action-btn" onclick={() => onAction?.('cache', bookIndex)}>缓存</button>
  </div>
</div>

<style>
  .book-card {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 16px;
    background: var(--card-bg);
    border: 1px solid var(--border);
    border-radius: 8px;
    width: 100%;
    text-align: left;
  }
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
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }
  .count {
    font-size: 12px;
    color: var(--accent);
    white-space: nowrap;
  }
  .action-btn {
    padding: 4px 10px;
    font-size: 12px;
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text-primary);
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s;
    white-space: nowrap;
  }
  .action-btn:hover {
    background: var(--accent);
    border-color: var(--accent);
    color: white;
  }
</style>
