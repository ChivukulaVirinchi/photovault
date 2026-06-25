# Smriti Assistant Plan

Status: planning branch. No implementation yet.

## Product Goal

Build a focused in-app Assistant that creates albums from natural language
requests by operating Smriti's typed tools. The Assistant should feel like a
private photo librarian for the current user's local library, not a generic
chatbot and not a broad automation system.

Initial scope is deliberately narrow:

- understand album creation requests
- resolve people, dates, places, media type, and semantic search terms
- find matching photos through Smriti metadata and search services
- preview the candidate album
- create the album only after approval
- tag the album with `created_by = agent`

No favorites, trash, undo, duplicate cleanup, burst curation, or general app
automation in the first version.

## Product Shape

The Assistant should be available anywhere without being tied to the current
route.

- Add an `Assistant` sidebar entry.
- Add a global keyboard shortcut that opens the Assistant input.
- The Assistant opens as a right-side drawer or bottom command panel over the
  current view.
- Switching routes must not cancel the active Assistant run.
- The active run lives in app state until it finishes, is stopped, or the user
  clears the Assistant.
- The UI shows an activity thread, not chain-of-thought.
- The UI has a Stop button for an active run.

Suggested shortcut:

- `Ctrl+K` if available and not already used for global search.
- Otherwise `Ctrl+Shift+A`.

The shortcut should focus the Assistant prompt input. If an Assistant run is
already active, it opens the current run instead of starting a new one.

## Activity Thread

Show operational steps only:

- "Resolving people: me, mom"
- "Checking date range: 2014"
- "Searching matching photos"
- "Found 42 photos"
- "Preparing album preview"
- "Waiting for approval"
- "Creating album"
- "Opening album"

Do not expose chain-of-thought or raw provider reasoning.

## Session Context

The Assistant keeps short-term context in memory until cleared:

- prior user requests in the current Assistant session
- resolved people, places, dates, and albums
- current candidate photo set IDs
- pending album preview
- current approval state

This allows follow-ups such as:

- "Use only 2014"
- "Include Goa too"
- "Actually only photos with both of us"
- "Make that album"

No persistent run history in v1. When dismissed or app closes, the session can
be forgotten. No audit log, no saved prompts, no stored transcript.

## AI Toggle

Add one setting:

`Enable Assistant`

When off:

- hide or disable the Assistant entry point
- disable provider configuration
- reject Assistant IPC commands with a clear disabled error

Semantic search remains available. It is a local search/index feature, not part
of the Assistant toggle.

## Privacy Boundary

The Assistant can see safe Smriti metadata:

- photo IDs
- dates and date source
- media type
- location city/country
- people IDs/names and face-derived membership
- album membership
- favorite/trash flags only as filters/exclusions
- semantic search candidates and scores
- thumbnail references for previews shown by the app

The Assistant cannot see:

- original photo or video bytes
- raw thumbnail bytes
- original filesystem paths
- face embeddings
- semantic vectors
- raw database handles
- private config/secrets

If a provider is remote, only safe tool results cross the provider boundary.
The provider never receives original photos.

## Album Provenance

Agent-created albums should behave like normal albums with one extra field:

- `created_by = agent`

That is enough for v1. Do not store the original prompt or model response.

UI can show a small `Assistant` tag on such albums.

## Tool Boundary

Tools are typed app operations. The model asks for tool calls; Smriti executes
them. The model never runs SQL and never receives direct filesystem access.

### Read Tools

`library_summary`

- counts, date range, people count, album count, semantic index availability

`resolve_people`

- input: name-like text
- output: person candidates with IDs, names, representative thumbnail refs, and
  photo counts
- if ambiguous, Assistant asks the user to pick

`resolve_places`

- input: place text
- output: known city/country candidates from indexed metadata

`parse_date_filter`

- input: text such as `2014`, `March 2020`, `last summer`
- output: normalized date range or clarification request

`search_album_candidates`

- input: structured filters:
  - date range
  - people_all
  - people_only
  - places
  - media type
  - semantic_text
- output: opaque `photo_set_id`, count, sample photo IDs, and safe summaries

`semantic_search`

- input: simple literal phrase such as `beach`, `ocean`, `dog`
- output: candidates and scores using the existing relevance gate
- no hidden synonym expansion
- no prompt templates
- no "photos that look like..." phrasing

`photo_set_summary`

- input: `photo_set_id`
- output: count, date span, people distribution, place distribution, and sample
  photo IDs for preview

### Write Tools

V1 write tools are album-only.

`preview_album_create`

- input: album name and `photo_set_id`
- output: proposed album name, count, representative sample, warnings
- no mutation

`create_album_from_set`

- requires approval token from UI
- creates album
- adds photos from `photo_set_id`
- sets `created_by = agent`
- returns album ID

`open_album`

- navigates UI to the created album
- no model access to route internals

No other write tools in v1.

## Confirmation Policy

Every album creation requires approval.

Approval preview must show:

- proposed album name
- number of photos
- representative sample
- included filters: people, date, place, semantic term, people-only mode
- note if semantic search was used and may be approximate

The user can:

- approve
- cancel
- refine with a follow-up message

## Execution Model

Use an in-memory run state, not the existing background job registry.

States:

