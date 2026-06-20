<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import SearchableSelect from "../components/SearchableSelect.svelte";
  import Toast from "../components/Toast.svelte";
  import { selectedBookIndex } from "../stores.svelte";

  interface Book {
    asset_id: string;
    title: string;
    author: string;
    note_count: number;
  }

  interface ChapterInfo {
    key: string;
    index: number;
    title: string;
    display_title: string;
    highlight_count: number;
    note_count: number;
    sample_highlights: string[];
  }

  const steps = [
    { id: "pre_reading", label: "读前 3 问" },
    { id: "recall_feedback", label: "复述补漏" },
    { id: "application_question", label: "应用问题" },
    { id: "chapter_card", label: "章节卡片" },
  ];

  let books = $state<Book[]>([]);
  let chapters = $state<ChapterInfo[]>([]);
  let selectedIndex = $state(1);
  let selectedChapterKey = $state("");
  let step = $state("pre_reading");
  let readingGoal = $state("");
  let userRecall = $state("");
  let userContext = $state("");
  let loadingChapters = $state(false);
  let processing = $state(false);
  let result = $state("");
  let toastMsg = $state("");

  $effect(() => {
    invoke<Book[]>("get_books")
      .then((items) => {
        books = items;
      })
      .catch((e) => {
        toastMsg = `加载书籍失败: ${e}`;
      });
  });

  // 从全局状态同步选中书籍
  $effect(() => {
    const idx = selectedBookIndex.value;
    if (idx !== null && idx > 0 && idx <= books.length) {
      selectedIndex = idx;
    }
  });

  $effect(() => {
    if (selectedIndex <= 0 || selectedIndex > books.length) return;

    loadingChapters = true;
    selectedChapterKey = "";
    invoke<ChapterInfo[]>("get_book_chapters", { bookIndex: selectedIndex })
      .then((items) => {
        chapters = items;
        selectedChapterKey = items[0]?.key ?? "";
      })
      .catch((e) => {
        toastMsg = `加载章节失败: ${e}`;
        chapters = [];
      })
      .finally(() => {
        loadingChapters = false;
      });
  });

  async function runCoach() {
    if (!selectedChapterKey) {
      toastMsg = "请先选择章节";
      return;
    }

    processing = true;
    result = "";
    try {
      result = await invoke("chapter_coach_cmd", {
        bookIndex: selectedIndex,
        chapterKey: selectedChapterKey,
        step,
        readingGoal: readingGoal.trim() || null,
        userRecall: userRecall.trim() || null,
        userContext: userContext.trim() || null,
      });
    } catch (e: any) {
      toastMsg = `陪练失败: ${e}`;
    } finally {
      processing = false;
    }
  }
</script>

<div class="page">
  <h2>章节陪练</h2>

  <div class="layout">
    <section class="form">
      <label>
        书籍
        <SearchableSelect
          items={books.map((b, i) => ({ label: `${b.title} — ${b.author}`, value: i + 1 }))}
          bind:value={selectedIndex}
          placeholder="搜索书名或作者..."
        />
      </label>

      <label>
        章节
        <select bind:value={selectedChapterKey} disabled={loadingChapters || chapters.length === 0}>
          {#if loadingChapters}
            <option value="">加载章节中...</option>
          {:else}
            {#each chapters as chapter}
              <option value={chapter.key}>
                {chapter.display_title}（{chapter.highlight_count} 条高亮）
              </option>
            {/each}
          {/if}
        </select>
      </label>

      <label>
        阅读目的
        <input
          type="text"
          bind:value={readingGoal}
          placeholder="例如：想把这一章用于改进编程学习"
        />
      </label>

      <div class="tabs">
        {#each steps as item}
          <button
            class:active={step === item.id}
            onclick={() => (step = item.id)}
            type="button"
          >
            {item.label}
          </button>
        {/each}
      </div>

      {#if step === "recall_feedback" || step === "chapter_card"}
        <label>
          你的复述 / 应用回答
          <textarea
            bind:value={userRecall}
            rows="8"
            placeholder="先不看书，用自己的话写下这一章讲了什么、你卡在哪里、你准备用在哪里。"
          ></textarea>
        </label>
      {/if}

      {#if step === "application_question" || step === "chapter_card"}
        <label>
          你的场景
          <input
            type="text"
            bind:value={userContext}
            placeholder="例如：工作、编程学习、产品设计、写作"
          />
        </label>
      {/if}

      <button class="btn-primary" onclick={runCoach} disabled={processing || !selectedChapterKey}>
        {processing ? "生成中..." : "运行陪练"}
      </button>
    </section>

    <section class="result-panel">
      <h3>输出</h3>
      {#if result}
        <pre>{result}</pre>
      {:else}
        <div class="empty">选择步骤后运行。读前阶段只生成问题；读后阶段需要先写自己的复述。</div>
      {/if}
    </section>
  </div>
</div>

{#if toastMsg}
  <Toast message={toastMsg} type="error" onclose={() => (toastMsg = "")} />
{/if}

<style>
  .page { padding: 24px; }
  h2 { font-size: 20px; margin-bottom: 20px; color: var(--text-primary); }
  h3 { font-size: 16px; margin-bottom: 12px; color: var(--text-primary); }
  .layout { display: grid; grid-template-columns: minmax(320px, 460px) minmax(360px, 1fr); gap: 24px; }
  .form { display: flex; flex-direction: column; gap: 16px; }
  label { display: flex; flex-direction: column; gap: 6px; font-size: 14px; color: var(--text-secondary); }
  input,
  select,
  textarea {
    padding: 8px 12px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-secondary);
    color: var(--text-primary);
    font-size: 14px;
    font-family: inherit;
  }
  textarea { resize: vertical; min-height: 132px; line-height: 1.5; }
  .tabs { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px; }
  .tabs button {
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-secondary);
    color: var(--text-primary);
    cursor: pointer;
    font-size: 13px;
  }
  .tabs button.active {
    border-color: var(--accent);
    background: rgba(0, 122, 255, 0.12);
  }
  .btn-primary {
    padding: 10px 24px;
    background: var(--accent);
    color: white;
    border: none;
    border-radius: 8px;
    cursor: pointer;
    font-size: 14px;
    font-weight: 500;
  }
  .btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }
  .result-panel {
    min-height: 520px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-secondary);
    padding: 16px;
  }
  pre {
    white-space: pre-wrap;
    word-break: break-word;
    color: var(--text-primary);
    font-family: "SF Mono", "Menlo", monospace;
    font-size: 13px;
    line-height: 1.7;
  }
  .empty { color: var(--text-secondary); font-size: 14px; line-height: 1.6; }

  @media (max-width: 900px) {
    .layout { grid-template-columns: 1fr; }
    .result-panel { min-height: 320px; }
  }
</style>
