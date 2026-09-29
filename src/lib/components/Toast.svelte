<script lang="ts">
  import { onMount } from "svelte";

  let { message = "", type = "info", duration = 3000, onclose = () => {} } = $props();

  onMount(() => {
    const timer = setTimeout(onclose, duration);
    return () => clearTimeout(timer);
  });
</script>

<div class="toast {type}">
  <span>{message}</span>
  <button onclick={onclose}>&times;</button>
</div>

<style>
  .toast {
    position: fixed; top: 16px; right: 16px; z-index: 200;
    display: flex; align-items: center; gap: 12px;
    padding: 12px 16px; border-radius: 8px;
    font-size: 14px; box-shadow: 0 4px 12px rgba(0,0,0,0.15);
    animation: slideIn 0.3s ease;
  }
  .info { background: var(--toast-bg); color: var(--toast-text); }
  .error { background: #ff3b30; color: white; }
  .success { background: #34c759; color: white; }
  button { background: none; border: none; color: inherit; cursor: pointer; font-size: 18px; }
  @keyframes slideIn { from { transform: translateX(100%); opacity: 0; } to { transform: none; opacity: 1; } }
</style>
