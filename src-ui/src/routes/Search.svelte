<script module lang="ts">
  import type { SearchResults as CachedSearchResults } from "../lib/api/all";

  /**
   * Saved search answer. Keyed by library session + data revision +
   * query: a reopened library is a new session, and any data change
   * (trash/restore, scan, faces…) moves the revision, so a stale
   * answer is shown immediately but re-run in the background instead
   * of being trusted.
   */
  interface SearchCache {
    driveRoot: string | null;
    session: number;
    revision: number;
    q: string;
    results: CachedSearchResults | null;
  }
  let cachedSearch: SearchCache | null = null;

  /// Scroll positions, kept separately from result data (keyed by
  /// library session; the value belongs to the query last shown).
  const searchScrollTops = new Map<string, number>();
</script>

<script lang="ts">
  import { photoLabel } from "../lib/a11y";
  import { ignoreShortcut } from "../lib/keys";
  import { onMount, tick } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { commandErrorMessage } from "../lib/api";
  import { people, search, trash } from "../lib/api/all";
  import { countOf } from "../lib/format";
  import History from "lucide-svelte/icons/history";
  import type { PersonDto } from "../lib/api/types";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { dataRevision } from "../lib/stores/dataRevision.svelte";
  import { browseContext } from "../lib/stores/browseContext.svelte";
  import { photoVisibility } from "../lib/stores/photoVisibility.svelte";
  import { selection, handleCellClick } from "../lib/stores/selection.svelte";
  import { toasts } from "../lib/stores/toast.svelte";
  import { marqueeSelect } from "../lib/actions/marqueeSelect";
  import { thumbUrl } from "../lib/thumbnail";
  import { thumbnailOnVisible } from "../lib/thumbnailRequest";
  import { nextSearchPage, SEARCH_MAX_RESULTS } from "../lib/searchWindow";
  import { drainSearchPages } from "../lib/searchPaging";
  import VirtualGrid from "../lib/components/VirtualGrid.svelte";
  import PageHeader from "../lib/components/PageHeader.svelte";
  import AssetSetup from "../lib/components/AssetSetup.svelte";
  import SelectionBar from "../lib/components/SelectionBar.svelte";
  import AddToAlbumDialog from "../lib/components/AddToAlbumDialog.svelte";
  import Check from "lucide-svelte/icons/check";
  import X from "lucide-svelte/icons/x";
  import type { SearchResults } from "../lib/api/all";
  import { inInactivePanel } from "../lib/inactivePanel";

  interface Props {
    /// Pre-fill the search bar from the URL (e.g. #/search?q=Goa).
    /// Lets Insights / Map / etc. deep-link into a filtered list.
    initialQuery?: string;
  }
  let { initialQuery = "" }: Props = $props();

  const currentDriveRoot = libraryStore.driveRoot;
  const currentSession = libraryStore.session;
  const sessionKey = `${currentDriveRoot ?? "closed"}:${currentSession}`;
  const sameSession =
    cachedSearch &&
    cachedSearch.session === currentSession &&
    cachedSearch.driveRoot === currentDriveRoot;
  const currentSearchCache = sameSession ? cachedSearch : null;
  // svelte-ignore state_referenced_locally
  const useSearchCache = currentSearchCache != null && (!initialQuery || currentSearchCache.q === initialQuery);

  /// Drop ids we already know are trashed so a restored answer never
  /// shows them, even before the re-run lands.
  function dropKnownTrashed(r: SearchResults): SearchResults {
    const trashed = photoVisibility.trashedIds;
    if (trashed.size === 0) return r;
    const drop = new Set(r.photo_ids.filter((id) => trashed.has(id)));
    if (drop.size === 0) return r;
    return {
      ...r,
      photo_ids: r.photo_ids.filter((id) => !drop.has(id)),
      photos: r.photos.filter((p) => !drop.has(p.photo_id)),
    };
  }

  // svelte-ignore state_referenced_locally
  const restoredResults = useSearchCache && currentSearchCache?.results
    ? dropKnownTrashed(currentSearchCache.results)
    : null;

  // svelte-ignore state_referenced_locally
  let q = $state(initialQuery || currentSearchCache?.q || "");
  let results = $state<SearchResults | null>(restoredResults);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let showAddDialog = $state(false);
  let debounceId: number | undefined;
  let inputEl: HTMLInputElement | undefined;
  let pageEl = $state<HTMLDivElement | undefined>(undefined);
  let actionBusy = $state(false);
  let runSeq = 0;
  let pageSeq = 0;
  let mounted = true;
  let lastInitialQuery = $state<string | null>(null);
  // The query and data revision the on-screen results describe.
  let loadedQuery = $state<string | null>(
    // svelte-ignore state_referenced_locally
    restoredResults ? currentSearchCache?.q ?? null : null,
  );
  // svelte-ignore state_referenced_locally
  let loadedRevision = $state<number | null>(
    restoredResults ? currentSearchCache?.revision ?? null : null,
  );
  const MAX_SEARCH_RESULTS = SEARCH_MAX_RESULTS;
  let loadingMore = $state(false);
  const visiblePhotos = $derived(results?.photos ?? []);
  const visiblePhotoIds = $derived(visiblePhotos.map((p) => p.photo_id));
  const selectedResultIds = $derived(selection.listIn(results?.photo_ids ?? []));

  function saveSearchCache() {
    cachedSearch = {
      driveRoot: currentDriveRoot,
      session: currentSession,
      revision: loadedRevision ?? dataRevision.version,
      // Record the query the results actually answer, so restoring this
      // cache can never pair a query with another query's results.
      q: results ? (loadedQuery ?? q) : q,
      results,
    };
  }

  let composing = false;
  let pagingError = $state(false);

  // ---------- suggestions: @people, #things, recent searches ----------
  type Suggest =
    | { kind: "people"; start: number; end: number; typed: string; matches: PersonDto[] }
    | { kind: "thing" }
    | { kind: "recent"; items: string[] };
  let suggest = $state<Suggest | null>(null);
  let active = $state(0);
  let focused = $state(false);
  let recent = $state<string[]>([]);
  let named: PersonDto[] | null = null;

  /// Named people, most photographed first, fetched on the first `@`.
  async function namedPeople(): Promise<PersonDto[]> {
    if (!named) {
      const list = await people.list({ namedOnly: true });
      named = list.filter((p) => p.name).sort((a, b) => b.photo_count - a.photo_count);
    }
    return named;
  }

  /// What to offer for the text before the caret: people after `@`, a hint
  /// after `#`, recent searches in an empty box.
  async function updateSuggest() {
    const el = inputEl;
    if (!el || !focused) {
      suggest = null;
      return;
    }
    const caret = el.selectionStart ?? q.length;
    const before = q.slice(0, caret);
    const at = before.lastIndexOf("@");
    const hash = before.lastIndexOf("#");
    if (at >= 0 && at > hash && (at === 0 || /\s/.test(before[at - 1]))) {
      const typed = before.slice(at + 1).replace(/^"/, "");
      const want = typed.toLowerCase();
      const list = await namedPeople();
      if (inputEl !== el || (el.selectionStart ?? q.length) !== caret) return;
      const matches = list
        .filter((p) => {
          const name = p.name!.toLowerCase();
          return name.startsWith(want) || name.split(/\s+/).some((w) => w.startsWith(want));
        })
        .slice(0, 6);
      // A finished name and a space mean the user has moved on.
      const done = matches.length === 0 || (/\s$/.test(typed) && matches.every((p) => !p.name!.toLowerCase().startsWith(want)));
      suggest = done ? null : { kind: "people", start: at, end: caret, typed, matches };
      active = 0;
      return;
    }
    if (hash >= 0 && hash > at && !/\s/.test(before.slice(hash + 1))) {
      suggest = { kind: "thing" };
      return;
    }
    suggest = q.trim() === "" && recent.length > 0 ? { kind: "recent", items: recent } : null;
    active = 0;
  }

  async function onFocus() {
    focused = true;
    try {
      recent = await search.recent();
    } catch {
      recent = [];
    }
    void updateSuggest();
  }

  function pickPerson(person: PersonDto) {
    const s = suggest;
    if (s?.kind !== "people" || !person.name) return;
    const insert = `@${person.name} `;
    q = q.slice(0, s.start) + insert + q.slice(s.end).replace(/^\s+/, "");
    suggest = null;
    const caret = s.start + insert.length;
    void tick().then(() => {
      inputEl?.focus();
      inputEl?.setSelectionRange(caret, caret);
    });
    onInput();
  }

  function pickRecent(item: string) {
    q = item;
    suggest = null;
    void run();
  }

  async function clearRecent() {
    try {
      await search.clearRecent();
    } catch {}
    recent = [];
    suggest = null;
  }

  /// Keys the open list takes over. True when it handled the key.
  function suggestKey(e: KeyboardEvent): boolean {
    const s = suggest;
    if (!s || s.kind === "thing") {
      if (s && e.key === "Escape") {
        suggest = null;
        return true;
      }
      return false;
    }
    const count = s.kind === "people" ? s.matches.length : s.items.length;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      active = (active + (e.key === "ArrowDown" ? 1 : count - 1)) % count;
      return true;
    }
    if (e.key === "Escape") {
      suggest = null;
      return true;
    }
    if (e.key === "Enter" || (e.key === "Tab" && s.kind === "people")) {
      if (s.kind === "people") pickPerson(s.matches[active]);
      else pickRecent(s.items[active]);
      return true;
    }
    return false;
  }

  /// The typed part of a suggested name, for picking it out.
  function splitName(name: string, typed: string): [string, string, string] {
    const want = typed.trim().toLowerCase();
    if (!want) return ["", "", name];
    const lower = name.toLowerCase();
    // At the start, or at the start of a later word: "Raju" in "Uncle Raju".
    const at = lower.startsWith(want)
      ? 0
      : lower.split(/(\s+)/).reduce<[number, number]>(
          ([found, pos], part) => [found < 0 && pos > 0 && part.startsWith(want) ? pos : found, pos + part.length],
          [-1, 0],
        )[0];
    if (at < 0) return ["", "", name];
    return [name.slice(0, at), name.slice(at, at + want.length), name.slice(at + want.length)];
  }

  /// A search the user settled on goes into the recent list.
  function rememberQuery() {
    const query = q.trim();
    if (query) void search.remember(query).catch(() => {});
  }

  async function run() {
    if (debounceId) window.clearTimeout(debounceId);
    debounceId = undefined;
    if (composing) return;
    pagingError = false;
    const seq = ++runSeq;
    pageSeq += 1;
    loadingMore = false;
    const query = q.trim();
    error = null;
    showAddDialog = false;
    actionBusy = false;
    if (!query) {
      results = null;
      loadedQuery = null;
      loadedRevision = null;
      loading = false;
      selection.clear();
      saveSearchCache();
      return;
    }
    loading = true;
    const atRevision = dataRevision.version;
    try {
      const nextResults = await search.query(query, 0, 200);
      if (!mounted || seq !== runSeq || query !== q.trim() || atRevision !== dataRevision.version || libraryStore.session !== currentSession) return;
      results = dropKnownTrashed(nextResults);
      loadedQuery = query;
      loadedRevision = atRevision;

      if (results) browseContext.set(`search:${query}`, results.photo_ids);
      selection.clear();
      saveSearchCache();
      void loadRemaining(query, atRevision, seq);
    } catch (e) {
      if (mounted && seq === runSeq) {
        results = null;
        loadedQuery = null;
        loadedRevision = null;

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
    // Invalidate old responses immediately; only the replacement request waits.
    runSeq += 1;
    pageSeq += 1;
    loading = false;
    loadingMore = false;
    results = null;
    loadedQuery = null;
    loadedRevision = null;
    selection.clear();
    if (debounceId) window.clearTimeout(debounceId);
    debounceId = undefined;
    if (!composing) debounceId = window.setTimeout(run, 300);
  }

  function clearSearch() {
    if (debounceId) window.clearTimeout(debounceId);
    debounceId = undefined;
    q = "";
    runSeq += 1;
    pageSeq += 1;
    results = null;
    loadedQuery = null;
    loadedRevision = null;

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

  async function showMorePhotos(): Promise<boolean> {
    if (!results || loadingMore || !results.has_more || results.photos.length >= MAX_SEARCH_RESULTS) return false;
    const query = loadedQuery;
    const revision = loadedRevision;
    if (!query || query !== q.trim() || revision !== dataRevision.version) return false;
    const seq = runSeq;
    const pageToken = ++pageSeq;
    const resultAtStart = results;
    loadingMore = true;
    try {
      const page = nextSearchPage(resultAtStart.photos.length, 200);
      if (!page) return false;
      const next = await search.query(query, page.offset, page.limit);
      if (
        !mounted ||
        seq !== runSeq ||
        pageToken !== pageSeq ||
        query !== loadedQuery ||
        query !== q.trim() ||
        revision !== loadedRevision ||
        revision !== dataRevision.version
      ) return false;
      const existingIds = new Set(results.photo_ids);
      const freshPhotos = next.photos.filter((photo) => !existingIds.has(photo.photo_id));
      const freshIds = next.photo_ids.filter((photoId) => !existingIds.has(photoId));
      results = {
        ...results,
        photos: [...results.photos, ...freshPhotos],
        photo_ids: [...results.photo_ids, ...freshIds],
        has_more: next.has_more,
      };

      browseContext.set(`search:${query}`, results.photo_ids);
      saveSearchCache();
      const advanced = results.photos.length > resultAtStart.photos.length;
      if (!advanced && results.has_more) pagingError = true;
      return advanced;
    } catch (e) {
      if (mounted && seq === runSeq) pagingError = true;
      if (mounted && seq === runSeq) toasts.error(`Couldn't load more results: ${commandErrorMessage(e)}`);
      return false;
    } finally {
      if (mounted && seq === runSeq && pageToken === pageSeq) loadingMore = false;
    }
  }

  async function loadRemaining(query: string, revision: number, seq: number) {
    await drainSearchPages(
      () => mounted && seq === runSeq && query === q.trim() && query === loadedQuery
        && libraryStore.session === currentSession && revision === loadedRevision
        && revision === dataRevision.version && !!results?.has_more
        && results.photos.length < MAX_SEARCH_RESULTS,
      showMorePhotos,
    );
  }

  async function bulkTrash() {
    if (!results || actionBusy) return;
    const ids = selection.listIn(results.photo_ids);
    if (ids.length === 0) return;
    const seq = runSeq;
    const query = q.trim();
    const session = libraryStore.session;
    const drop = new Set(ids);
    const snapshot = results.photos
      .map((photo, idx) => ({ photo, idx }))
      .filter((entry) => drop.has(entry.photo.photo_id));
    const idSnapshot = results.photo_ids
      .map((photoId, idx) => ({ photoId, idx }))
      .filter((entry) => drop.has(entry.photoId));
    try {
      actionBusy = true;
      const result = await trash.trashPhotos(ids, session);
      if (!mounted || seq !== runSeq || query !== q.trim() || !results) return;
      if (result.count === 0) {
        toasts.info("Those photos are already in Trash");
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
          await trash.restore(ids, session);
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
    if (inInactivePanel(pageEl)) return;
    if (ignoreShortcut(e)) return;
    if ((e.ctrlKey || e.metaKey) && !e.shiftKey && e.key.toLowerCase() === "a") {
      const ids = visiblePhotoIds;
      if (ids.length > 0) { selection.replace(new Set(ids)); e.preventDefault(); }
      return;
    }
    if (!selection.active()) return;
    if (e.key === "Escape") { selection.clear(); e.preventDefault(); }
    else if (e.key === "Delete" || e.key === "Backspace") { bulkTrash(); e.preventDefault(); }
    else if ((e.key === "a" || e.key === "A") && !e.metaKey && !e.ctrlKey) {
      showAddDialog = true; e.preventDefault();
    }
  }

  onMount(() => {
    mounted = true;
    let unlistenSemantic: UnlistenFn | undefined;
    let cancelled = false;
    listen("semantic:ready", () => {
      if (mounted && q.trim()) void run();
    }).then((unlisten) => {
      if (cancelled) unlisten();
      else unlistenSemantic = unlisten;
    }).catch(() => {});
    if (results) {
      if (loadedQuery && loadedRevision !== null) void loadRemaining(loadedQuery, loadedRevision, runSeq);
      browseContext.set(`search:${q.trim()}`, results.photo_ids);
      requestAnimationFrame(() => {
        if (mounted && pageEl && useSearchCache) {
          pageEl.scrollTop = searchScrollTops.get(sessionKey) ?? 0;
        }
      });
    } else {
      inputEl?.focus();
      if (q.trim()) run();
    }
    const el = pageEl;
    const onScroll = () => {
      if (el) searchScrollTops.set(sessionKey, el.scrollTop);
    };
    el?.addEventListener("scroll", onScroll, { passive: true });
    window.addEventListener("keydown", onGlobalKey);
    return () => {
      cancelled = true;
      unlistenSemantic?.();
      if (el) searchScrollTops.set(sessionKey, el.scrollTop);
      saveSearchCache();
      mounted = false;
      runSeq += 1;
      if (debounceId) window.clearTimeout(debounceId);
      el?.removeEventListener("scroll", onScroll);
      window.removeEventListener("keydown", onGlobalKey);
    };
  });

  /// Keep on-screen results honest: the moment the library's data
  /// revision moves (trash/restore here or on another route, a
  /// completed scan/faces job…), reconcile immediately with the known
  /// trashed ids and re-run the query in the background so additions
  /// and metadata changes appear too. The runSeq guard inside `run`
  /// still guarantees a newer query always wins.
  $effect(() => {
    void photoVisibility.version;
    void dataRevision.version;
    if (!results) return;
    const trashed = photoVisibility.trashedIds;
    if (results.photo_ids.some((id) => trashed.has(id))) {
      results = dropKnownTrashed(results);
      if (browseContext.source?.startsWith("search:")) {
        browseContext.set(`search:${q.trim()}`, results.photo_ids);
      }
    }
    const query = q.trim();
    if (
      query &&
      loadedQuery === query &&
      loadedRevision !== dataRevision.version &&
      !loading
    ) {
      void run();
    }
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
    loadedQuery = null;
    loadedRevision = null;

    saveSearchCache();
    if (debounceId) window.clearTimeout(debounceId);
    void run();
  });
</script>

<PageHeader title="Search" />
<AssetSetup feature="visual" />

<div class="search-row">
  <div class="bar">
    <svg class="lookup" width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
      <circle cx="6" cy="6" r="4" stroke="currentColor" stroke-width="1.4"/>
      <path d="M9.2 9.2L12 12" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/>
    </svg>
    <input
      bind:this={inputEl}
      bind:value={q}
      oninput={() => { onInput(); void updateSuggest(); }}
      onfocus={onFocus}
      onblur={() => { focused = false; suggest = null; }}
      onclick={() => void updateSuggest()}
      oncompositionstart={() => { composing = true; onInput(); }}
      oncompositionend={() => { composing = false; onInput(); }}
      onkeydown={(event) => {
        if (event.isComposing) return;
        if (suggestKey(event)) { event.preventDefault(); return; }
        if (event.key === "Enter") { event.preventDefault(); rememberQuery(); void run(); }
      }}
      placeholder="Search photos, people, places · @someone #something"
      class="search-input"
      aria-label="Search"
    />
    {#if loading}<span class="loading mono">…</span>{/if}
    {#if q}
      <button class="clear-search" type="button" onclick={clearSearch} aria-label="Clear search" title="Clear search">
        <X size={14} strokeWidth={2} />
      </button>
    {/if}
  </div>
  {#if suggest}
    <!-- Items take the mouse down so the input keeps focus. -->
    <div class="suggest" role="listbox" aria-label={suggest.kind === "people" ? "People" : suggest.kind === "recent" ? "Recent searches" : "Search tip"}>
      {#if suggest.kind === "people"}
        {@const s = suggest}
        <div class="suggest-head">People</div>
        {#each s.matches as p, i (p.id)}
          {@const [lead, hit, rest] = splitName(p.name ?? "", s.typed)}
          <button
            type="button"
            class="suggest-item"
            class:active={i === active}
            role="option"
            aria-selected={i === active}
            onmousedown={(e) => { e.preventDefault(); pickPerson(p); }}
            onmouseenter={() => (active = i)}
          >
            {#if p.representative_thumbnail_path}
              <img class="avatar" src={thumbUrl(libraryStore.driveRoot, p.representative_thumbnail_path) ?? ""} alt="" />
            {:else}
              <span class="avatar initial">{(p.name ?? "?").slice(0, 1)}</span>
            {/if}
            <span class="name">{lead}<strong>{hit}</strong>{rest}</span>
            <span class="meta mono">{countOf(p.photo_count, "photo")}</span>
          </button>
        {/each}
      {:else if suggest.kind === "thing"}
        <p class="suggest-hint">Looks for what's in the photo, like <em>#beach</em> or <em>#birthday cake</em>.</p>
      {:else}
        {@const s = suggest}
        <div class="suggest-head">
          Recent
          <button type="button" class="suggest-clear" onmousedown={(e) => { e.preventDefault(); void clearRecent(); }}>Clear</button>
        </div>
        {#each s.items as item, i (item)}
          <button
            type="button"
            class="suggest-item"
            class:active={i === active}
            role="option"
            aria-selected={i === active}
            onmousedown={(e) => { e.preventDefault(); pickRecent(item); }}
            onmouseenter={() => (active = i)}
          >
            <History size={14} strokeWidth={1.75} />
            <span class="name">{item}</span>
          </button>
        {/each}
      {/if}
    </div>
  {/if}
</div>

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

<div class="page" bind:this={pageEl} use:marqueeSelect={{ getAllIds: () => visiblePhotoIds }}
  onclickcapture={(e) => { if ((e.target as Element | null)?.closest("a")) rememberQuery(); }}>
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
            Photos · {results.photos.length}{results.has_more ? "+" : ""}
          </h3>
          {#if pagingError}
            <button class="ghost" onclick={() => run()}>Retry search</button>
          {:else if results.photos.length >= MAX_SEARCH_RESULTS}
            <span class="muted small">Showing the first 20,000. Try a narrower search.</span>
          {:else if results.has_more}
            <span class="muted small">{loadingMore ? "Loading…" : "Continuing automatically…"}</span>
          {/if}
        </div>
        <VirtualGrid count={visiblePhotos.length} scrollEl={pageEl}>
          {#snippet children(start, end)}
          {#each visiblePhotos.slice(start, end) as p (p.photo_id)}
            <a
              class="pv-photo-cell"
              class:selected={selection.has(p.photo_id)}
              data-photo-id={p.photo_id}
              aria-label={photoLabel(p)}
              href="#/photo?id={p.photo_id}"
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
          {/snippet}
        </VirtualGrid>
      </section>
    {/if}
    {#if results.people.length === 0 && results.albums.length === 0 && results.places.length === 0 && results.photos.length === 0}
      <div class="empty">
        <p>No matches. Try other words, a name or a place.</p>
      </div>
    {/if}
  {:else if !loading}
    <div class="empty">
      <p>Try a person, a place, a year, or anything you remember about the photo.</p>
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
  .search-row { padding: var(--s-4) var(--s-7) 0; position: relative; }
  .suggest {
    position: absolute;
    left: var(--s-7);
    right: var(--s-7);
    top: calc(100% + 6px);
    z-index: 20;
    max-width: 520px;
    padding: 6px;
    background: var(--bg-paper);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    box-shadow: 0 18px 40px -14px rgb(0 0 0 / 45%), 0 2px 6px rgb(0 0 0 / 12%);
    animation: suggest-in 140ms var(--ease) both;
  }
  @keyframes suggest-in { from { opacity: 0; transform: translateY(-4px); } }
  @media (prefers-reduced-motion: reduce) { .suggest { animation: none; } }
  .suggest-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 8px 6px;
    font-size: var(--t-xs);
    font-weight: 600;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--ink-muted);
  }
  .suggest-clear {
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--ink-muted);
    font-size: var(--t-xs);
    letter-spacing: 0;
    text-transform: none;
    font-weight: 500;
  }
  .suggest-clear:hover { color: var(--ink); }
  .suggest-item {
    display: flex;
    align-items: center;
    gap: var(--s-3);
    width: 100%;
    min-height: 40px;
    padding: 6px 8px;
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--ink-soft);
    text-align: left;
    font-size: var(--t-sm);
    transition: background var(--t-fast) var(--ease), color var(--t-fast) var(--ease);
  }
  .suggest-item.active { background: var(--bg-card); color: var(--ink); }
  .suggest-item .name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .suggest-item .name strong { color: var(--ink); font-weight: 600; }
  .suggest-item .meta { color: var(--ink-faint); font-size: var(--t-xs); }
  .avatar {
    width: 26px;
    height: 26px;
    flex-shrink: 0;
    border-radius: 50%;
    object-fit: cover;
    box-shadow: 0 0 0 1px var(--line);
  }
  .avatar.initial {
    display: inline-grid;
    place-items: center;
    background: var(--bg-card);
    color: var(--ink-muted);
    font-size: var(--t-xs);
    font-weight: 600;
  }
  .suggest-hint {
    margin: 0;
    padding: 8px 10px;
    font-size: var(--t-sm);
    color: var(--ink-muted);
  }
  .suggest-hint em { color: var(--ink-soft); font-style: normal; font-weight: 500; }
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
  .interpreted .chip[data-kind="missing"] {
    border-style: dashed;
    color: var(--hot);
    border-color: color-mix(in oklab, var(--hot) 45%, var(--line));
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
