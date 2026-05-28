<script lang="ts">
  import Sidebar from "./lib/Sidebar.svelte";

  let currentPage = $state("books");
</script>

<main class="app">
  <Sidebar bind:currentPage />
  <div class="content">
    <h1>Apple Books Exporter</h1>
    <p>页面: {currentPage}</p>
  </div>
</main>

<style>
  .app {
    display: flex;
    height: 100vh;
  }
  .content {
    flex: 1;
    padding: 20px;
    overflow-y: auto;
  }
</style>
