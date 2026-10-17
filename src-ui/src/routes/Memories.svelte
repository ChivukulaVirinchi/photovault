<script module lang="ts">
  import type { CollageSuggestion as CachedCollage, MemoryCard as CachedMemoryCard } from "../lib/api/all";

  let cachedMemoriesRoute:
    | {
        driveRoot: string | null;
        /// Library session this cache belongs to. See `currentMemoriesCache`.
        session: number;
        cards: CachedMemoryCard[];
        collages: CachedCollage[];
        scrollTop: number;
      }
    | null = null;
</script>

<script lang="ts">
  import { onMount } from "svelte";
  import { commandErrorMessage } from "../lib/api";
  import { collageApi, memories } from "../lib/api/all";
  import { collage } from "../lib/stores/collage.svelte";
  import { toasts } from "../lib/stores/toast.svelte";
  import CollageSuggestionCard from "../lib/components/CollageSuggestionCard.svelte";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import { thumbnailOnVisible } from "../lib/thumbnailRequest";
  import PageHeader from "../lib/components/PageHeader.svelte";
  import type { CollageSuggestion, MemoryCard } from "../lib/api/all";
  import type { ShapeId } from "../lib/collage";

  const currentDriveRoot = libraryStore.driveRoot;
  const currentSession = libraryStore.session;
  // Keyed on the session as well as the drive root. `session` increments on
  // every open, so closing and reopening the same folder used to serve the
  // previous session's memory cards.
  const currentMemoriesCache =
    cachedMemoriesRoute?.driveRoot === currentDriveRoot &&
    cachedMemoriesRoute.session === currentSession
      ? cachedMemoriesRoute
      : null;

  let cards = $state<MemoryCard[]>(currentMemoriesCache?.cards ?? []);
  let collages = $state<CollageSuggestion[]>(currentMemoriesCache?.collages ?? []);
  let error = $state<string | null>(null);
  let loading = $state((currentMemoriesCache?.cards.length ?? 0) === 0);
  let pageEl = $state<HTMLDivElement | undefined>(undefined);
  let mounted = true;
  let loadSeq = 0;

  function saveMemoriesCache() {
    cachedMemoriesRoute = {
      driveRoot: currentDriveRoot,
      session: currentSession,
      cards,
      collages,
      scrollTop: pageEl?.scrollTop ?? cachedMemoriesRoute?.scrollTop ?? 0,
    };
  }

  function restoreMemoriesScroll() {
    const target = currentMemoriesCache?.scrollTop ?? 0;
    if (target <= 0) return;
    requestAnimationFrame(() => {
      if (mounted && pageEl) pageEl.scrollTop = target;
    });
  }

  async function load() {
    const seq = ++loadSeq;
    loading = true;
    error = null;
    // Suggested collages come separately; they can take a moment longer
    // (thumbnails) and must never hold up or break the memories.
    void loadCollages(seq);
    try {
      const nextCards = await memories.today();
      if (!mounted || seq !== loadSeq) return;
      cards = nextCards;
      saveMemoriesCache();
      restoreMemoriesScroll();
    } catch (e) {
      if (mounted && seq === loadSeq) error = commandErrorMessage(e);
    } finally {
      if (mounted && seq === loadSeq) loading = false;
    }
  }

  async function loadCollages(seq: number) {
    try {
      const next = await collageApi.suggestions();
      if (!mounted || seq !== loadSeq) return;
      collages = next;
      saveMemoriesCache();
    } catch {
      // Suggestions are a nicety; leave whatever was there.
    }
  }

  function openCollage(ids: number[], layoutId: string, shape: ShapeId) {
    collage.show(ids, { auto: true, layoutId, shape });
  }

  async function dismissCollage(suggestion: CollageSuggestion) {
    const before = collages;
    collages = collages.filter((c) => c.key !== suggestion.key);
    saveMemoriesCache();
    try {
      await collageApi.dismissSuggestion(suggestion.key);
      toasts.info("Got it. You won't see that collage again");
    } catch (e) {
      collages = before;
      saveMemoriesCache();
      toasts.error(`Couldn't hide that collage: ${commandErrorMessage(e)}`);
    }
  }

  function todayLabel(): string {
    return new Date().toLocaleDateString("en", {
      weekday: "long", day: "numeric", month: "long",
    });
  }

  function memoryKindLabel(kind: string): string {
    if (kind === "on_this_day") return "From this day";
    if (kind === "fallback_window") return "From your library";
    if (kind === "person_story") return "Together over time";
    if (kind === "place_story") return "A place in your story";
    if (kind === "seasonal_recap" || kind === "year_recap") return "A look back";
    return kind
      .replaceAll("_", " ")
      .replace(/\b\w/g, (m) => m.toUpperCase());
  }

  function patchHeroThumbnail(photoId: number, thumbnailPath: string) {
    cards = cards.map((c) => (
      c.hero_photo_id === photoId ? { ...c, hero_thumbnail_path: thumbnailPath } : c
    ));
  }

  onMount(() => {
    void load();
    restoreMemoriesScroll();
    const el = pageEl;
    const onScroll = () => saveMemoriesCache();
    el?.addEventListener("scroll", onScroll, { passive: true });
    return () => {
      saveMemoriesCache();
      mounted = false;
      loadSeq += 1;
      el?.removeEventListener("scroll", onScroll);
    };
  });
