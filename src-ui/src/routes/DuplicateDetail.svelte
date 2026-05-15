<script lang="ts">
  import { duplicates } from "../lib/api/all";
  import { libraryStore } from "../lib/stores/library.svelte";
  import { browseContext } from "../lib/stores/browseContext.svelte";
  import { thumbUrl } from "../lib/thumbnail";
  import { thumbnailOnVisible } from "../lib/thumbnailRequest";
  import DetailHeader from "../lib/components/DetailHeader.svelte";
  import { ZoomIn } from "lucide-svelte";
  import type { DupMember } from "../lib/api/all";

  interface Props { id: number }
  let { id }: Props = $props();

  let group = $state<{ id: number; members: DupMember[] } | null>(null);
  let error = $state<string | null>(null);

  async function load() {
    try { group = await duplicates.getGroup(id); }
    catch (e) { error = JSON.stringify(e); }
  }

  async function setKeep(photoId: number) {
    try { group = await duplicates.setKeep(id, photoId); }
    catch (e) { error = JSON.stringify(e); }
  }

  function patchThumbnail(photoId: number, thumbnailPath: string) {
    if (!group) return;
    group = {
      ...group,
      members: group.members.map((m) => (
        m.photo_id === photoId ? { ...m, thumbnail_path: thumbnailPath } : m
      )),
    };
  }

  async function trashOthers() {
    if (!confirm("Trash all non-keep duplicates?")) return;
    await duplicates.trashOthers(id);
    window.location.hash = "/duplicates";
  }

  async function dismiss() {
    await duplicates.dismiss(id);
    window.location.hash = "/duplicates";
  }

  $effect(() => { void id; load(); });

  function fmtSize(b: number | null): string {
    if (b === null) return "—";
    return (b / 1024 / 1024).toFixed(1) + " MB";
  }
</script>

<DetailHeader backHref="#/duplicates" backLabel="Duplicates">
  {#snippet title()}
    <h1>The same photograph</h1>
  {/snippet}
  {#snippet subtitle()}
    {#if group}
      <span class="mono">{group.members.length} copies</span>
      <span class="hint">Pick one to keep — trash the rest.</span>
    {/if}
  {/snippet}
  {#snippet actions()}
    {#if group && group.members.length > 1}
      <button class="ghost" onclick={dismiss}>Dismiss</button>
      <button class="danger" onclick={trashOthers}>
        Trash {group.members.length - 1} other{group.members.length - 1 === 1 ? "" : "s"}
      </button>
    {/if}
  {/snippet}
</DetailHeader>

{#if error}<p class="error" style="padding: var(--s-3) var(--s-7)">{error}</p>{/if}

{#if group}
  {@const memberIds = group.members.map((m) => m.photo_id)}
  <div class="grid">
    {#each group.members as m (m.photo_id)}
      <div class="card" class:keep={m.is_suggested_keep}>
        <!--
          Frame is image-only — no anchor wrapping. The card's "Keep
          this" button below is the primary action; we expose photo
          viewing via a small "Open" pill in the corner. Keeps the
          two clearly distinct.
        -->
        <div
          class="frame"
          use:thumbnailOnVisible={{
            id: m.photo_id,
            thumbnailPath: m.thumbnail_path,
            onReady: (path) => patchThumbnail(m.photo_id, path),
          }}
        >
          {#if m.thumbnail_path}
            <img src={thumbUrl(libraryStore.driveRoot, m.thumbnail_path) ?? ""} alt="" />
          {/if}
          {#if m.is_suggested_keep}<span class="badge">Keep</span>{/if}
          <a
            class="open-pill"
            href="#/photo?id={m.photo_id}"
            onclick={() => browseContext.set(`duplicate:${id}`, memberIds)}
            aria-label="Open at full size"
            title="Open at full size"
          >
            <ZoomIn size={14} strokeWidth={2} />
            <span>Open</span>
          </a>
        </div>
        <dl>
          <dt>Size</dt><dd class="mono">{fmtSize(m.file_size)}</dd>
          <dt>Date</dt><dd class="mono">{m.date_taken ?? "—"}</dd>
          <dt>Path</dt><dd class="mono path" title={m.file_path ?? ""}>{m.file_path ?? "—"}</dd>
        </dl>
        <div class="actions">
          {#if m.is_suggested_keep}
            <button class="keep-btn keeping" disabled>
              <span class="check-glyph" aria-hidden="true">✓</span>
              Keeping this
            </button>
          {:else}
            <button class="keep-btn primary" onclick={() => setKeep(m.photo_id)}>
              Keep this one
            </button>
          {/if}
        </div>
      </div>
    {/each}
  </div>
{/if}

<style>
  .hint {
    color: var(--ink-muted);
    font-style: italic;
  }
  .grid {
    padding: var(--s-4) var(--s-7) var(--s-7);
    flex: 1;
    overflow-y: auto;
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
    gap: var(--s-4);
  }
  .card {
    background: var(--bg-card);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    transition: border-color var(--t-fast) var(--ease);
  }
  .card.keep {
    border-color: var(--keep);
    box-shadow: 0 0 0 1px var(--keep) inset;
  }
  .frame {
    aspect-ratio: 4 / 3;
    background: var(--bg-elev);
    position: relative;
    display: block;
  }
  .frame img { width: 100%; height: 100%; object-fit: contain; background: #000; }
  .badge {
    position: absolute;
    top: var(--s-3);
    left: var(--s-3);
    background: var(--keep);
    color: #fff;
    padding: 3px 10px;
    border-radius: 999px;
    font-size: var(--t-xs);
    font-weight: 600;
    z-index: 1;
  }
  .open-pill {
    position: absolute;
    top: var(--s-3);
    right: var(--s-3);
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 10px;
    background: rgba(0, 0, 0, 0.55);
    color: #fff;
    border-radius: 999px;
    font-size: var(--t-xs);
    font-weight: 500;
    text-decoration: none;
    backdrop-filter: blur(4px);
    z-index: 1;
    transition: background var(--t-fast) var(--ease);
  }
  .open-pill:hover { background: rgba(0, 0, 0, 0.78); }

  dl {
    display: grid;
    grid-template-columns: 60px 1fr;
    gap: 4px var(--s-3);
    padding: var(--s-3) var(--s-4);
    margin: 0;
  }
  dt {
    font-size: var(--t-xs);
    color: var(--ink-muted);
    padding-top: 2px;
  }
  dd {
    margin: 0;
    font-size: var(--t-xs);
    color: var(--ink-soft);
  }
  dd.path {
    color: var(--ink-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .actions {
    padding: 0 var(--s-4) var(--s-4);
    margin-top: auto;
  }
  /*
    Keep button is the primary action of this card — make it
    visually unmistakable. Full-width, accent-colored, slightly
    taller than usual buttons. Disabled "Keeping" state shows the
    chosen status without losing prominence.
  */
  .keep-btn {
    width: 100%;
    padding: 10px 14px;
    font-size: var(--t-base);
    font-weight: 600;
    border-radius: var(--r-md);
    border: 1px solid var(--accent);
    background: var(--accent);
    color: #fff;
    cursor: pointer;
    transition: filter var(--t-fast) var(--ease);
  }
  .keep-btn.primary:hover { filter: brightness(1.08); }
  .keep-btn.keeping {
    background: color-mix(in oklab, var(--keep) 18%, var(--bg-card));
    border-color: var(--keep);
    color: var(--keep);
    cursor: default;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
  }
  .keep-btn.keeping .check-glyph {
    font-weight: 700;
  }
</style>
