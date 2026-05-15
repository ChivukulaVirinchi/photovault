<script lang="ts">
  import { onMount } from "svelte";
  import { albums } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { jobs } from "../lib/stores/jobs.svelte";
  import { browseContext } from "../lib/stores/browseContext.svelte";
  import { toasts } from "../lib/stores/toast.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import { thumbnailOnVisible } from "../lib/thumbnailRequest";
  import PageHeader from "../lib/components/PageHeader.svelte";
  import type { AlbumDto, AlbumSuggestionDto } from "../lib/api/types";
  import type { PhotoSummaryDto } from "../lib/api/types";

  let list = $state<AlbumDto[]>([]);
  let suggestions = $state<AlbumSuggestionDto[]>([]);
  let creating = $state(false);
  let newName = $state("");
  let error = $state<string | null>(null);
  // Detection state derived from the global jobs store so it survives
  // page navigation. The store gets fed by `album_suggestions:progress`
  // / `album_suggestions:complete` events emitted by the Rust shell.
  const detecting = $derived(jobs.isRunning("albumSuggestions"));
  const detectJob = $derived(jobs.byKind("albumSuggestions"));

  // Live filter — the text field appears once the user has enough
  // albums to actually need scanning (8+).
  let filter = $state("");
  const visibleList = $derived.by(() => {
    if (!filter.trim()) return list;
    const q = filter.trim().toLowerCase();
    return list.filter((a) => a.name.toLowerCase().includes(q));
  });

  // Preview modal — opened by clicking a suggestion card. Holds the
  // currently-previewed suggestion + its first ~12 photos.
  let previewSugg = $state<AlbumSuggestionDto | null>(null);
  let previewPhotos = $state<PhotoSummaryDto[]>([]);
  let previewLoading = $state(false);
  let previewActing = $state(false);

  async function load() {
    try {
      list = await albums.list();
      suggestions = await albums.suggestions.list();
    } catch (e) { error = JSON.stringify(e); }
  }

  async function createAlbum() {
    if (!newName.trim()) return;
    try {
      await albums.create(newName.trim());
      newName = "";
      creating = false;
      await load();
    } catch (e) { error = JSON.stringify(e); }
  }

  function onCreateKey(e: KeyboardEvent) {
    if (e.key === "Enter") { e.preventDefault(); createAlbum(); }
    else if (e.key === "Escape") {
      e.preventDefault();
      newName = "";
      creating = false;
    }
  }

  async function runDetection() {
    if (detecting) return;
    // Optimistic placeholder so the indicator pops on the click
    // frame, before the IPC even returns.
    const placeholderId = `pending-album-${Date.now()}`;
    jobs.register(placeholderId, "albumSuggestions");
    toasts.success("Looking for trip and event patterns…");
    try {
      const r = await albums.suggestions.runDetection();
      jobs.dismiss(placeholderId);
      jobs.register(r.job_id, "albumSuggestions");
    } catch (e) {
      jobs.dismiss(placeholderId);
      const msg = typeof e === "string" ? e : JSON.stringify(e);
      error = msg;
      toasts.error(`Couldn't detect: ${msg}`);
    }
  }

  async function openPreview(s: AlbumSuggestionDto) {
    previewSugg = s;
    previewPhotos = [];
    previewLoading = true;
    try {
      // Load every photo in the suggestion — the modal scrolls so the
      // user can scan the whole set before accepting. Cap matches the
      // backend preview limit; suggestions almost never exceed 200.
      previewPhotos = await albums.suggestions.preview(s.id, Math.max(s.photo_ids.length, 12));
    } catch (e) { error = JSON.stringify(e); }
    finally { previewLoading = false; }
  }
  function closePreview() {
    previewSugg = null;
    previewPhotos = [];
  }

  function patchPreviewThumbnail(photoId: number, thumbnailPath: string) {
    previewPhotos = previewPhotos.map((p) => (
      p.id === photoId ? { ...p, thumbnail_path: thumbnailPath } : p
    ));
  }

  async function acceptSuggestion(id: number) {
    previewActing = true;
    try { await albums.suggestions.accept(id); closePreview(); await load(); }
    catch (e) { error = JSON.stringify(e); }
    finally { previewActing = false; }
  }

  async function dismissSuggestion(id: number) {
    previewActing = true;
    try { await albums.suggestions.dismiss(id); closePreview(); await load(); }
    catch (e) { error = JSON.stringify(e); }
    finally { previewActing = false; }
  }

  function onPreviewKey(e: KeyboardEvent) {
    if (!previewSugg) return;
    if (e.key === "Escape") { e.preventDefault(); closePreview(); }
  }

  // React to album-suggestion completion via the global jobs store.
  // Doing it here (instead of via a one-shot Tauri `listen()`) means
  // the toast still fires if the user clicks Detect, navigates away,
  // and comes back — the store is filled by the app-boot subscription.
  let toastedJobIds = new Set<string>();
  $effect(() => {
    if (!detectJob) return;
    if (detectJob.status === "complete" && !toastedJobIds.has(detectJob.id)) {
      toastedJobIds.add(detectJob.id);
      load();
      const msg = detectJob.message || "Suggestion detection finished.";
      if (msg.toLowerCase().startsWith("couldn't")) {
        toasts.error(msg);
      } else {
        toasts.success(msg);
      }
    }
  });

  onMount(() => {
    load();
    window.addEventListener("keydown", onPreviewKey);
    return () => {
      window.removeEventListener("keydown", onPreviewKey);
    };
  });
