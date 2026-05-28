<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Sidebar from "./lib/Sidebar.svelte";
  import StatusBar from "./lib/StatusBar.svelte";
  import PermissionDialog from "./lib/components/PermissionDialog.svelte";
  import Books from "./lib/pages/Books.svelte";
  import Export from "./lib/pages/Export.svelte";
  import Config from "./lib/pages/Config.svelte";
  import Enrich from "./lib/pages/Enrich.svelte";
  import Card from "./lib/pages/Card.svelte";
  import Cache from "./lib/pages/Cache.svelte";

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
  <div class="main-area">
    <div class="content">
      {#if checking}
        <div class="center">检测数据库...</div>
      {:else if !dbOk}
        <PermissionDialog onRetry={checkAccess} />
      {:else}
        {#if currentPage === "books"}
          <Books />
        {:else if currentPage === "export"}
          <Export />
        {:else if currentPage === "enrich"}
          <Enrich />
        {:else if currentPage === "card"}
          <Card />
        {:else if currentPage === "cache"}
          <Cache />
        {:else if currentPage === "config"}
          <Config />
        {/if}
      {/if}
    </div>
    <StatusBar />
  </div>
</main>

<style>
  .app { display: flex; height: 100vh; }
  .main-area { flex: 1; display: flex; flex-direction: column; overflow: hidden; }
  .content { flex: 1; overflow-y: auto; background: var(--bg-primary); }
  .center { display: flex; align-items: center; justify-content: center; height: 100%; color: var(--text-secondary); }
</style>