- `Idle`
- `Running`
- `WaitingForClarification`
- `WaitingForApproval`
- `Applying`
- `Completed`
- `Stopped`
- `Failed`

The Stop button moves the run to `Stopped`. Any in-flight provider request is
cancelled if possible; otherwise its result is ignored when it returns.

## Provider Abstraction

Keep provider-specific code behind an adapter:

```rust
trait AssistantProvider {
    fn run_turn(&mut self, input: AssistantProviderInput) -> AssistantProviderResult;
}
```

Provider input includes:

- compact Assistant policy
- current user message
- active session summary
- available tool schemas
- recent tool results for this run

Provider output includes:

- tool call
- clarification request
- album preview request
- final user-facing response

Adapters:

- deterministic fake provider for tests
- OpenAI-compatible provider later
- local provider later if needed

No tests should call a real provider.

## Context Engineering Rules

Do not dump the whole library into context. The Assistant should build context
through tools:

1. Resolve entities.
2. Query candidates.
3. Summarize candidate set.
4. Ask for clarification if ambiguous.
5. Preview only the final proposed action.

Keep the active session summary small:

- selected people
- selected date range
- selected place
- selected semantic term
- latest `photo_set_id`
- pending preview

Tool outputs should be compact and structured. Large photo sets are represented
by opaque IDs and summaries, not lists of thousands of photos in the model
context.

## Example Flow

Request:

"Make an album of me and mom from 2014."

Flow:

1. `resolve_people("me")`
2. `resolve_people("mom")`
3. Ask user to choose if either is ambiguous.
4. `parse_date_filter("2014")`
5. `search_album_candidates({
     people_all: [me_id, mom_id],
     date_range: "2014-01-01..2014-12-31",
     people_only: false
   })`
6. `photo_set_summary(photo_set_id)`
7. Show preview.
8. On approval:
   - `create_album_from_set(name, photo_set_id, approval_token)`
   - `open_album(album_id)`

If user says "only me and mom", set `people_only = true`.

## UI Plan

### Components

- `AssistantDrawer.svelte`
- `AssistantPrompt.svelte`
- `AssistantActivity.svelte`
- `AssistantPreview.svelte`
- `AssistantApprovalBar.svelte`
- `AssistantDisambiguation.svelte`

### Stores

- `assistant.svelte.ts`
  - open/closed
  - active run state
  - current input
  - activity thread
  - pending clarification
  - pending preview

### API Client

Add `assistant` namespace in `src-ui/src/lib/api/all.ts`:

- `start(message)`
- `continue(run_id, message)`
- `stop(run_id)`
- `approve(run_id, approval_id)`
- `reject(run_id, approval_id)`
- `state()`

### Keyboard

Register global shortcut in `App.svelte`:

- open drawer
- focus prompt
- do not trigger while typing in another input unless shortcut uses modifier

## Backend Plan

### Engine

Add `src/services/assistant/`:

- `mod.rs`
- `types.rs`
- `tools.rs`
- `runner.rs`
- `policy.rs`
- `provider.rs`
- `fake_provider.rs`

### Tauri

Add `src-tauri/src/commands/assistant.rs`.

Add in `AppState`:

- `assistant: Arc<Mutex<AssistantRuntime>>`

The runtime stores only current in-memory sessions.

### Database

Add album provenance:

- `albums.created_by TEXT DEFAULT 'user'`

Allowed values:

- `user`
- `agent`

No prompt history column.

## Testing Plan

### Engine Tests

- provider output is parsed into typed tool calls
- invalid tool calls are rejected
- people/date filters produce expected search request
- `people_only` is enforced by the search tool
- semantic terms pass through literally
- large photo sets stay behind `photo_set_id`
- album creation requires approval token
- fake provider can complete album flow

### Tauri Tests

- `assistant_start` creates run
- run can pause for clarification
- run can pause for approval
- reject leaves DB unchanged
- approve creates album and sets `created_by = agent`
- stop marks run stopped and ignores late provider output

### Frontend Tests

- shortcut opens Assistant and focuses input
- activity thread renders tool steps
- disambiguation choices continue run
- preview renders count/sample/filters
- approve and cancel buttons call correct commands
- route changes do not clear active run

No real provider in tests.

## Phased Build

### Phase 1: Local Harness And Fake Provider

- Engine types and in-memory run state.
- Tool definitions for album creation flow.
- Fake provider that drives deterministic album scenarios.
- Tauri commands.
- Frontend drawer, shortcut, activity thread, preview, approval UI.
- Album provenance migration.

### Phase 2: Real Provider Adapter

- Provider settings.
- Assistant toggle.
- OpenAI-compatible provider adapter.
- Request cancellation/timeout.
- Redaction of tool results.

### Phase 3: Production Hardening

- Better disambiguation UX.
- Better album naming rules.
- More eval fixtures for people/date/place/semantic album creation.
- Performance checks for large candidate sets.

No additional write tools until album creation is solid.

## Release Criteria

- Assistant can create an album from people/date/place/semantic filters.
- Every album creation shows preview and requires approval.
- Agent-created albums have `created_by = agent`.
- Assistant cannot access original photo paths or bytes.
- Assistant session context survives route changes.
- Clearing Assistant forgets in-memory context.
- Stop button works.
- Tests pass without a real AI provider.
