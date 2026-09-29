<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  interface ApiConfig { name: string; base_url: string; api_key: string; model: string; }

  // API 配置列表
  let apiConfigs = $state<ApiConfig[]>([{ name: "默认", base_url: "", api_key: "", model: "" }]);

  // 输出配置
  let outputFormat = $state("obsidian");
  let outputDir = $state("");

  // 卡片生成配置
  let enrichPrompt = $state("");
  let enrichApi = $state("");

  // 状态
  let saving = $state(false);
  let testing = $state(false);
  let testResult = $state("");
  let saved = $state(false);

  const defaultPrompt = `你是一个阅读助手，帮助读者理解书籍内容。请对以下摘录进行分析，提供：
1. 解释：用通俗易懂的语言解释这段内容的含义
2. 标签：3-5 个关键词标签，用逗号分隔
3. 复习问题：一个能帮助读者回顾和理解的问题

请按照以下 JSON 格式返回结果：
{
  "explanation": "解释内容",
  "tags": ["标签1", "标签2", "标签3"],
  "question": "复习问题"
}

摘录内容：
> {highlight}`;

  $effect(() => {
    invoke<any>("load_app_config").then(c => {
      // 加载 API 配置
      if (c.api_configs && c.api_configs.length > 0) {
        apiConfigs = c.api_configs;
      } else {
        // 兼容旧配置：从 llm 迁移
        apiConfigs = [{
          name: "默认",
          base_url: c.llm?.base_url || "",
          api_key: c.llm?.api_key || "",
          model: c.llm?.model || "",
        }];
      }
      outputFormat = c.output_format || "obsidian";
      outputDir = c.card_output || "";
      enrichPrompt = c.card_gen?.enrich_prompt || defaultPrompt;
      enrichApi = c.card_gen?.enrich_api || "";
    });
  });

  function addApi() {
    apiConfigs = [...apiConfigs, { name: `API ${apiConfigs.length + 1}`, base_url: "", api_key: "", model: "" }];
  }

  function removeApi(idx: number) {
    if (apiConfigs.length <= 1) return;
    apiConfigs = apiConfigs.filter((_, i) => i !== idx);
  }

  async function save() {
    saving = true;
    testResult = "";
    try {
      // 兼容：把第一个 API 配置也写入 llm 字段
      const firstApi = apiConfigs[0] || { base_url: "", api_key: "", model: "" };
      await invoke("save_app_config", {
        config: {
          llm: { provider: "openai_compatible", base_url: firstApi.base_url, api_key: firstApi.api_key, model: firstApi.model, batch_size: 5, max_retries: 3, retry_delays: [1000, 3000, 5000] },
          api_configs: apiConfigs,
          card_gen: {
            enrich_prompt: enrichPrompt,
            enrich_api: enrichApi,
          },
          output_format: outputFormat,
          card_style: "dark",
          card_output: outputDir,
        }
      });
      saved = true;
      testResult = "保存成功";
      setTimeout(() => { saved = false; testResult = ""; }, 2000);
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

  function resetPrompt() {
    enrichPrompt = defaultPrompt;
  }
</script>

<div class="page">
  <h2>设置</h2>

  <section>
    <h3>API 配置</h3>
    {#each apiConfigs as api, idx}
      <div class="api-card">
        <div class="api-header">
          <input type="text" bind:value={api.name} placeholder="配置名称" class="api-name" />
          {#if apiConfigs.length > 1}
            <button class="btn-remove" onclick={() => removeApi(idx)}>删除</button>
          {/if}
        </div>
        <div class="form">
          <label>
            Base URL
            <input type="text" bind:value={api.base_url} placeholder="https://api.example.com/v1" />
          </label>
          <label>
            API Key
            <input type="password" bind:value={api.api_key} placeholder="sk-..." />
          </label>
          <label>
            Model
            <input type="text" bind:value={api.model} placeholder="mimo-v2.5-pro" />
          </label>
        </div>
      </div>
    {/each}
    <button class="btn-add" onclick={addApi}>+ 添加 API 配置</button>
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

  <section>
    <h3>卡片生成配置</h3>
    <div class="form">
      <label>
        AI 增强 API
        <select bind:value={enrichApi}>
          <option value="">使用第一个 API</option>
          {#each apiConfigs as api}
            <option value={api.name}>{api.name} ({api.model || '未设置'})</option>
          {/each}
        </select>
      </label>
      <label>
        <div class="label-header">
          AI 增强 Prompt
          <button class="btn-link" onclick={resetPrompt}>恢复默认</button>
        </div>
        <textarea bind:value={enrichPrompt} rows="12" placeholder="输入 prompt 模板..."></textarea>
        <span class="hint">使用 {`{highlight}`} 代表高亮内容</span>
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
  textarea {
    padding: 8px 12px; border: 1px solid var(--border); border-radius: 8px;
    background: var(--bg-secondary); color: var(--text-primary); font-size: 13px;
    font-family: "SF Mono", "Menlo", monospace; resize: vertical; line-height: 1.5;
  }
  .hint { font-size: 12px; color: var(--text-secondary); margin-top: 2px; }
  .label-header { display: flex; justify-content: space-between; align-items: center; }
  .btn-link { background: none; border: none; color: var(--accent); cursor: pointer; font-size: 12px; padding: 0; }
  .btn-link:hover { text-decoration: underline; }
  .api-card { background: var(--bg-secondary); border: 1px solid var(--border); border-radius: 8px; padding: 16px; margin-bottom: 12px; }
  .api-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px; }
  .api-name { max-width: 200px; font-weight: 600; }
  .btn-remove { padding: 4px 12px; background: #ff3b30; color: white; border: none; border-radius: 6px; cursor: pointer; font-size: 12px; }
  .btn-remove:hover { background: #d32f2f; }
  .btn-add { padding: 8px 16px; background: var(--bg-secondary); border: 1px dashed var(--border); border-radius: 8px; cursor: pointer; font-size: 14px; color: var(--text-secondary); width: 100%; }
  .btn-add:hover { border-color: var(--accent); color: var(--accent); }
</style>
