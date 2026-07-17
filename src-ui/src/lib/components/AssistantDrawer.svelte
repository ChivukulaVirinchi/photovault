<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { Sparkles, X, Send, StopCircle, Check, Ban } from "lucide-svelte";
  import { assistantStore, type AssistantActivityEvent } from "../stores/assistant.svelte";
  import { libraryStore } from "../stores/library.svelte";
  import { thumbUrl } from "../thumbnail";

  let input = $state("");
  let inputEl = $state<HTMLInputElement | null>(null);

  const run = $derived(assistantStore.run);
  const preview = $derived(run?.preview ?? null);
  const busy = $derived(assistantStore.busy);
  const activity = $derived(assistantStore.activity);
  let focusTimer: ReturnType<typeof setTimeout> | null = null;

  function responseHtml(text: string): string {
    const escaped = text
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;");
    return escaped
      .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
      .split(/\n{2,}/)
      .map((block) => {
        const lines = block.split("\n");
        if (lines.every((line) => line.trim().startsWith("- "))) {
          return `<ul>${lines
            .map((line) => `<li>${line.trim().slice(2)}</li>`)
            .join("")}</ul>`;
        }
        return `<p>${lines.join("<br />")}</p>`;
      })
      .join("");
  }

  async function submit() {
    const message = input.trim();
    if (!message || busy) return;
    input = "";
    await assistantStore.start(message);
  }

  function onInputKey(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      submit();
    } else if (e.key === "Escape") {
      assistantStore.hide();
    }
  }

  function onDialogKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      assistantStore.hide();
    }
  }

  onMount(() => {
    const focus = () => inputEl?.focus();
    document.addEventListener("smriti:assistant-focus", focus);
    const unlistenPromise = listen<AssistantActivityEvent>("assistant:activity", (event) => {
      assistantStore.appendActivity(event.payload);
    });
    focusTimer = setTimeout(focus, 0);
    return () => {
      document.removeEventListener("smriti:assistant-focus", focus);
      if (focusTimer != null) clearTimeout(focusTimer);
      unlistenPromise.then((unlisten) => unlisten()).catch(() => {});
    };
  });
</script>

