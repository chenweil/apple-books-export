<script lang="ts">
  let { size = 24, message = "" } = $props();
</script>

<div class="spinner-container">
  <div class="spinner" style="width: {size}px; height: {size}px;"></div>
  {#if message}
    <p class="message">{message}</p>
  {/if}
</div>

<style>
  .spinner-container {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 24px;
  }
  .spinner {
    border: 3px solid var(--border);
    border-top: 3px solid var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to { transform: rotate(360deg); }
  }
  .message { color: var(--text-secondary); font-size: 14px; }
</style>