</script>

<PageHeader title="Memories">
  <span class="when mono">{todayLabel()}</span>
</PageHeader>

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

<div class="page" bind:this={pageEl}>
  {#if collages.length > 0}
    <section class="made" aria-labelledby="made-for-you">
      <header>
        <h2 id="made-for-you">Made for you</h2>
        <p>Collages from your photos, put together today. Nothing is kept unless you save it.</p>
      </header>
      <div class="collages">
        {#each collages as suggestion (suggestion.key)}
          <CollageSuggestionCard {suggestion} onopen={openCollage} ondismiss={() => dismissCollage(suggestion)} />
        {/each}
      </div>
    </section>
  {/if}
  {#if loading}
    <div class="empty">
      <p>Loading memories...</p>
    </div>
  {:else if cards.length === 0}
    <div class="empty">
      <p>
        Nothing today. Memories surface once your library has roughly three months of photos to look back on.
      </p>
    </div>
  {:else}
    <div class="cards">
      {#each cards as c (c.id)}
        <a
          class="card"
          href="#/memory?id={c.id}"
          use:thumbnailOnVisible={{
            id: c.hero_photo_id,
            thumbnailPath: c.hero_thumbnail_path,
            onReady: (path) => patchHeroThumbnail(c.hero_photo_id, path),
          }}
        >
          {#if c.hero_thumbnail_path}
            <img src={thumbUrl(libraryStore.driveRoot, c.hero_thumbnail_path) ?? ""} alt="" />
          {/if}
          <div class="overlay">
            <div class="caption">
              <span class="kind mono">{memoryKindLabel(c.kind)}</span>
              <h3>{c.title}</h3>
              <span class="count mono">{c.photo_count} photos</span>
            </div>
          </div>
        </a>
      {/each}
    </div>
  {/if}
</div>

<style>
  .page { padding: var(--s-5) var(--s-7) var(--s-7); flex: 1; overflow-y: auto; }
  .when { font-size: var(--t-sm); color: var(--ink-muted); }
  .empty {
    padding: var(--s-8) var(--s-5);
    text-align: center;
    max-width: 44ch;
    margin: 0 auto;
  }
  .empty p { color: var(--ink-soft); line-height: 1.6; }
  .made { margin-bottom: var(--s-7); }
  .made header { display: flex; align-items: baseline; gap: var(--s-3); flex-wrap: wrap; margin-bottom: var(--s-4); }
  .made h2 {
    margin: 0;
    font-family: var(--font-display);
    font-size: var(--t-xl);
    font-weight: 500;
  }
  .made p { margin: 0; font-size: var(--t-sm); color: var(--ink-muted); }
  .collages {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(170px, 200px));
    gap: var(--s-5);
  }
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: var(--s-4);
  }
  .card {
    aspect-ratio: 3 / 4;
    background: var(--bg-card);
    border-radius: var(--r-md);
    overflow: hidden;
    color: inherit;
    position: relative;
    text-decoration: none;
    transition: box-shadow var(--t-base-d) var(--ease);
    border: 1px solid var(--line);
  }
  .card:hover {
    box-shadow: 0 0 0 2px var(--accent-ghost);
  }
  .card img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    transition: transform var(--t-slow) var(--ease);
  }
  .card:hover img { transform: scale(1.03); }
  .overlay {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: flex-end;
    padding: var(--s-5);
    background: linear-gradient(
      to top,
      rgba(0, 0, 0, 0.82) 0%,
      rgba(0, 0, 0, 0.35) 38%,
      transparent 72%
    );
  }
  .caption {
    display: flex;
    flex-direction: column;
    gap: 4px;
    color: #ebedf0;
  }
  .kind {
    font-size: var(--t-xs);
    color: var(--accent);
    text-transform: uppercase;
    letter-spacing: 0.12em;
    font-weight: 600;
  }
  h3 {
    font-family: var(--font-display);
    font-size: var(--t-2xl);
    font-weight: 500;
    color: #fff;
    line-height: 1.1;
    font-variation-settings: "opsz" 60;
    margin: 0;
  }
  .count {
    font-size: var(--t-xs);
    color: rgba(235, 237, 240, 0.7);
  }
</style>