{#if assistantStore.open}
  <div class="scrim" onclick={() => assistantStore.hide()} aria-hidden="true"></div>
  <div class="assistant" role="dialog" aria-modal="true" aria-label="Assistant" tabindex="-1" onkeydown={onDialogKey}>
    <header>
      <div class="title">
        <Sparkles size={16} strokeWidth={1.75} />
        <span>Assistant</span>
      </div>
      <button class="icon" onclick={() => assistantStore.hide()} aria-label="Close Assistant">
        <X size={16} strokeWidth={1.75} />
      </button>
    </header>

    <section class="prompt">
      <input
        bind:this={inputEl}
        bind:value={input}
        onkeydown={onInputKey}
        placeholder="Find photos..."
        disabled={busy}
        aria-label="Assistant request"
      />
      <button class="send" onclick={submit} disabled={busy || !input.trim()} aria-label="Run Assistant">
        <Send size={15} strokeWidth={1.75} />
      </button>
    </section>

    {#if assistantStore.error}
      <p class="error">{assistantStore.error}</p>
    {/if}

    {#if run || activity.length > 0}
      <section class="thread">
        {#if run}<div class="request">{run.message}</div>{/if}
        {#if run?.response}<div class="response">{@html responseHtml(run.response)}</div>{/if}
        {#if run?.clarification_options?.length}
          <div class="choices" aria-label="Clarification options">
            {#each run.clarification_options as option}
              <button class="choice" onclick={() => assistantStore.choose(option)} disabled={busy}>
                {option}
              </button>
            {/each}
          </div>
        {/if}
        <ol>
          {#each activity as item}
            <li>{item.label}</li>
          {/each}
        </ol>
      </section>

      {#if run && preview}
        <section class="preview">
          <div class="preview-head">
            <div>
              <h3>{preview.album_name}</h3>
              <p>{preview.photo_count.toLocaleString()} {preview.photo_count === 1 ? "photo" : "photos"}</p>
            </div>
            {#if run.status === "waiting_for_approval"}
              <span class="state">Approval needed</span>
            {:else if run.status === "results_ready"}
              <span class="state">Results</span>
            {/if}
          </div>

          <div class="filters">
            {#each preview.people as person}
              <span>{person.name}</span>
            {/each}
            {#each preview.places as place}
              <span>{place.label}</span>
            {/each}
            {#if preview.date}<span>{preview.date.label}</span>{/if}
            {#if preview.media_type}<span>{preview.media_type}</span>{/if}
            {#if preview.people_only}<span>only these people</span>{/if}
            {#if preview.semantic_text}<span>{preview.semantic_text}</span>{/if}
          </div>

          <div class="samples">
            {#each preview.sample as photo}
              <a href="#/photo?id={photo.id}" class="sample">
                {#if photo.thumbnail_path}
                  <img src={thumbUrl(libraryStore.driveRoot, photo.thumbnail_path) ?? ""} alt="" />
                {/if}
              </a>
            {/each}
          </div>

          {#if run.status === "waiting_for_approval"}
            <div class="actions">
              <button class="primary" onclick={() => assistantStore.approve()} disabled={busy || preview.photo_count === 0}>
                <Check size={15} strokeWidth={1.75} />
                Create album
              </button>
              <button class="ghost" onclick={() => assistantStore.reject()} disabled={busy}>
                <Ban size={15} strokeWidth={1.75} />
                Cancel
              </button>
            </div>
          {/if}
        </section>
      {/if}

      <footer>
        {#if run && run.status !== "completed" && run.status !== "stopped" && run.status !== "failed"}
          <button class="ghost danger-soft" onclick={() => assistantStore.stop()}>
            <StopCircle size={15} strokeWidth={1.75} />
            Stop
          </button>
        {/if}
        <button class="ghost" onclick={() => assistantStore.clear()} disabled={busy}>Clear</button>
      </footer>
    {:else}
      <div class="empty">
        <p>Ask to find photos or create an album by people, date, place, or visual meaning.</p>
      </div>
    {/if}
  </div>
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.28);
    z-index: 80;
  }
  .assistant {
    position: fixed;
    right: 0;
    top: 0;
    bottom: 0;
    width: min(440px, 100vw);
    background: var(--bg-paper);
    border-left: 1px solid var(--line);
    box-shadow: 0 16px 50px rgba(0, 0, 0, 0.28);
    z-index: 81;
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow-y: auto;
  }

  header {
    height: 52px;
    padding: 0 var(--s-4);
    border-bottom: 1px solid var(--line-soft);
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-shrink: 0;
  }
  .title {
    display: flex;
    align-items: center;
    gap: var(--s-2);
    font-weight: 650;
  }
  .icon, .send {
    width: 32px;
    height: 32px;
    padding: 0;
    display: grid;
    place-items: center;
  }
  .prompt {
    display: grid;
    grid-template-columns: 1fr 34px;
    gap: var(--s-2);
    padding: var(--s-4);
    border-bottom: 1px solid var(--line-soft);
    flex-shrink: 0;
  }
  input {
    min-width: 0;
    padding: 8px var(--s-3);
    border-radius: var(--r-sm);
    border: 1px solid var(--line);
    background: var(--bg);
    color: var(--ink);
  }
  .error {
    margin: var(--s-3) var(--s-4) 0;
    color: var(--danger, #d96363);
    font-size: var(--t-sm);
  }
  .thread, .preview, .empty {
    margin: var(--s-4);
  }
  .thread {
    border-bottom: 1px solid var(--line-soft);
    padding-bottom: var(--s-4);
  }
  .request {
    color: var(--ink);
    font-size: var(--t-sm);
    margin-bottom: var(--s-3);
  }
  .response {
    color: var(--ink);
    font-size: var(--t-sm);
    line-height: 1.45;
    margin-bottom: var(--s-3);
  }
  .response :global(p) {
    margin: 0 0 var(--s-2);
  }
  .response :global(p:last-child) {
    margin-bottom: 0;
  }
  .response :global(ul) {
    margin: var(--s-2) 0 0;
    padding-left: 18px;
  }
  .choices {
    display: flex;
    flex-wrap: wrap;
    gap: var(--s-2);
    margin: 0 0 var(--s-3);
  }
  .choice {
    border: 1px solid var(--line);
    background: var(--bg);
    color: var(--ink);
    border-radius: var(--r-sm);
    padding: 6px 9px;
    font-size: var(--t-sm);
  }
  ol {
    margin: 0;
    padding-left: 18px;
    color: var(--ink-muted);
    font-size: var(--t-sm);
    display: grid;
    gap: 6px;
  }
  .preview {
    overflow-y: auto;
  }
  .preview-head {
    display: flex;
    justify-content: space-between;
    gap: var(--s-3);
    align-items: flex-start;
    margin-bottom: var(--s-3);
  }
  h3 {
    margin: 0 0 3px;
    font-size: var(--t-lg);
  }
  .preview-head p {
    margin: 0;
    color: var(--ink-muted);
    font-size: var(--t-sm);
  }
  .state {
    font-size: var(--t-xs);
    color: var(--accent);
    white-space: nowrap;
  }
  .filters {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: var(--s-3);
  }
  .filters span {
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    padding: 3px 7px;
    color: var(--ink-soft);
    font-size: var(--t-xs);
  }
  .samples {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 6px;
  }
  .sample {
    aspect-ratio: 1;
    background: var(--bg-card);
    border: 1px solid var(--line-soft);
    border-radius: var(--r-sm);
    overflow: hidden;
  }
  .sample img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .actions, footer {
    display: flex;
    gap: var(--s-2);
    margin-top: var(--s-4);
  }
  .actions button, footer button {
    display: inline-flex;
    align-items: center;
    gap: var(--s-2);
  }
  footer {
    margin: auto var(--s-4) var(--s-4);
    padding-top: var(--s-3);
    border-top: 1px solid var(--line-soft);
  }
  .empty {
    color: var(--ink-muted);
    font-size: var(--t-sm);
  }
</style>
