<script lang="ts">
  interface Props {
    items: { label: string; value: number }[];
    value: number;
    placeholder?: string;
  }

  let { items, value = $bindable(), placeholder = "搜索..." }: Props = $props();

  let searchText = $state("");
  let open = $state(false);
  let inputEl: HTMLInputElement | null = $state(null);

  let displayText = $derived(
    items.find(item => item.value === value)?.label ?? ""
  );

  let filtered = $derived(
    searchText.trim()
      ? items.filter(item =>
          item.label.toLowerCase().includes(searchText.toLowerCase())
        )
      : items
  );

  function select(val: number) {
    value = val;
    searchText = "";
    open = false;
  }

  function onInput(e: Event) {
    searchText = (e.target as HTMLInputElement).value;
    open = true;
  }

  function onFocus() {
    searchText = "";
    open = true;
  }

  function onBlur() {
    setTimeout(() => { open = false; searchText = ""; }, 150);
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      open = false;
      searchText = "";
      inputEl?.blur();
    }
  }
</script>

<div class="searchable-select">
  <input
    bind:this={inputEl}
    type="text"
    value={open ? searchText : displayText}
    {placeholder}
    oninput={onInput}
    onfocus={onFocus}
    onblur={onBlur}
    onkeydown={onKeydown}
  />
  {#if open && filtered.length > 0}
    <ul class="dropdown">
      {#each filtered as item (item.value)}
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <li
          class:selected={item.value === value}
          onmousedown={() => select(item.value)}
        >
          {item.label}
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .searchable-select { position: relative; }
  input {
    width: 100%; padding: 8px 12px;
    border: 1px solid var(--border); border-radius: 8px;
    background: var(--bg-secondary); color: var(--text-primary);
    font-size: 14px; box-sizing: border-box;
  }
  input:focus { outline: none; border-color: var(--accent); }
  .dropdown {
    position: absolute; top: 100%; left: 0; right: 0;
    max-height: 240px; overflow-y: auto;
    background: var(--bg-secondary); border: 1px solid var(--border);
    border-radius: 8px; margin-top: 4px; padding: 4px 0;
    list-style: none; z-index: 100;
    box-shadow: 0 4px 12px rgba(0,0,0,0.15);
  }
  li {
    padding: 8px 12px; cursor: pointer; font-size: 14px;
    color: var(--text-primary);
  }
  li:hover { background: var(--bg-tertiary, rgba(128,128,128,0.1)); }
  li.selected { color: var(--accent); font-weight: 500; }
</style>