</script>

<PageHeader title="Albums">
  <span class="count mono">{list.length}<span class="muted"> albums</span></span>
  <button class="ghost" onclick={runDetection} disabled={detecting}>
    {detecting ? "Detecting…" : "Detect"}
  </button>
  {#if creating}
    <!-- svelte-ignore a11y_autofocus -->
    <input bind:value={newName} onkeydown={onCreateKey} placeholder="Album name" autofocus />
    <button class="primary" onclick={createAlbum}>Create</button>
    <button class="ghost" onclick={() => (creating = false)}>Cancel</button>
  {:else}
    <button class="primary" onclick={() => (creating = true)}>New album</button>
  {/if}
</PageHeader>

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

<div class="page">
  {#if suggestions.length > 0}
    <section class="suggestions">
      <div class="section-head">
        <h3 class="section-title">Suggestions</h3>
        <span class="hint">Patterns we noticed — accept to save, dismiss to ignore.</span>
      </div>
      <div class="suggest-grid">
        {#each suggestions as s (s.id)}
          <button class="suggestion" onclick={() => openPreview(s)} aria-label="Preview suggestion {s.title}">
            {#if s.cover_thumbnail_path}
              <img src={thumbUrl(libraryStore.driveRoot, s.cover_thumbnail_path) ?? ""} alt="" />
            {/if}
            <div class="body">
              <span class="kind mono">{s.kind}</span>
              <strong class="title">{s.title}</strong>
              <span class="muted small">{s.photo_ids.length} photos · click to preview</span>
            </div>
          </button>
        {/each}
      </div>
    </section>
    {#if list.length > 0}<hr class="hairline" />{/if}
  {/if}

  {#if list.length === 0 && suggestions.length === 0}
    <div class="empty">
      <p>Make one, or run detection to find what already groups itself.</p>
      <div class="row">
        <button class="primary" onclick={() => (creating = true)}>New album</button>
        <button class="ghost" onclick={runDetection} disabled={detecting}>
    {detecting ? "Detecting…" : "Detect"}
  </button>
      </div>
    </div>
  {/if}

  {#if previewSugg}
    {@const s = previewSugg}
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div class="modal-scrim" role="presentation" onclick={closePreview}>
      <div class="modal" role="dialog" aria-modal="true" tabindex="-1" onclick={(e) => e.stopPropagation()}>
        <header>
          <span class="kind mono">{s.kind}</span>
          <strong class="title">{s.title}</strong>
          <span class="muted small">{s.photo_ids.length} photos</span>
        </header>
        <!--
          Album-modal preview uses the padding-top:100% aspect-ratio
          pattern (a child span absolutely-fills the cell). This is
          the bulletproof version — `aspect-ratio: 1` on grid items
          inside a nested flex container kept fighting computed
          heights and producing overlap. The hack predates aspect-
          ratio in CSS but works reliably across every browser.
        -->
        <div class="preview-grid">
          {#if previewLoading}
            {#each Array(12) as _}
              <span class="m-cell loading-ph">
                <span class="m-pad"></span>
              </span>
            {/each}
          {:else}
            {#each previewPhotos as p (p.id)}
              <a
                class="m-cell"
                href="#/photo?id={p.id}"
                use:thumbnailOnVisible={{
                  id: p.id,
                  thumbnailPath: p.thumbnail_path,
                  onReady: (path) => patchPreviewThumbnail(p.id, path),
                }}
                onclick={() =>
                  browseContext.set(
                    `suggestion:${s.id}`,
                    previewPhotos.map((q) => q.id),
                  )}
              >
                <span class="m-pad">
                  {#if p.thumbnail_path}
                    <img src={thumbUrl(libraryStore.driveRoot, p.thumbnail_path) ?? ""} alt="" loading="lazy" />
                  {/if}
                </span>
              </a>
            {/each}
          {/if}
        </div>
        <footer>
          <button class="ghost" onclick={() => dismissSuggestion(s.id)} disabled={previewActing}>
            Dismiss
          </button>
          <button class="ghost" onclick={closePreview} disabled={previewActing}>Cancel</button>
          <button class="primary" onclick={() => acceptSuggestion(s.id)} disabled={previewActing}>
            Accept
          </button>
        </footer>
      </div>
    </div>
  {/if}

  {#if list.length > 0}
    {#if list.length >= 8}
      <div class="filter-row">
        <input
          type="search"
          placeholder="Filter albums…"
          bind:value={filter}
          aria-label="Filter albums"
        />
        {#if filter}
          <button class="clear" onclick={() => (filter = "")} aria-label="Clear filter">×</button>
        {/if}
      </div>
    {/if}
    <div class="grid">
      {#each visibleList as a (a.id)}
        <a class="card" href="#/album?id={a.id}">
          <div class="cover">
            {#if a.cover_thumbnail_path}
              <img src={thumbUrl(libraryStore.driveRoot, a.cover_thumbnail_path) ?? ""} alt="" />
            {:else}
              <span class="placeholder small">empty</span>
            {/if}
            <span class="meta-chip mono">{a.photo_count}</span>
          </div>
          <strong class="title">{a.name}</strong>
          {#if a.date_range_start && a.date_range_end}
            <span class="dates mono small">
              {new Date(a.date_range_start).toLocaleDateString()}
              — {new Date(a.date_range_end).toLocaleDateString()}
            </span>
          {/if}
        </a>
      {/each}
    </div>
  {/if}
</div>

<style>
  .page { padding: var(--s-5) var(--s-7) var(--s-7); flex: 1; overflow-y: auto; }
  .count { font-size: var(--t-sm); color: var(--ink); }

  .empty {
    padding: var(--s-8) var(--s-5);
    text-align: center;
    display: flex;
    flex-direction: column;
    gap: var(--s-4);
    align-items: center;
    max-width: 42ch;
    margin: 0 auto;
  }
  .empty p { color: var(--ink-soft); line-height: 1.55; }
  .empty .row { display: flex; gap: var(--s-2); }

  .suggestions { margin-bottom: var(--s-4); }
  .section-head {
    display: flex;
    align-items: baseline;
    gap: var(--s-3);
    margin-bottom: var(--s-3);
  }
  .section-title {
    font-size: var(--t-sm);
    font-weight: 600;
    color: var(--ink);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    margin: 0;
  }
  .hint {
    font-size: var(--t-sm);
    color: var(--ink-muted);
    font-style: italic;
  }

  .suggest-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: var(--s-3);
  }
  .suggestion {
    background: var(--bg-card);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    text-align: left;
    color: inherit;
    cursor: pointer;
    padding: 0;
    transition: border-color var(--t-fast) var(--ease),
                box-shadow var(--t-fast) var(--ease),
                transform var(--t-fast) var(--ease);
  }
  .suggestion:hover {
    border-color: var(--accent);
    transform: translateY(-1px);
    box-shadow: 0 6px 18px color-mix(in oklab, var(--accent) 18%, transparent);
  }
  .suggestion img {
    width: 100%;
    aspect-ratio: 16 / 9;
    object-fit: cover;
    display: block;
  }
  .suggestion .body {
    padding: var(--s-3) var(--s-4) var(--s-4);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .suggestion .kind {
    font-size: var(--t-xs);
    color: var(--accent);
    text-transform: uppercase;
    letter-spacing: 0.1em;
    font-weight: 600;
  }
  .suggestion .title {
    font-size: var(--t-base);
    font-weight: 600;
  }

  /* ----- preview modal ----- */
  .modal-scrim {
    position: fixed;
    inset: 0;
    background: color-mix(in oklab, #000 60%, transparent);
    backdrop-filter: blur(2px);
    z-index: 80;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--s-6);
  }
  .modal {
    background: var(--bg-paper);
    border: 1px solid var(--line);
    border-radius: var(--r-lg, 12px);
    box-shadow: 0 28px 60px rgba(0,0,0,0.55);
    max-width: 760px;
    width: 100%;
    max-height: 88vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .modal header {
    padding: var(--s-5) var(--s-6) var(--s-3);
    display: flex;
    flex-direction: column;
    gap: 4px;
    border-bottom: 1px solid var(--line-soft);
  }
  .modal header .kind {
    font-size: var(--t-xs);
    color: var(--accent);
    text-transform: uppercase;
    letter-spacing: 0.1em;
    font-weight: 600;
  }
  .modal header .title {
    font-family: var(--font-display);
    font-size: var(--t-2xl);
    font-weight: 600;
    color: var(--ink);
    line-height: 1.1;
  }
  /*
    Bulletproof grid for the suggestion preview modal. `aspect-ratio:
    1` on grid items inside this nested flex layout kept producing
    overlap on resize / scroll. The padding-top:100% hack guarantees
    a 1:1 cell at any width, in any container. Don't migrate this
    back to `.pv-photo-grid` without re-testing on small/large modals.
  */
  .preview-grid {
    padding: var(--s-4) var(--s-6);
    overflow-y: auto;
    flex: 1 1 auto;
    min-height: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    gap: 6px;
    align-content: start;
  }
  .m-cell {
    display: block;
    background: var(--bg-elev);
    border-radius: var(--r-sm);
    overflow: hidden;
    text-decoration: none;
    color: inherit;
    transition: filter var(--t-fast) var(--ease),
                box-shadow var(--t-fast) var(--ease);
  }
  .m-cell:hover {
    filter: brightness(1.06);
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  .m-pad {
    display: block;
    position: relative;
    padding-top: 100%; /* the square */
  }
  .m-cell img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .preview-grid .loading-ph {
    /* Skeleton slot while preview load is in flight. */
    background: var(--bg-elev);
  }
  .modal footer {
    flex-shrink: 0;
    padding: var(--s-3) var(--s-6) var(--s-5);
    border-top: 1px solid var(--line-soft);
    background: var(--bg-paper);
    display: flex;
    gap: var(--s-2);
    justify-content: flex-end;
  }

  .filter-row {
    position: relative;
    margin-bottom: var(--s-4);
    max-width: 360px;
  }
  .filter-row input {
    width: 100%;
    padding: 8px 32px 8px 12px;
    background: var(--bg-paper);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    color: var(--ink);
    font-size: var(--t-sm);
  }
  .filter-row input:focus {
    outline: none;
    border-color: var(--accent);
  }
  .filter-row .clear {
    position: absolute;
    right: 6px;
    top: 50%;
    transform: translateY(-50%);
    background: transparent;
    border: 0;
    color: var(--ink-muted);
    cursor: pointer;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    line-height: 1;
    font-size: 16px;
  }
  .filter-row .clear:hover { background: var(--bg-card); color: var(--ink); }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: var(--s-5) var(--s-4);
  }
  .card {
    color: inherit;
    display: flex;
    flex-direction: column;
    gap: 6px;
    text-decoration: none;
  }
  .cover {
    aspect-ratio: 4 / 5;
    min-width: 0;
    background: var(--bg-card);
    border-radius: var(--r-md);
    overflow: hidden;
    position: relative;
    border: 1px solid var(--line);
    display: flex;
    align-items: center;
    justify-content: center;
    transition: border-color var(--t-fast) var(--ease),
                box-shadow var(--t-fast) var(--ease);
  }
  .card:hover .cover {
    border-color: var(--accent);
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .cover img { width: 100%; height: 100%; object-fit: cover; }
  .placeholder { color: var(--ink-faint); }
  .meta-chip {
    position: absolute;
    bottom: var(--s-2);
    right: var(--s-2);
    background: color-mix(in oklab, var(--bg) 72%, transparent);
    backdrop-filter: blur(6px);
    padding: 3px 9px;
    border-radius: 999px;
    font-size: var(--t-xs);
    color: var(--ink);
    border: 1px solid var(--line);
  }
  .title {
    font-size: var(--t-base);
    font-weight: 600;
    color: var(--ink);
    margin-top: 4px;
  }
  .dates {
    color: var(--ink-faint);
  }
  .small { font-size: var(--t-xs); }
</style>
