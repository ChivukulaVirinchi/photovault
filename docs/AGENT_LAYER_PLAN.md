# Smriti Agent Layer Plan

Status: planning branch. No implementation yet.

## Goal

Build an in-app agent that can operate Smriti through explicit tools, not
through raw filesystem access and not through hardcoded prompt flows. The agent
should be able to understand a user request, inspect library metadata through
safe query tools, propose concrete actions, and execute approved changes through
the same application boundary used by the UI.

The agent must be a first-class application citizen:

- It uses typed Smriti tools, not direct SQL, raw photo files, or image pixels.
- It can read photo metadata, generated semantic metadata, people, albums,
  dates, places, favorites, stacks, trash state, and search results.
- It can perform controlled app actions such as creating albums, adding photos
  to albums, applying filters, starting indexed jobs, and navigating the UI.
- It previews destructive or bulk actions before applying them.
- It is provider-agnostic: local model, OpenAI-compatible provider, or future
  local runtime should use the same tool protocol.

## Non-Goals

- No direct access to original photo bytes, raw thumbnails, filesystem paths,
  EXIF blobs, or private DB handles.
- No autonomous background edits without a user request and confirmation.
- No open-ended taxonomy, hand-built synonym dictionary, or brittle semantic
  label layer.
- No chatbot-only feature where the model merely returns text and the app then
  manually implements one-off commands.
- No model-specific command code. Provider differences stay behind an adapter.

## Core Product Shape

The agent should feel like a command surface for the library, not a support
chatbot. The main UI can be a compact assistant panel or command drawer, but
the important surface is the tool execution flow:

1. User asks for an outcome.
2. Agent plans with app tools.
3. Agent shows a short preview when action scope is non-trivial.
4. User approves, edits, or cancels.
5. Agent applies through Smriti commands.
6. App shows the resulting view or created object.

Example:

User: "Make an album of me and mom from 2014."

Expected tool flow:

1. Resolve people matching "me" and "mom".
2. Ask user to disambiguate if multiple people match.
3. Query photos from 2014 containing both people.
4. If user said "only me and mom", filter out photos with other recognized or
   inferred identities.
5. Preview count and a small representative result set.
6. Create album, add photos, and open the album.

## Architecture

### Engine Layer

Add `src/services/agent/` with provider-independent primitives:

- `AgentRequest`
- `AgentTurn`
- `AgentToolCall`
- `AgentToolResult`
- `AgentActionPreview`
- `AgentExecutionPlan`
- `AgentRunState`
- `AgentError`

The engine owns tool definitions and execution policy. It should not know about
Svelte UI details.

### Tauri Layer

Add `src-tauri/src/commands/agent.rs` with thin IPC handlers:

- `agent_start`
- `agent_continue`
- `agent_cancel`
- `agent_approve_action`
- `agent_reject_action`
- `agent_list_tools`

Agent runs should be job-like, but not identical to indexing jobs. A run may
pause for approval or disambiguation, then continue.

### Frontend Layer

Add a compact agent surface:

- command drawer or side panel
- streaming status of tool calls
- disambiguation chips for people/albums/places
- preview panel for proposed changes
- approve/cancel controls
- link/open resulting album/search/person/timeline view

No decorative AI UI. Keep it operational and quiet.

## Provider Abstraction

Introduce a provider trait behind the Tauri layer:

```rust
trait AgentProvider {
    fn run_turn(&mut self, input: AgentProviderInput) -> AgentProviderResult;
}
```

Provider input includes:

- system policy
- current user request
- compact app context
- available tool schemas
- prior tool results for the active run

Provider output includes:

- tool call
- final response
- request for clarification
- action preview

Provider adapters:

- OpenAI-compatible HTTP provider
- future local provider
- test provider for deterministic tests

Do not bake provider prompts into random commands. Tool descriptions and policy
belong in one agent module.

## Tool Boundary

Tools are the real product. They must be typed, narrow, testable, and reusable.

### Read Tools

- `library_summary`
  - counts, date range, indexed semantic status, people count, album count
- `search_photos`
  - accepts structured filters: date range, people, places, media type,
    favorites, semantic text, strict people-only mode
  - returns photo IDs plus safe summaries, not paths
- `semantic_search`
  - uses current semantic vector search and score gate
  - returns candidates with scores and safe summaries
- `resolve_people`
  - name query to person candidates
- `resolve_album`
  - album name query to album candidates
- `resolve_place`
  - place query to known place candidates
- `photo_set_summary`
  - count, date span, people distribution, places, sample thumbnails by ID only
- `album_preview`
  - safe album metadata and member summaries

### Write Tools

All write tools must support dry-run first.

- `create_album`
- `add_photos_to_album`
- `remove_photos_from_album`
- `mark_favorite`
- `move_photos_to_trash`
- `restore_photos`
- `rename_album`
- `open_view`
  - navigates UI to album/search/person/timeline/map/detail

