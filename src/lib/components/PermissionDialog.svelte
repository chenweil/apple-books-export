<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  let { onRetry = () => {} } = $props();
  let checking = $state(false);

  async function handleRetry() {
    checking = true;
    const ok: boolean = await invoke("check_db_access");
    checking = false;
    if (ok) onRetry();
  }

  function openSettings() {
    invoke("open_system_settings");
  }
</script>

<div class="overlay">
  <div class="dialog">
    <h2>需要磁盘访问权限</h2>
    <p>Apple Books Exporter 需要访问 Apple Books 数据库。</p>

    <div class="steps">
      <ol>
        <li>点击下方按钮打开<strong>系统设置 → 隐私与安全性 → 完全磁盘访问权限</strong></li>
        <li>将 <strong>Apple Books Exporter</strong> 添加到列表并启用</li>
        <li>返回此窗口点击<strong>重新检测</strong></li>
      </ol>
    </div>

    <div class="path">
      <code>~/Library/Containers/com.apple.iBooksX/Data/Documents/</code>
    </div>

    <div class="actions">
      <button class="btn primary" onclick={openSettings}>打开系统设置</button>
      <button class="btn" onclick={handleRetry} disabled={checking}>
        {checking ? "检测中..." : "重新检测"}
      </button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0,0,0,0.4);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
    backdrop-filter: blur(8px);
  }
  .dialog {
    background: var(--bg-secondary);
    border-radius: 12px;
    padding: 32px;
    max-width: 480px;
    box-shadow: 0 20px 60px rgba(0,0,0,0.3);
  }
  h2 { font-size: 20px; margin-bottom: 8px; color: var(--text-primary); }
  p { color: var(--text-secondary); margin-bottom: 16px; font-size: 14px; }
  .steps ol { padding-left: 20px; font-size: 14px; color: var(--text-primary); }
  .steps li { margin-bottom: 8px; line-height: 1.5; }
  .path {
    background: var(--bg-primary);
    padding: 8px 12px;
    border-radius: 6px;
    margin: 16px 0;
    font-size: 12px;
  }
  .actions { display: flex; gap: 12px; justify-content: flex-end; }
  .btn {
    padding: 8px 20px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-secondary);
    color: var(--text-primary);
    cursor: pointer;
    font-size: 14px;
  }
  .btn.primary {
    background: var(--accent);
    color: white;
    border-color: var(--accent);
  }
  .btn:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
