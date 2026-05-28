<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  let provider = $state("openai_compatible");
  let baseUrl = $state("");
  let apiKey = $state("");
  let model = $state("");
  let outputFormat = $state("obsidian");
  let outputDir = $state("");
  let saving = $state(false);
  let testing = $state(false);
  let testResult = $state("");
  let saved = $state(false);

  $effect(() => {
    invoke<any>("load_app_config").then(c => {
      provider = c.llm?.provider || "openai_compatible";
      baseUrl = c.llm?.base_url || "";
      apiKey = c.llm?.api_key || "";
      model = c.llm?.model || "";
      outputFormat = c.output_format || "obsidian";
      outputDir = c.card_output || "";
    });
  });

  async function save() {
    saving = true;
    try {
      await invoke("save_app_config", {
        config: {
          llm: { provider, base_url: baseUrl, api_key: apiKey, model, batch_size: 5, max_retries: 3, retry_delays: [1000, 3000, 5000] },
          epub_mappings: {},
          output_format: outputFormat,
          card_style: "dark",
          card_output: outputDir,
          context_chars: 2000,
          filename_max_length: 50,
        }
      });
      saved = true;
      setTimeout(() => saved = false, 2000);
    } catch (e: any) {
      testResult = `保存失败: ${e}`;
    }
    saving = false;
  }

  async function testConnection() {
    testing = true;
    testResult = "";
    try {
      await save();
      const ok: boolean = await invoke("test_llm_connection");
      testResult = ok ? "连接成功" : "连接失败";
    } catch (e: any) {
      testResult = `${e}`;
    }
    testing = false;
  }
</script>

<div class="page">
  <h2>设置</h2>

  <section>
    <h3>LLM 配置</h3>
    <div class="form">
      <label>
        Provider
        <select bind:value={provider}>
          <option value="openai">OpenAI</option>
          <option value="openai_compatible">OpenAI Compatible</option>
          <option value="custom">自定义</option>
        </select>
      </label>
      <label>
        Base URL
        <input type="text" bind:value={baseUrl} placeholder="https://api.example.com/v1" />
      </label>
      <label>
        API Key
        <input type="password" bind:value={apiKey} placeholder="sk-..." />
      </label>
      <label>
        Model
        <input type="text" bind:value={model} placeholder="mimo-v2.5-pro" />
      </label>
    </div>
  </section>

  <section>
    <h3>输出配置</h3>
    <div class="form">
      <label>
        默认格式
        <select bind:value={outputFormat}>
          <option value="obsidian">Obsidian</option>
          <option value="markdown">Markdown</option>
        </select>
      </label>
      <label>
        输出目录
        <input type="text" bind:value={outputDir} placeholder="~/Documents/books/" />
      </label>
    </div>
  </section>

  <div class="actions">
    <button class="btn-primary" onclick={save} disabled={saving}>
      {saved ? "已保存" : saving ? "保存中..." : "保存配置"}
    </button>
    <button class="btn-secondary" onclick={testConnection} disabled={testing}>
      {testing ? "测试中..." : "测试连接"}
    </button>
  </div>

  {#if testResult}
    <p class="test-result">{testResult}</p>
  {/if}
</div>

<style>
  .page { padding: 24px; }
  h2 { font-size: 20px; margin-bottom: 20px; color: var(--text-primary); }
  h3 { font-size: 16px; margin-bottom: 12px; color: var(--text-primary); }
  section { margin-bottom: 24px; }
  .form { display: flex; flex-direction: column; gap: 12px; max-width: 480px; }
  label { display: flex; flex-direction: column; gap: 6px; font-size: 14px; color: var(--text-secondary); }
  select, input {
    padding: 8px 12px; border: 1px solid var(--border); border-radius: 8px;
    background: var(--bg-secondary); color: var(--text-primary); font-size: 14px;
  }
  .actions { display: flex; gap: 12px; margin-top: 8px; }
  .btn-primary {
    padding: 10px 24px; background: var(--accent); color: white; border: none;
    border-radius: 8px; cursor: pointer; font-size: 14px;
  }
  .btn-secondary {
    padding: 10px 24px; background: var(--bg-secondary); color: var(--text-primary);
    border: 1px solid var(--border); border-radius: 8px; cursor: pointer; font-size: 14px;
  }
  .btn-primary:disabled, .btn-secondary:disabled { opacity: 0.5; cursor: not-allowed; }
  .test-result { margin-top: 12px; font-size: 14px; color: var(--text-primary); }
</style>