Bulk write tools require approval when they affect more than a small threshold
or are destructive.

### Job Tools

- `start_semantic_indexing`
- `start_face_processing`
- `start_duplicate_detection`
- `start_burst_detection`
- `get_job_status`
- `cancel_job`

These should not hide long-running work behind an agent text response.

## Data Access Rules

The agent sees:

- photo IDs
- dates and date source
- media type
- location city/country
- people IDs/names and confidence where available
- album membership
- favorite/trash state
- generated semantic candidate scores
- thumbnail IDs or safe thumbnail references for UI preview only

The agent does not see:

- original filesystem paths
- original image/video bytes
- raw thumbnails as bytes
- face embeddings
- semantic vectors
- database file path
- private user config secrets

If a future tool needs visual inspection, it should use generated metadata from
Smriti services, not hand the model the image.

## Confirmation Policy

No approval required:

- read-only queries
- navigation
- tiny non-destructive drafts

Approval required:

- create or rename album
- add/remove photos in bulk
- favorite/unfavorite in bulk
- start long-running jobs
- any trash/restore operation

The approval UI should show:

- action type
- number of photos affected
- representative sample
- reversible or irreversible status
- destination album/view

## Execution Model

Use a state machine:

- `Running`
- `WaitingForTool`
- `WaitingForUserClarification`
- `WaitingForApproval`
- `Applying`
- `Completed`
- `Cancelled`
- `Failed`

Each run stores a short audit trail:

- user request
- tool calls
- tool results summary
- approval decision
- final applied actions

Audit should be local-only and bounded. Do not store provider raw responses
forever by default.

## Example Tool Flow: Album From People And Date

Request:

"Make an album of me and mom from 2014."

Tool sequence:

1. `resolve_people({"query":"me"})`
2. `resolve_people({"query":"mom"})`
3. If ambiguous, ask user to choose.
4. `search_photos({
     "date_range": {"start":"2014-01-01","end":"2014-12-31"},
     "people_all": [me_id, mom_id],
     "people_only": false
   })`
5. `photo_set_summary(photo_ids)`
6. Preview: "Found 42 photos from 2014 containing both people."
7. On approval:
   - `create_album({"name":"Me and Mom, 2014"})`
   - `add_photos_to_album(album_id, photo_ids)`
   - `open_view({"kind":"album","id":album_id})`

If user says "only me and mom", set `people_only: true`.

## Semantic Search Integration

The agent should use semantic search as one signal, not as a truth oracle.

Good uses:

- "find beach photos"
- "show photos that look like a family portrait"
- "make a rough album of dog photos"
- "find visually similar photos to this one"

Bad uses:

- "all photos containing a child" with guaranteed recall
- "all documents with Aadhaar"
- "all photos of a thread ceremony"

For high-recall requests, the agent should communicate uncertainty and preview
the candidate set. It should never silently imply perfect recall from vector
search.

## Testing Strategy

Engine tests:

- tool schema serialization
- tool permission policy
- people/date/album filter composition
- dry-run vs apply behavior
- approval state machine
- provider adapter with deterministic fake provider

Tauri tests:

- command DTO snapshots
- agent run lifecycle
- approval/resume
- cancellation

Frontend tests:

- preview rendering
- disambiguation UI
- approve/cancel flow
- navigation after successful action

No tests should call a real AI provider.

## Phased Build

### Phase 1: Tool Harness

- Add agent service module and run state.
- Add deterministic fake provider.
- Add read tools and write dry-runs.
- Add Tauri commands.
- No real provider yet.

### Phase 2: App Actions

- Implement album create/add flow.
- Implement person/date/photo search tool.
- Add approval UI.
- Ship one real flow: create album from people/date/query.

### Phase 3: Provider Adapter

- Add provider config.
- Add OpenAI-compatible adapter.
- Add timeout, cancellation, retry policy.
- Add redaction and local audit.

### Phase 4: Broader Control

- Add favorites/trash with stricter approval.
- Add job tools.
- Add UI navigation tools.
- Add semantic search tool.

## Open Questions

- Should the agent panel be global, or scoped to current view?
- Do we allow provider calls without explicit user opt-in?
- How much run history should be retained?
- Should agent-created albums be marked with provenance?
- What is the smallest useful preview sample size?
- Do we need a separate "draft album" state before creating a real album?

## Release Criteria

The first shipped version is ready only when:

- The agent can create an album from people/date filters end to end.
- Every write action has dry-run and approval.
- The agent cannot access original photo paths or bytes.
- Tool execution is test-covered without a real provider.
- Failed provider/tool calls leave the app state unchanged.
- The user can inspect what the agent is about to do before it does it.
