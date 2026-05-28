<script lang="ts">
  let { currentPage = $bindable("books") } = $props();

  const pages = [
    { id: "books", icon: "📚", label: "书籍" },
    { id: "export", icon: "📤", label: "导出" },
    { id: "enrich", icon: "✨", label: "增强" },
    { id: "card", icon: "🎨", label: "卡片" },
    { id: "cache", icon: "💾", label: "缓存" },
    { id: "config", icon: "⚙️", label: "设置" },
  ];
</script>

<nav class="sidebar">
  {#each pages as page}
    <button
      class="nav-item"
      class:active={currentPage === page.id}
      onclick={() => (currentPage = page.id)}
    >
      <span class="icon">{page.icon}</span>
      <span class="label">{page.label}</span>
    </button>
  {/each}
</nav>

<style>
  .sidebar {
    width: 72px;
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 12px 0;
    gap: 4px;
    background: rgba(246, 246, 246, 0.8);
    border-right: 1px solid rgba(0, 0, 0, 0.1);
    -webkit-app-region: drag;
  }
  .nav-item {
    -webkit-app-region: no-drag;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    padding: 8px 12px;
    border: none;
    background: transparent;
    border-radius: 8px;
    cursor: pointer;
    width: 56px;
  }
  .nav-item:hover {
    background: rgba(0, 0, 0, 0.05);
  }
  .nav-item.active {
    background: rgba(0, 122, 255, 0.15);
  }
  .icon { font-size: 20px; }
  .label { font-size: 10px; color: #333; }

  @media (prefers-color-scheme: dark) {
    .sidebar {
      background: rgba(40, 40, 40, 0.8);
      border-right-color: rgba(255, 255, 255, 0.1);
    }
    .nav-item:hover { background: rgba(255, 255, 255, 0.05); }
    .nav-item.active { background: rgba(0, 122, 255, 0.25); }
    .label { color: #ccc; }
  }
</style>
