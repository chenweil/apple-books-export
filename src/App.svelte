<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Sidebar from "./lib/Sidebar.svelte";
  import PermissionDialog from "./lib/components/PermissionDialog.svelte";
  import Books from "./lib/pages/Books.svelte";

  let currentPage = $state("books");
  let dbOk = $state(false);
  let checking = $state(true);

  async function checkAccess() {
    checking = true;
    dbOk = await invoke("check_db_access");
    checking = false;
  }

  $effect(() => { checkAccess(); });
</script>

<main class="app">
  <Sidebar bind:currentPage />
  <div class="content">
    {#if checking}
      <div class="center">检测数据库...</div>
    {:else if !dbOk}
      <PermissionDialog onRetry={checkAccess} />
    {:else}
      {#if currentPage === "books"}
        <Books />
      {:else}
        <div class="placeholder">
          <h2>{currentPage}</h2>
          <p>待实现</p>
        </div>
      {/if}
    {/if}
  </div>
</main>

<style>
  .app { display: flex; height: 100vh; }
  .content { flex: 1; overflow-y: auto; background: var(--bg-primary); }
  .center { display: flex; align-items: center; justify-content: center; height: 100%; color: var(--text-secondary); }
  .placeholder { display: flex; flex-direction: column; align-items: center; justify-content: center; height: 100%; color: var(--text-secondary); }
</style>
