<script module lang="ts">
  import type { SearchResults as CachedSearchResults } from "../lib/api/all";

  let cachedSearch:
    | {
        driveRoot: string | null;
        q: string;
        results: CachedSearchResults | null;
        visiblePhotoLimit: number;
        scrollTop: number;
      }
    | null = null;
</script>

<script lang="ts">
  import { onMount } from "svelte";
  import { commandErrorMessage } from "../lib/api";
  import { search, trash } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { browseContext } from "../lib/stores/browseContext.svelte";
  import { photoVisibility } from "../lib/stores/photoVisibility.svelte";
  import { selection, handleCellClick } from "../lib/stores/selection.svelte";
  import { toasts } from "../lib/stores/toast.svelte";
  import { marqueeSelect } from "../lib/actions/marqueeSelect";
  import { thumbUrl } from "../lib/thumbnail";
  import { thumbnailOnVisible } from "../lib/thumbnailRequest";
  import PageHeader from "../lib/components/PageHeader.svelte";
  import SelectionBar from "../lib/components/SelectionBar.svelte";
  import AddToAlbumDialog from "../lib/components/AddToAlbumDialog.svelte";
  import { Check, X } from "lucide-svelte";
  import type { SearchResults } from "../lib/api/all";

  interface Props {
    /// Pre-fill the search bar from the URL (e.g. #/search?q=Goa).
    /// Lets Insights / Map / etc. deep-link into a filtered list.
    initialQuery?: string;
  }
  let { initialQuery = "" }: Props = $props();

  const currentDriveRoot = libraryStore.driveRoot;
  const currentSearchCache = cachedSearch?.driveRoot === currentDriveRoot ? cachedSearch : null;
  // svelte-ignore state_referenced_locally
  const useSearchCache = currentSearchCache != null && (!initialQuery || currentSearchCache.q === initialQuery);

  // svelte-ignore state_referenced_locally
  let q = $state(initialQuery || currentSearchCache?.q || "");
  let results = $state<SearchResults | null>(useSearchCache ? currentSearchCache?.results ?? null : null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let showAddDialog = $state(false);
  let debounceId: number | undefined;
  let inputEl: HTMLInputElement | undefined;
  let pageEl = $state<HTMLDivElement | undefined>(undefined);
  let actionBusy = $state(false);
  let runSeq = 0;
  let mounted = true;
  let lastInitialQuery = $state<string | null>(null);
  let visiblePhotoLimit = $state(useSearchCache ? currentSearchCache?.visiblePhotoLimit ?? 200 : 200);
  const visiblePhotos = $derived(results?.photos.slice(0, visiblePhotoLimit) ?? []);
  const visiblePhotoIds = $derived(visiblePhotos.map((p) => p.photo_id));
  const selectedResultIds = $derived(selection.listIn(results?.photo_ids ?? []));

  function saveSearchCache() {
    cachedSearch = {
      driveRoot: currentDriveRoot,
      q,
      results,
      visiblePhotoLimit,
      scrollTop: pageEl?.scrollTop ?? cachedSearch?.scrollTop ?? 0,
    };
  }

  async function run() {
    const seq = ++runSeq;
    const query = q.trim();
    error = null;
    showAddDialog = false;
    actionBusy = false;
    if (!query) {
      results = null;
      loading = false;
      selection.clear();
      saveSearchCache();
      return;
    }
    loading = true;
    try {
      const nextResults = await search.query(query);
      if (!mounted || seq !== runSeq) return;
      results = nextResults;
      visiblePhotoLimit = 200;
      if (results) browseContext.set(`search:${query}`, results.photo_ids);
      selection.clear();
      saveSearchCache();
    } catch (e) {
      if (mounted && seq === runSeq) {
        results = null;
        visiblePhotoLimit = 200;
        selection.clear();
        if (browseContext.source?.startsWith("search:")) browseContext.clear();
        error = commandErrorMessage(e);
        saveSearchCache();
      }
    }
    finally {
      if (mounted && seq === runSeq) loading = false;
    }
  }

  function onInput() {
    if (debounceId) window.clearTimeout(debounceId);
    debounceId = window.setTimeout(run, 250);
  }

  function clearSearch() {
    if (debounceId) window.clearTimeout(debounceId);
    debounceId = undefined;
    q = "";
    results = null;
    visiblePhotoLimit = 200;
    error = null;
    loading = false;
    selection.clear();
    if (browseContext.source?.startsWith("search:")) browseContext.clear();
    saveSearchCache();
    try {
      if (window.location.hash !== "#/search") {
        history.replaceState(history.state ?? {}, "", "#/search");
        window.dispatchEvent(new HashChangeEvent("hashchange"));
      }
    } catch {}
    inputEl?.focus();
  }

  function patchThumbnail(photoId: number, thumbnailPath: string) {
    if (!results) return;
    results = {
      ...results,
      photos: results.photos.map((p) => (
        p.photo_id === photoId ? { ...p, thumbnail_path: thumbnailPath } : p
      )),
    };
    saveSearchCache();
  }

  function patchAlbumCover(photoId: number, thumbnailPath: string) {
    if (!results) return;
    results = {
      ...results,
      albums: results.albums.map((a) => (
        a.cover_photo_id === photoId ? { ...a, cover_thumbnail_path: thumbnailPath } : a
      )),
    };
    saveSearchCache();
  }

  function onCellClick(e: MouseEvent, photoId: number) {
    const ids = results?.photo_ids ?? visiblePhotoIds;
    const handled = handleCellClick(e, photoId, ids);
    if (!handled) {
      browseContext.set(`search:${q.trim()}`, ids);
      saveSearchCache();
    }
  }

  function showMorePhotos() {
    visiblePhotoLimit += 200;
    saveSearchCache();
  }

  async function bulkTrash() {
    if (!results || actionBusy) return;
    const ids = selection.listIn(results.photo_ids);
    if (ids.length === 0) return;
    const seq = runSeq;
    const query = q.trim();
    const drop = new Set(ids);
    const snapshot = results.photos
      .map((photo, idx) => ({ photo, idx }))
      .filter((entry) => drop.has(entry.photo.photo_id));
    const idSnapshot = results.photo_ids
      .map((photoId, idx) => ({ photoId, idx }))
      .filter((entry) => drop.has(entry.photoId));
    try {
      actionBusy = true;
      const result = await trash.trashPhotos(ids);
      if (!mounted || seq !== runSeq || query !== q.trim() || !results) return;
      if (result.count === 0) {
        toasts.info("No selected photos needed trashing");
        return;
      }
      photoVisibility.markTrashed(ids);
      results = {
        ...results,
        photo_ids: results.photo_ids.filter((id) => !drop.has(id)),
        photos: results.photos.filter((p) => !drop.has(p.photo_id)),
      };
      saveSearchCache();
      browseContext.remove(ids);
      selection.clear();
      toasts.undoable(
        `${result.count} ${result.count === 1 ? "photo" : "photos"} moved to trash`,
        async () => {
          await trash.restore(ids);
          if (!mounted || query !== q.trim() || !results) return;
          photoVisibility.markRestored(ids);
          const nextPhotos = results.photos.slice();
          for (const entry of snapshot) {
            nextPhotos.splice(Math.min(entry.idx, nextPhotos.length), 0, entry.photo);
          }
          const nextPhotoIds = results.photo_ids.slice();
          for (const entry of idSnapshot) {
            nextPhotoIds.splice(Math.min(entry.idx, nextPhotoIds.length), 0, entry.photoId);
          }
          results = {
            ...results,
            photo_ids: nextPhotoIds,
            photos: nextPhotos,
          };
          saveSearchCache();
          browseContext.set(`search:${query}`, results.photo_ids);
        },
      );
    } catch (e) {
      if (mounted && seq === runSeq && query === q.trim()) {
        toasts.error(`Couldn't move to trash: ${commandErrorMessage(e)}`);
      }
    } finally {
      if (mounted && seq === runSeq && query === q.trim()) actionBusy = false;
    }
  }

  function onGlobalKey(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
    if (!selection.active()) return;
    if (e.key === "Escape") { selection.clear(); e.preventDefault(); }
    else if (e.key === "Delete" || e.key === "Backspace") { bulkTrash(); e.preventDefault(); }
    else if ((e.key === "a" || e.key === "A") && !e.metaKey && !e.ctrlKey) {
      showAddDialog = true; e.preventDefault();
    }
  }

  onMount(() => {
    mounted = true;
    if (results) {
      browseContext.set(`search:${q.trim()}`, results.photo_ids);
      requestAnimationFrame(() => {
        if (mounted && pageEl && currentSearchCache) pageEl.scrollTop = currentSearchCache.scrollTop;
      });
    } else {
      inputEl?.focus();
      if (q.trim()) run();
    }
    const el = pageEl;
    const onScroll = () => saveSearchCache();
    el?.addEventListener("scroll", onScroll, { passive: true });
    window.addEventListener("keydown", onGlobalKey);
    return () => {
      saveSearchCache();
      mounted = false;
      runSeq += 1;
      if (debounceId) window.clearTimeout(debounceId);
      el?.removeEventListener("scroll", onScroll);
      window.removeEventListener("keydown", onGlobalKey);
    };
  });

  $effect(() => {
    if (lastInitialQuery === null) {
      lastInitialQuery = initialQuery;
      return;
    }
    if (initialQuery === lastInitialQuery) return;
    lastInitialQuery = initialQuery;
    q = initialQuery;
    results = null;
    visiblePhotoLimit = 200;
    saveSearchCache();
    if (debounceId) window.clearTimeout(debounceId);
    void run();
  });
</script>

<PageHeader title="Search" />

<div class="search-row">
  <div class="bar">
    <svg class="lookup" width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
      <circle cx="6" cy="6" r="4" stroke="currentColor" stroke-width="1.4"/>
      <path d="M9.2 9.2L12 12" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/>
    </svg>
    <input
      bind:this={inputEl}
      bind:value={q}
      oninput={onInput}
      placeholder='Try a name, place, "Goa 2023", or "beach sunset"'
    />
    {#if loading}<span class="loading mono">…</span>{/if}
    {#if q}
      <button class="clear-search" type="button" onclick={clearSearch} aria-label="Clear search" title="Clear search">
        <X size={14} strokeWidth={2} />
      </button>
    {/if}
  </div>
</div>

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

<div class="page" bind:this={pageEl} use:marqueeSelect={{ getAllIds: () => visiblePhotoIds }}>
  {#if results}
    {#if results.interpreted.length > 0}
      <div class="chips interpreted" aria-label="Interpreted filters">
        {#each results.interpreted as f}
          <span class="chip" data-kind={f.kind}>{f.label}</span>
        {/each}
      </div>
    {/if}
    {#if results.people.length > 0}
      <section>
        <h3 class="section-title">People</h3>
        <ul class="row">
          {#each results.people as p}
            <li>
              <a href="#/person?id={p.cluster_id}">
                {#if p.face_thumbnail_path}
                  <img src={thumbUrl(libraryStore.driveRoot, p.face_thumbnail_path) ?? ""} alt="" />
                {:else}
                  <span class="placeholder"></span>
                {/if}
                <strong>{p.name}</strong>
                <span class="muted small mono">{p.photo_count}</span>
              </a>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
    {#if results.albums.length > 0}
      <section>
        <h3 class="section-title">Albums</h3>
        <ul class="row">
          {#each results.albums as a}
            <li>
              <a
                href="#/album?id={a.album_id}"
                use:thumbnailOnVisible={{
                  id: a.cover_photo_id ?? 0,
                  thumbnailPath: a.cover_thumbnail_path,
                  onReady: (path) => a.cover_photo_id != null && patchAlbumCover(a.cover_photo_id, path),
                }}
              >
                {#if a.cover_thumbnail_path}
                  <img src={thumbUrl(libraryStore.driveRoot, a.cover_thumbnail_path) ?? ""} alt="" />
                {:else}
                  <span class="placeholder"></span>
                {/if}
                <strong>{a.name}</strong>
                <span class="muted small mono">{a.photo_count}</span>
              </a>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
    {#if results.places.length > 0}
      <section>
        <h3 class="section-title">Places</h3>
        <ul class="places">
          {#each results.places as pl}
            <li>
              <a class="place-link" href={`#/search?q=${encodeURIComponent(pl.city ?? pl.country ?? "")}`}>
                <span class="city">{pl.city}</span>
                {#if pl.country}<span class="country">, {pl.country}</span>{/if}
                <span class="muted small mono">{pl.photo_count}</span>
              </a>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
    {#if results.photos.length > 0}
      <section>
        <div class="section-heading-row">
          <h3 class="section-title">
            Photos · {Math.min(visiblePhotoLimit, results.photos.length)} / {results.photos.length}
          </h3>
          {#if visiblePhotoLimit < results.photos.length}
            <button class="ghost small-action" onclick={showMorePhotos}>
              Show more
            </button>
          {/if}
        </div>
        <div class="pv-photo-grid">
          {#each visiblePhotos as p (p.photo_id)}
            <a
              class="pv-photo-cell"
              class:selected={selection.has(p.photo_id)}
              data-photo-id={p.photo_id}
              href="#/photo?id={p.photo_id}"
              title="#{p.photo_id}"
              onclick={(e) => onCellClick(e, p.photo_id)}
              use:thumbnailOnVisible={{
                id: p.photo_id,
                thumbnailPath: p.thumbnail_path,
                onReady: (path) => patchThumbnail(p.photo_id, path),
              }}
            >
              {#if p.thumbnail_path}
                <img
                  src={thumbUrl(libraryStore.driveRoot, p.thumbnail_path) ?? ""}
                  alt=""
                  loading="lazy"
                />
              {/if}
              {#if selection.has(p.photo_id)}
                <span class="check" aria-hidden="true">
                  <Check size={14} strokeWidth={2.5} />
                </span>
              {/if}
            </a>
          {/each}
        </div>
      </section>
    {/if}
    {#if results.people.length === 0 && results.albums.length === 0 && results.places.length === 0 && results.photos.length === 0}
      <div class="empty">
        <p>Nothing matches that yet.</p>
      </div>
    {/if}
  {:else if !loading}
    <div class="empty">
      <p>Type to search across people, dates, albums, places, favourites, filenames, camera names, and visual meaning.</p>
    </div>
  {/if}
</div>

{#if selectedResultIds.length > 0}
  <SelectionBar
    count={selectedResultIds.length}
    onAddToAlbum={() => (showAddDialog = true)}
    onTrash={bulkTrash}
    onCancel={() => selection.clear()}
  />
{/if}

{#if showAddDialog}
  <AddToAlbumDialog
    photoIds={selectedResultIds}
    onclose={() => (showAddDialog = false)}
    onsuccess={() => selection.clear()}
  />
{/if}

<style>
  .search-row { padding: var(--s-4) var(--s-7) 0; }
  .bar {
    display: flex;
    align-items: center;
    gap: var(--s-3);
    background: var(--bg-paper);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    padding: var(--s-2) var(--s-4);
    transition: border-color var(--t-fast) var(--ease),
                box-shadow var(--t-fast) var(--ease);
  }
  .bar:focus-within {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-ghost);
  }
  .lookup { color: var(--ink-muted); flex-shrink: 0; }
  .bar input {
    flex: 1;
    border: none;
    background: transparent;
    padding: 0;
    font-size: var(--t-lg);
    color: var(--ink);
  }
  .bar input:focus { outline: none; box-shadow: none; }
  .bar input::placeholder {
    color: var(--ink-faint);
  }
  .loading { color: var(--ink-muted); }
  .clear-search {
    width: 28px;
    height: 28px;
    padding: 0;
    border: none;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--ink-muted);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }
  .clear-search:hover {
    background: var(--bg-elev);
    color: var(--ink);
  }

  .page { padding: var(--s-5) var(--s-7); flex: 1; overflow-y: auto; }
  .pv-photo-cell .check {
    position: absolute;
    top: 6px;
    left: 6px;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--accent);
    color: var(--bg);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    box-shadow: 0 2px 6px rgba(0,0,0,0.4);
    pointer-events: none;
  }
  .interpreted {
    display: flex;
    flex-wrap: wrap;
    gap: var(--s-2);
    margin: 0 0 var(--s-4);
  }
  .interpreted .chip {
    display: inline-flex;
    align-items: center;
    min-height: 24px;
    padding: 3px 9px;
    border: 1px solid var(--line);
    border-radius: 999px;
    background: var(--bg-paper);
    color: var(--ink-soft);
    font-size: var(--t-xs);
  }
  section { margin-bottom: var(--s-6); }
  .section-heading-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--s-3);
    margin: 0 0 var(--s-3);
  }
  .section-title {
    font-size: var(--t-xs);
    font-weight: 600;
    color: var(--ink-muted);
    text-transform: uppercase;
    letter-spacing: 0.1em;
    margin: 0;
  }
  .small-action {
    min-height: 28px;
    padding: 4px 9px;
    font-size: var(--t-xs);
  }

  .row {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .row li {
    background: var(--bg-paper);
    border-radius: var(--r-md);
    border: 1px solid var(--line);
    transition: border-color var(--t-fast) var(--ease);
  }
  .row li:hover { border-color: var(--accent); }
  .row a {
    display: flex;
    align-items: center;
    gap: var(--s-3);
    padding: 6px var(--s-3) 6px 6px;
    color: inherit;
    text-decoration: none;
  }
  .row img, .placeholder {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    object-fit: cover;
    flex-shrink: 0;
  }
  .placeholder { background: var(--bg-elev); }

  .places {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .places li { padding: 0; }
  .place-link {
    display: flex;
    align-items: baseline;
    gap: var(--s-2);
    padding: var(--s-3) var(--s-4);
    background: var(--bg-paper);
    border-radius: var(--r-md);
    border: 1px solid var(--line);
    color: inherit;
    text-decoration: none;
    transition: border-color var(--t-fast) var(--ease),
                background var(--t-fast) var(--ease);
  }
  .place-link:hover {
    background: var(--bg-card);
    border-color: var(--accent);
  }
  .city { font-size: var(--t-base); font-weight: 600; }
  .country { color: var(--ink-muted); }
  .places .muted { margin-left: auto; }

  .empty {
    padding: var(--s-9) var(--s-5);
    text-align: center;
  }
  .empty p {
    color: var(--ink-muted);
    font-style: italic;
  }
  .small { font-size: var(--t-xs); }
</style>
