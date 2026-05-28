<script lang="ts">
  let { value = 0, max = 100, showLabel = true } = $props();
  let percent = $derived(max > 0 ? Math.round((value / max) * 100) : 0);
</script>

<div class="progress-bar">
  <div class="track">
    <div class="fill" style="width: {percent}%"></div>
  </div>
  {#if showLabel}
    <span class="label">{percent}%</span>
  {/if}
</div>

<style>
  .progress-bar { display: flex; align-items: center; gap: 12px; }
  .track { flex: 1; height: 6px; background: var(--border); border-radius: 3px; overflow: hidden; }
  .fill { height: 100%; background: var(--accent); border-radius: 3px; transition: width 0.3s; }
  .label { font-size: 13px; color: var(--text-secondary); min-width: 40px; text-align: right; }
</style>
