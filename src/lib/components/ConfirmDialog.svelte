<script lang="ts">
  let { title = "确认", message = "", onConfirm = () => {}, onCancel = () => {} } = $props();
</script>

<div class="overlay">
  <div class="dialog">
    <h3>{title}</h3>
    <p>{message}</p>
    <div class="actions">
      <button class="btn" onclick={onCancel}>取消</button>
      <button class="btn danger" onclick={onConfirm}>确认</button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed; inset: 0; background: rgba(0,0,0,0.4);
    display: flex; align-items: center; justify-content: center; z-index: 100;
    backdrop-filter: blur(4px);
  }
  .dialog { background: var(--bg-secondary); border-radius: 12px; padding: 24px; max-width: 400px; }
  h3 { font-size: 16px; margin-bottom: 8px; color: var(--text-primary); }
  p { font-size: 14px; color: var(--text-secondary); margin-bottom: 20px; }
  .actions { display: flex; gap: 12px; justify-content: flex-end; }
  .btn {
    padding: 8px 20px; border: 1px solid var(--border); border-radius: 8px;
    background: var(--bg-secondary); color: var(--text-primary); cursor: pointer; font-size: 14px;
  }
  .btn.danger { background: #ff3b30; color: white; border-color: #ff3b30; }
</style>
