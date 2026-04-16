# Production Polish — Implementation Plan

Status: **ready to implement**

Three phases turning PhotoVault from a feature-complete app into a
fluid, keyboard-first, production-grade desktop application.

- **Phase A** — Make it feel solid (toasts, spinners, skeletons, state)
- **Phase B** — Make it keyboard-first (focus, shortcuts, navigation)
- **Phase C** — Polish destructive actions + visual consistency

Each phase is independently shippable. Commit after each.

---

# Phase A — Make it feel solid

Goal: app stops feeling like a dev build. Errors visible, loading
states animated, view switches smooth, last view restored.

## A1. Toast notification system

**New files:**
- `src/components/toast.rs`

**Modified files:**
- `src/app/state/mod.rs`
- `src/app/messages.rs`
- `src/app/handlers/mod.rs`
- `src/app/mod.rs` (subscription)
- `src/app/views.rs` (overlay rendering)

### A1.a Toast struct + state

In `src/components/toast.rs`:

```rust
//! Toast notification component for transient user feedback.

use std::time::SystemTime;
use iced::widget::{button, column, container, row, text, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::config::AppTheme;
use crate::theme::colors;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct ToastAction {
    pub label: String,
    pub message: Message,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub id: u64,
    pub kind: ToastKind,
    pub title: String,
    pub message: Option<String>,
    pub action: Option<ToastAction>,
    /// Unix millis when created. Used for auto-dismiss.
    pub created_at_ms: u128,
    /// Auto-dismiss after this many ms. 0 = sticky.
    pub ttl_ms: u128,
}

impl Toast {
    pub fn now_ms() -> u128 {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    }

    pub fn is_expired(&self) -> bool {
        if self.ttl_ms == 0 { return false; }
        Self::now_ms().saturating_sub(self.created_at_ms) > self.ttl_ms
    }

    pub fn success(title: impl Into<String>) -> Self { /* ttl 3000 */ }
    pub fn info(title: impl Into<String>) -> Self { /* ttl 3000 */ }
    pub fn warning(title: impl Into<String>) -> Self { /* ttl 5000 */ }
    pub fn error(title: impl Into<String>, msg: impl Into<String>) -> Self { /* ttl 6000 */ }

    pub fn with_action(mut self, label: impl Into<String>, msg: Message) -> Self {
        self.action = Some(ToastAction { label: label.into(), message: msg });
        self
    }

    pub fn sticky(mut self) -> Self {
        self.ttl_ms = 0;
        self
    }
}

/// Render the toast stack (bottom-right corner).
pub fn toast_stack(toasts: &[Toast], theme: AppTheme) -> Element<'static, Message> {
    let p = colors::palette(theme);
    let mut col = column![].spacing(8);
    for t in toasts.iter().take(5) {
        col = col.push(toast_card(t, p));
    }
    container(col)
        .padding(Padding::from([0, 24, 24, 0]))
        .align_x(iced::alignment::Horizontal::Right)
        .align_y(iced::alignment::Vertical::Bottom)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn toast_card(t: &Toast, p: colors::Palette) -> Element<'static, Message> {
    let accent = match t.kind {
        ToastKind::Success => p.semantic_success,
        ToastKind::Info => p.text_secondary,
        ToastKind::Warning => p.semantic_warning,
        ToastKind::Error => p.semantic_danger,
    };
    let title_text = text(t.title.clone()).size(13).color(p.text_primary);
    let mut col = column![title_text].spacing(4);
    if let Some(ref m) = t.message {
        col = col.push(text(m.clone()).size(11).color(p.text_secondary));
    }
    let mut card_row = row![
        // Left accent bar (3px wide)
        container(Space::new(Length::Fixed(3.0), Length::Fixed(40.0)))
            .style(move |_| container::Style {
                background: Some(accent.into()),
                ..Default::default()
            }),
        Space::with_width(10),
        col,
    ].align_y(Alignment::Start);

    if let Some(action) = &t.action {
        let action_msg = action.message.clone();
        card_row = card_row.push(Space::with_width(12)).push(
            button(text(action.label.clone()).size(11).color(p.text_primary))
                .padding([4, 10])
                .style(/* accent style */)
                .on_press(action_msg)
        );
    }

    let id = t.id;
    card_row = card_row.push(Space::with_width(8)).push(
        button(text("×").size(13).color(p.text_tertiary))
            .padding([4, 6])
            .on_press(Message::ToastDismiss(id))
    );

    container(card_row)
        .padding(12)
        .width(Length::Fixed(360.0))
        .style(/* elevated card with shadow-like border */)
        .into()
}
```

### A1.b State + Messages

In `src/app/state/mod.rs`:
```rust
// --- Toast notifications ---
pub(crate) toasts: Vec<crate::components::toast::Toast>,
pub(crate) toast_next_id: u64,
```

Initialize in `new()`:
```rust
toasts: Vec::new(),
toast_next_id: 0,
```

In `src/app/messages.rs`:
```rust
// --- Toasts ---
ToastShow(crate::components::toast::Toast),
ToastDismiss(u64),
ToastTick,  // periodic check for expired toasts
```

### A1.c Handlers

New file `src/app/handlers/toasts.rs`:
```rust
use iced::Task;
use crate::components::toast::Toast;
use super::super::messages::Message;
use super::super::state::PhotoVault;

pub(crate) fn show(app: &mut PhotoVault, mut toast: Toast) -> Task<Message> {
    toast.id = app.toast_next_id;
    app.toast_next_id = app.toast_next_id.wrapping_add(1);
    app.toasts.push(toast);
    // Keep at most 5
    if app.toasts.len() > 5 {
        app.toasts.remove(0);
    }
    Task::none()
}

pub(crate) fn dismiss(app: &mut PhotoVault, id: u64) -> Task<Message> {
    app.toasts.retain(|t| t.id != id);
    Task::none()
}

pub(crate) fn tick(app: &mut PhotoVault) -> Task<Message> {
    app.toasts.retain(|t| !t.is_expired());
    Task::none()
}
```

Wire in `src/app/handlers/mod.rs`.

### A1.d Subscription tick

In `src/app/mod.rs` `subscription()`, add:
```rust
if !self.toasts.is_empty() {
    subs.push(
        iced::time::every(std::time::Duration::from_millis(500))
            .map(|_| Message::ToastTick),
    );
}
```

### A1.e Render overlay

In `src/app/views.rs`, after the album picker overlay code (before the
final `main_row` assembly), add the toast stack as the topmost overlay:

```rust
let content = if !app.toasts.is_empty() {
    let toast_overlay = crate::components::toast::toast_stack(
        &app.toasts, app.config.theme,
    );
    iced::widget::stack![content, toast_overlay].into()
} else {
    content
};
```

### A1.f Wire all silent error paths

In every handler that currently does `tracing::error!` and returns
default, also push an error toast. Example pattern:

```rust
Err(e) => {
    tracing::error!("Failed to load X: {}", e);
    return super::handle(app, Message::ToastShow(
        Toast::error("Couldn't load X", "Try restarting the app or reopening the drive.")
    ));
}
```

Files to update (high-priority ones — the rest follow same pattern):
- `src/app/state/loaders.rs` — load_photos, load_face_clusters, load_albums, load_album_photos, load_documents, load_trash, load_suggestions, load_insights
- `src/app/handlers/scanning.rs` — select_drive failures
- `src/app/handlers/duplicates.rs`, `bursts.rs`, `faces.rs` — detection failures
- `src/app/handlers/trash.rs` — trash/restore/delete failures

**Checkpoint**: Toasts visible, errors no longer silent.

---

## A2. Loading spinner component

**New files:**
- `src/components/spinner.rs`

**Modified files:**
- `src/app/mod.rs` (subscription tick)
- `src/app/state/mod.rs` (spinner phase counter)
- `src/app/messages.rs` (SpinnerTick message)
- Various view files (replace text-only loading states)

### A2.a Spinner widget

In `src/components/spinner.rs`:
```rust
//! Animated loading spinner using braille glyphs.

use iced::widget::text;
use iced::Element;
use crate::app::Message;
use crate::theme::colors::Palette;

const FRAMES: &[&str] = &["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"];

pub fn spinner(phase: u32, p: Palette) -> Element<'static, Message> {
    let frame = FRAMES[(phase as usize) % FRAMES.len()];
    text(frame.to_string()).size(14).color(p.text_secondary).into()
}

pub fn spinner_with_label(phase: u32, label: &str, p: Palette) -> Element<'static, Message> {
    let frame = FRAMES[(phase as usize) % FRAMES.len()];
    iced::widget::row![
        text(frame.to_string()).size(14).color(p.text_secondary),
        iced::widget::Space::with_width(10),
        text(label.to_string()).size(13).color(p.text_secondary),
    ].align_y(iced::Alignment::Center).into()
}
```

### A2.b State + tick

In `src/app/state/mod.rs`:
```rust
pub(crate) spinner_phase: u32,
```

Initialize to `0`.

In `src/app/messages.rs`:
```rust
SpinnerTick,
```

In `src/app/mod.rs` subscription, add (always-on, lightweight):
```rust
// Spinner animation tick (only when something is loading)
if self.is_anything_loading() {
    subs.push(
        iced::time::every(std::time::Duration::from_millis(120))
            .map(|_| Message::SpinnerTick),
    );
}
```

Where `is_anything_loading()` is a new helper on PhotoVault:
```rust
pub(crate) fn is_anything_loading(&self) -> bool {
    self.search_loading
        || self.face_processing_active
        || self.duplicate_detection_running
        || self.burst_detection_running
        || self.document_analysis_active
        || self.suggestion_detection_running
        || self.insights_loading
        || self.scan_state.is_some()
}
```

Handler:
```rust
pub(crate) fn spinner_tick(app: &mut PhotoVault) -> Task<Message> {
    app.spinner_phase = app.spinner_phase.wrapping_add(1);
    Task::none()
}
```

### A2.c Replace text-only loading states

Wherever we have `text("Loading...")` or `text("Searching...")`, swap
for `spinner_with_label(app.spinner_phase, "Searching", p)`:

- `src/views/search.rs` — `loading_state()` function
- `src/views/insights.rs` — when data is None and loading
- `src/views/duplicates.rs` — detection running state
- `src/views/bursts.rs` — detection running state
- `src/views/people/grid.rs` — face processing status

**Checkpoint**: All loading states animate.

---

## A3. Skeleton screens for view switches

**Modified files:**
- `src/components/photo_grid.rs` (add skeleton helper)
- `src/views/timeline.rs`, `src/views/albums.rs`, `src/views/people/grid.rs`,
  `src/views/duplicates.rs`, `src/views/bursts.rs`, `src/views/insights.rs`

### A3.a Add per-view loading flag

Each view that loads data needs a `loading: bool` parameter.

For Timeline, we already have `app.photos`. Add `app.photos_loading: bool`
to state, set to true at start of `load_photos`, false on `PhotosLoaded`.

Same pattern for albums, faces, documents, trash, etc.

### A3.b Skeleton helper

In `src/components/photo_grid.rs`:
```rust
/// Render a grid of placeholder boxes for skeleton screens.
pub fn skeleton_grid(
    rows: usize,
    cols: usize,
    cell_size: f32,
    theme: AppTheme,
) -> Element<'static, Message> {
    let p = colors::palette(theme);
    let bg = p.bg_elevated;
    let mut row_els: Vec<Element<'static, Message>> = Vec::new();
    for _ in 0..rows {
        let mut cells: Vec<Element<'static, Message>> = Vec::new();
        for _ in 0..cols {
            cells.push(
                container(Space::new(Length::Fixed(cell_size), Length::Fixed(cell_size)))
                    .style(move |_t| container::Style {
                        background: Some(bg.into()),
                        border: iced::Border { radius: 6.0.into(), ..Default::default() },
                        ..Default::default()
                    })
                    .into()
            );
        }
        row_els.push(Row::with_children(cells).spacing(6).into());
    }
    Column::with_children(row_els).spacing(6).padding(Padding::from([0, 20])).into()
}
```

### A3.c Use in each view

In each grid view, when loading and no data:
```rust
if loading && photos.is_empty() {
    return skeleton_grid(4, columns, 160.0, theme);
}
```

For Albums: skeleton album cards (160x160 + name placeholder).
For People: skeleton circle avatars.
For Insights: skeleton stat cards.

**Checkpoint**: View switches show skeleton instead of blank.

---

## A4. Persist + restore last-viewed view

**Modified files:**
- `src/config/mod.rs`
- `src/app/handlers/scanning.rs`
- `src/app/state/mod.rs`

### A4.a Config field

In `src/config/mod.rs`:
```rust
#[serde(default)]
pub last_view: Option<String>,  // store as string for forward compat
```

### A4.b Update navigate_to to persist

In `src/app/handlers/scanning.rs`, in `navigate_to`, after setting
`current_view`, persist:

```rust
app.config.last_view = Some(view_to_string(&view));
let _ = app.config.save();
```

Helper:
```rust
fn view_to_string(v: &View) -> String {
    match v {
        View::Timeline => "timeline",
        View::Map => "map",
        View::Memories => "memories",
        View::Albums => "albums",
        View::Insights => "insights",
        View::Search => "search",
        View::People => "people",
        View::Documents => "documents",
        View::Duplicates => "duplicates",
        View::Bursts => "bursts",
        View::Trash => "trash",
        View::Settings => "settings",
        // Detail views and modal-ish ones don't get persisted
        _ => return "timeline",
    }.to_string()
}

fn string_to_view(s: &str) -> Option<View> {
    Some(match s {
        "timeline" => View::Timeline,
        "map" => View::Map,
        "memories" => View::Memories,
        "albums" => View::Albums,
        "insights" => View::Insights,
        "search" => View::Search,
        "people" => View::People,
        "documents" => View::Documents,
        "duplicates" => View::Duplicates,
        "bursts" => View::Bursts,
        "trash" => View::Trash,
        "settings" => View::Settings,
        _ => return None,
    })
}
```

### A4.c Restore on drive select

In `select_drive`, after photos loaded successfully, dispatch a navigate
to the saved view:
```rust
if let Some(saved) = app.config.last_view.as_ref().and_then(|s| string_to_view(s)) {
    if saved != View::Timeline {
        return Task::batch(vec![
            existing_tasks,
            Task::done(Message::NavigateTo(saved)),
        ]);
    }
}
```

**Checkpoint**: Reopens app to last view.

---

## A5. Phase A commit

```
git commit -m "Phase A: toasts, spinners, skeletons, last-view persistence

- Toast notification system with Success/Info/Warning/Error kinds,
  auto-dismiss TTL, optional Undo action button
- Animated braille spinner for all loading states (search, insights,
  detection, face processing)
- Skeleton screens for view switches (grids, cards)
- Persist + restore last-viewed view across app launches
- Replace silent error paths with user-visible toast notifications"
```

---

# Phase B — Make it keyboard-first

Goal: every interaction reachable from keyboard. Tab order works,
focus visible, `?` reveals shortcuts.

## B1. Focus management infrastructure

**Modified files:**
- `src/app/state/mod.rs`
- `src/app/messages.rs`
- `src/app/handlers/timeline.rs` (key handler)

### B1.a Focusable element identifier

In `src/app/state/mod.rs`:
```rust
/// Identifies an element that can hold keyboard focus.
/// Per-view; the FocusManager tracks current focus per view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FocusTarget {
    /// Sidebar nav button (index in nav order)
    SidebarNav(u8),
    /// The search input
    SearchInput,
    /// A button by stable name (e.g. "delete-album", "rename-album")
    Named(&'static str),
    /// A grid cell by index
    GridCell(usize),
}
```

State field:
```rust
pub(crate) focus: Option<FocusTarget>,
/// Per-view focus order (rebuilt by each view function on render).
/// Tab cycles through this list.
pub(crate) focus_order: Vec<FocusTarget>,
```

### B1.b Focus messages

```rust
// --- Focus management ---
FocusSet(FocusTarget),
FocusClear,
FocusNext,
FocusPrev,
```

Handler:
```rust
pub(crate) fn focus_set(app: &mut PhotoVault, t: FocusTarget) -> Task<Message> {
    app.focus = Some(t);
    Task::none()
}

pub(crate) fn focus_next(app: &mut PhotoVault) -> Task<Message> {
    if app.focus_order.is_empty() { return Task::none(); }
    let next = match app.focus {
        None => app.focus_order[0],
        Some(cur) => {
            let i = app.focus_order.iter().position(|t| *t == cur).unwrap_or(0);
            app.focus_order[(i + 1) % app.focus_order.len()]
        }
    };
    app.focus = Some(next);
    // If it's the search input, also call iced::widget::text_input::focus
    Task::none()
}
// focus_prev mirrors
```

### B1.c Tab/Shift+Tab in key handler

In `key_pressed`, before the view-specific arms, add:

```rust
// Tab navigation (works in any non-text-input context)
if let keyboard::Key::Named(keyboard::key::Named::Tab) = key {
    // Note: text_input captures Tab natively for moving between inputs.
    // Our Tab routing handles non-input focus.
    let modifiers = /* would need to track via event::listen_with */;
    return super::handle(app, Message::FocusNext);
}
```

Actually, since `event::listen_with` only gives us the key (no
modifiers in current setup), we'd need to extend the listener to
capture modifiers. Update in `src/app/mod.rs`:

```rust
iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
    Some(Message::KeyPressed(key, modifiers))
}
```

Update `Message::KeyPressed` to carry modifiers:
```rust
KeyPressed(keyboard::Key, keyboard::Modifiers),
```

Then in handler:
```rust
if let keyboard::Key::Named(keyboard::key::Named::Tab) = key {
    if modifiers.shift() {
        return super::handle(app, Message::FocusPrev);
    }
    return super::handle(app, Message::FocusNext);
}
```

### B1.d Focus border styling helper

In `src/theme/colors.rs` or a new `src/components/focus.rs`:

```rust
/// Wrap an element with a focus indicator (2px accent border when focused).
pub fn focusable<'a>(
    el: Element<'a, Message>,
    target: FocusTarget,
    current_focus: Option<FocusTarget>,
    theme: AppTheme,
) -> Element<'a, Message> {
    let p = colors::palette(theme);
    let is_focused = current_focus == Some(target);
    container(el)
        .style(move |_t| container::Style {
            border: iced::Border {
                color: if is_focused { p.accent_primary } else { iced::Color::TRANSPARENT },
                width: 2.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .padding(2)
        .into()
}
```

Apply to sidebar buttons, action bar buttons, etc.

**Checkpoint**: Tab cycles focus, ring visible.

---

## B2. Sidebar keyboard reachable

**Modified files:**
- `src/components/sidebar.rs`

Wrap each nav button with `focusable()`. Build `focus_order` based on
current view:

```rust
// In sidebar nav, ordered list of FocusTargets:
// SidebarNav(0) Timeline, SidebarNav(1) Map, ..., then SearchInput, then Named buttons

let order = vec![
    FocusTarget::SidebarNav(0), // Timeline
    FocusTarget::SidebarNav(1), // Map
    // ...
];
```

This `focus_order` is set on each render in `app/views.rs`:
```rust
app.focus_order = build_focus_order_for(&app.current_view, app);
```

But `app` is `&PhotoVault` in `view()` — we can't mutate. So instead,
maintain `focus_order` via the navigate handler whenever view changes.

In `navigate_to`:
```rust
app.focus_order = focus_order_for(&view);
app.focus = app.focus_order.first().copied();
```

`focus_order_for` returns the per-view focus list.

**Checkpoint**: Tab from any view cycles through sidebar + view actions.

---

## B3. Standard keyboard shortcuts

**Modified files:**
- `src/app/handlers/timeline.rs` (key_pressed, global section)

Add to the global shortcuts section:

```rust
// --- Global shortcuts (Ctrl/Cmd modified) ---
let cmd = modifiers.control() || modifiers.command();
if cmd {
    match key {
        keyboard::Key::Character(ref ch) => {
            let lower = ch.to_lowercase();
            // Cmd+, → Settings
            if ch == "," {
                return super::handle(app, Message::NavigateTo(View::Settings));
            }
            // Cmd+Z → undo (context-aware)
            if lower == "z" {
                return super::handle(app, Message::UndoLastAction);
            }
            // Cmd+W → close current detail view
            if lower == "w" {
                return close_current_detail(app);
            }
            // Cmd+1..9 → sidebar nav by index
            if let Ok(n) = ch.parse::<usize>() {
                if (1..=9).contains(&n) {
                    return navigate_by_index(app, n - 1);
                }
            }
        }
        _ => {}
    }
}

// '?' → show keyboard shortcuts overlay
if let keyboard::Key::Character(ref ch) = key {
    if ch == "?" && app.editing_cluster_id.is_none() && app.editing_album_id.is_none() {
        return super::handle(app, Message::ToggleShortcutsOverlay);
    }
}
```

`navigate_by_index(0)` → Timeline, `(1)` → Map, etc. (define order).

`close_current_detail`:
```rust
match app.current_view {
    View::PhotoDetail => super::handle(app, Message::ClosePhotoDetail),
    View::AlbumDetail => super::handle(app, Message::BackToAlbums),
    View::ClusterDetail => super::handle(app, Message::BackToPeople),
    View::MemoryDetail => super::handle(app, Message::CloseMemoryDetail),
    View::DuplicateDetail => super::handle(app, Message::CloseDuplicateDetail),
    View::BurstDetail => super::handle(app, Message::CloseBurstDetail),
    _ => Task::none(),
}
```

`UndoLastAction` — context-aware undo:
```rust
match app.current_view {
    View::Cull => super::handle(app, Message::CullUndo),
    View::FaceReview => super::handle(app, Message::FaceReviewUndo),
    _ => {
        // Show toast: "Nothing to undo here"
        Task::none()
    }
}
```

**Checkpoint**: All standard shortcuts work.

---

## B4. Grid arrow navigation in Timeline

**Modified files:**
- `src/app/state/mod.rs` (highlighted cell index)
- `src/app/handlers/timeline.rs`
- `src/components/photo_grid.rs` (render highlight)

### B4.a State

```rust
/// Currently highlighted photo in the timeline grid (keyboard cursor).
/// Distinct from selection (which is for multi-select operations).
pub(crate) timeline_highlight_index: Option<usize>,
```

### B4.b Arrow handlers in key_pressed for Timeline

```rust
} else if app.current_view == View::Timeline {
    let cols = Self::timeline_columns_for_width(app.window_width);
    match key {
        keyboard::Key::Named(keyboard::key::Named::ArrowRight) => {
            return move_highlight(app, 1, cols);
        }
        keyboard::Key::Named(keyboard::key::Named::ArrowLeft) => {
            return move_highlight(app, -1, cols);
        }
        keyboard::Key::Named(keyboard::key::Named::ArrowDown) => {
            return move_highlight(app, cols as i32, cols);
        }
        keyboard::Key::Named(keyboard::key::Named::ArrowUp) => {
            return move_highlight(app, -(cols as i32), cols);
        }
        keyboard::Key::Named(keyboard::key::Named::Enter) => {
            if let Some(i) = app.timeline_highlight_index {
                if let Some(photo) = app.photos.get(i) {
                    return super::handle(app, Message::SelectPhoto(photo.id));
                }
            }
        }
        keyboard::Key::Named(keyboard::key::Named::Space) => {
            // Toggle selection of highlighted photo
            if let Some(i) = app.timeline_highlight_index {
                if let Some(photo) = app.photos.get(i) {
                    return super::handle(app, Message::ToggleTimelinePhotoSelection(photo.id));
                }
            }
        }
        // ... existing Delete/Escape/etc
        _ => {}
    }
}
```

### B4.c Render highlight

In `photo_grid.rs`, the `photo_card` already takes `hovered_photo_id`.
Add a third state: `highlighted_photo_id`. Render same border as
hovered but in accent color (more prominent).

Pass `app.timeline_highlight_index.and_then(|i| app.photos.get(i)).map(|p| p.id)`
through.

**Checkpoint**: Arrow keys navigate timeline grid.

---

## B5. `?` keyboard shortcuts overlay

**New files:**
- `src/views/shortcuts.rs`
- `src/app/handlers/shortcuts.rs` (or fold into existing)

**Modified files:**
- `src/app/state/mod.rs` (overlay flag)
- `src/app/messages.rs` (toggle message)
- `src/app/views.rs` (overlay rendering)

### B5.a State

```rust
pub(crate) shortcuts_overlay_open: bool,
```

### B5.b Message + handler

```rust
ToggleShortcutsOverlay,
```

```rust
pub(crate) fn toggle_shortcuts_overlay(app: &mut PhotoVault) -> Task<Message> {
    app.shortcuts_overlay_open = !app.shortcuts_overlay_open;
    Task::none()
}
```

Esc closes it (handle in key_pressed: if overlay open and Esc pressed,
close the overlay first instead of falling through to view's Esc).

### B5.c Shortcuts data model

In `src/views/shortcuts.rs`:
```rust
pub struct ShortcutGroup {
    pub title: &'static str,
    pub items: Vec<(&'static str, &'static str)>,  // (keys, description)
}

pub fn shortcuts_for(view: &View) -> Vec<ShortcutGroup> {
    let global = ShortcutGroup {
        title: "Global",
        items: vec![
            ("?", "Show this help"),
            ("/  or  F", "Jump to Search"),
            ("[", "Toggle sidebar"),
            ("Tab  /  Shift+Tab", "Move focus"),
            ("Cmd+,", "Open Settings"),
            ("Cmd+1-9", "Jump to sidebar item"),
            ("Cmd+W", "Close current view"),
            ("Cmd+Z", "Undo last action (where supported)"),
            ("Esc", "Cancel / go back"),
        ],
    };

    let view_specific = match view {
        View::Timeline => ShortcutGroup {
            title: "Timeline",
            items: vec![
                ("← → ↑ ↓", "Move grid cursor"),
                ("Enter", "Open highlighted photo"),
                ("Space", "Toggle selection"),
                ("Delete", "Trash selected"),
            ],
        },
        View::PhotoDetail => ShortcutGroup {
            title: "Photo Detail",
            items: vec![
                ("← →", "Previous / next photo"),
                ("R", "Rotate"),
                ("I", "Toggle metadata panel"),
                ("Delete", "Trash photo"),
                ("Esc", "Close"),
            ],
        },
        View::Cull => ShortcutGroup {
            title: "Cull",
            items: vec![
                ("← →", "Previous / next"),
                ("X", "Toggle trash mark"),
                ("U", "Undo"),
                ("Enter", "Finish"),
            ],
        },
        // ... per-view shortcuts
        _ => ShortcutGroup { title: "View", items: vec![] },
    };

    vec![global, view_specific]
}

pub fn shortcuts_overlay(view: &View, theme: AppTheme) -> Element<'static, Message> {
    let p = colors::palette(theme);
    let groups = shortcuts_for(view);

    let mut col = column![
        row![
            text("Keyboard Shortcuts").size(20).color(p.text_primary),
            Space::with_width(Length::Fill),
            button(text("×").size(16).color(p.text_secondary))
                .on_press(Message::ToggleShortcutsOverlay),
        ].align_y(Alignment::Center),
        Space::with_height(16),
    ].spacing(8);

    for group in groups {
        if group.items.is_empty() { continue; }
        col = col.push(text(group.title).size(13).color(p.text_secondary));
        col = col.push(Space::with_height(4));
        for (keys, desc) in group.items {
            col = col.push(
                row![
                    container(text(keys).size(12).color(p.text_primary))
                        .width(Length::Fixed(180.0)),
                    text(desc).size(12).color(p.text_secondary),
                ]
            );
        }
        col = col.push(Space::with_height(12));
    }

    let panel = container(col)
        .padding(24)
        .width(Length::Fixed(560.0))
        .style(/* elevated card */);

    let backdrop = mouse_area(
        container(Space::new(Length::Fill, Length::Fill))
            .style(/* semi-transparent */)
    ).on_press(Message::ToggleShortcutsOverlay);

    iced::widget::stack![
        backdrop,
        container(panel)
            .width(Length::Fill).height(Length::Fill)
            .center_x(Length::Fill).center_y(Length::Fill)
    ].into()
}
```

### B5.d Wire in app/views.rs

After all other overlays:
```rust
let content = if app.shortcuts_overlay_open {
    let overlay = crate::views::shortcuts::shortcuts_overlay(
        &app.current_view, app.config.theme,
    );
    iced::widget::stack![content, overlay].into()
} else {
    content
};
```

### B5.e First-launch hint

On first app launch (config has `shortcuts_hint_shown: bool`), show a
toast: "Press ? for keyboard shortcuts" once.

**Checkpoint**: `?` opens overlay, shows view-specific shortcuts.

---

## B6. Settings → Keyboard Shortcuts page

**Modified files:**
- `src/views/settings.rs`

Add a "Keyboard Shortcuts" section that renders the same shortcut data
as the overlay, but inline in Settings. This is the canonical reference.

Use the same `shortcuts_for(View::Timeline)` style listing, just
showing all groups instead of context-specific.

**Checkpoint**: Settings has shortcuts reference.

---

## B7. Phase B commit

```
git commit -m "Phase B: keyboard-first navigation + ? shortcuts overlay

- Focus management: Tab/Shift+Tab cycles focus, visible accent border
- Sidebar nav reachable via Tab and Cmd+1-9
- Standard shortcuts: Cmd+, (Settings), Cmd+Z (undo), Cmd+W (close)
- Timeline grid: arrow keys move cursor, Enter opens, Space selects
- ? key opens contextual shortcuts overlay for current view
- Settings: dedicated Keyboard Shortcuts reference page
- First-launch toast hints at ? for discoverability"
```

---

# Phase C — Polish destructive actions + visual consistency

Goal: confirm dangerous actions, undo where missing, fix empty states,
extract reusable components.

## C1. Unified confirmation pattern

**Modified files:**
- `src/app/state/mod.rs`
- `src/app/messages.rs`
- `src/app/handlers/mod.rs` (or new file)
- Various views (use new pattern)

### C1.a State

Replace existing per-action confirm flags with one unified state:

```rust
#[derive(Debug, Clone)]
pub enum PendingConfirmation {
    DeleteAlbum(i64),
    EmptyTrash,
    PermanentlyDeletePhoto(i64),
    MergeClusters { source: i64, target: i64 },
    DeletePhoto(i64),  // for non-trash flows
}

pub(crate) pending_confirmation: Option<PendingConfirmation>,
```

Remove old fields (or migrate them to use this enum):
- `confirm_empty_trash: bool` → use `PendingConfirmation::EmptyTrash`
- `confirm_delete_photo_id: Option<i64>` → use `PendingConfirmation::PermanentlyDeletePhoto`

### C1.b Messages

```rust
RequestConfirmation(PendingConfirmation),
ConfirmPending,
CancelPending,
```

### C1.c Handlers

```rust
pub(crate) fn request_confirmation(app: &mut PhotoVault, c: PendingConfirmation) -> Task<Message> {
    app.pending_confirmation = Some(c);
    Task::none()
}

pub(crate) fn confirm_pending(app: &mut PhotoVault) -> Task<Message> {
    let Some(c) = app.pending_confirmation.take() else { return Task::none(); };
    match c {
        PendingConfirmation::DeleteAlbum(id) => super::handle(app, Message::DeleteAlbum(id)),
        PendingConfirmation::EmptyTrash => super::handle(app, Message::ConfirmEmptyTrash),
        PendingConfirmation::PermanentlyDeletePhoto(id) =>
            super::handle(app, Message::ConfirmPermanentlyDeletePhoto(id)),
        PendingConfirmation::MergeClusters { source, target } => {
            // existing merge logic
            Task::none()
        }
        PendingConfirmation::DeletePhoto(id) =>
            super::handle(app, Message::TrashPhotos(vec![id])),
    }
}

pub(crate) fn cancel_pending(app: &mut PhotoVault) -> Task<Message> {
    app.pending_confirmation = None;
    Task::none()
}
```

### C1.d Render confirm dialog

New file `src/components/confirm.rs`:
```rust
pub fn confirm_overlay(c: &PendingConfirmation, theme: AppTheme) -> Element<'static, Message> {
    let p = colors::palette(theme);
    let (title, body, action_label) = match c {
        PendingConfirmation::DeleteAlbum(_) => (
            "Delete this album?",
            "Photos will not be deleted, only the album itself.",
            "Delete album",
        ),
        PendingConfirmation::EmptyTrash => (
            "Empty trash?",
            "All photos in trash will be permanently deleted from disk.",
            "Empty trash",
        ),
        // ...
    };
    // Modal card with title, body text, [Cancel] [destructive button]
    // Esc and backdrop click → CancelPending
    // Enter → ConfirmPending
}
```

### C1.e Wire in views

Replace direct `Delete` button on_press with:
```rust
.on_press(Message::RequestConfirmation(PendingConfirmation::DeleteAlbum(album_id)))
```

Update places: `src/views/albums.rs` (delete album, delete photo from
album), `src/views/photo_detail.rs` (delete photo), `src/views/trash.rs`
(empty trash, permanent delete), `src/views/people/detail.rs` (merge
confirmation).

### C1.f Render in app/views.rs

Layer above all content:
```rust
let content = if let Some(ref pending) = app.pending_confirmation {
    let overlay = crate::components::confirm::confirm_overlay(pending, app.config.theme);
    iced::widget::stack![content, overlay].into()
} else {
    content
};
```

### C1.g Esc + Enter handling

In key_pressed, when `pending_confirmation.is_some()`:
- Esc → `CancelPending`
- Enter → `ConfirmPending`

**Checkpoint**: Destructive actions consistent, keyboard-cancellable.

---

## C2. Undo toast for Timeline trash

**Modified files:**
- `src/app/handlers/trash.rs`
- `src/app/messages.rs` (UndoTrash message if not present)

After `TrashPhotos` succeeds:
```rust
let count = ids.len();
let undo_ids = ids.clone();
return Task::batch(vec![
    /* existing trash logic */,
    super::handle(app, Message::ToastShow(
        Toast::success(format!("Trashed {} photo{}", count, if count == 1 { "" } else { "s" }))
            .with_action("Undo", Message::RestorePhotos(undo_ids))
    )),
]);
```

Add `RestorePhotos(Vec<i64>)` if not present (extend existing
`RestorePhoto(i64)` to handle multiple).

**Checkpoint**: Trash from Timeline shows undo toast.

---

## C3. Missing empty states

**Modified files:**
- `src/views/trash.rs`
- `src/views/people/grid.rs`

### C3.a Trash empty state

In `src/views/trash.rs`, when `items.is_empty()` and `stats.count == 0`:
```rust
container(
    column![
        text("Trash is empty").size(16).color(p.text_secondary),
        Space::with_height(8),
        text("Deleted photos appear here for 30 days before permanent removal.")
            .size(13).color(p.text_tertiary),
    ].align_x(Alignment::Center),
)
.width(Length::Fill)
.padding(Padding::from([80, 0]))
.center_x(Length::Fill)
```

### C3.b People empty state

In `src/views/people/grid.rs`, when `clusters.is_empty()` and not
processing:
```rust
column![
    text("No people detected yet").size(16).color(p.text_secondary),
    Space::with_height(8),
    text("Run face processing to find people in your photos.")
        .size(13).color(p.text_tertiary),
    Space::with_height(16),
    button(text("Process faces").size(13))
        .on_press(Message::ProcessFaces),
].align_x(Alignment::Center)
```

**Checkpoint**: Empty states consistent.

---

## C4. Tooltips on settings sliders

**Modified files:**
- `src/views/settings.rs`

Use `iced::widget::tooltip`:
```rust
use iced::widget::tooltip;

let face_confidence_slider = tooltip(
    slider(...)
        .on_change(Message::SetFaceConfidence),
    "Higher values reduce false positives but may miss real faces. Default: 0.25",
    tooltip::Position::Right,
).style(/* styled */);
```

Apply to all sliders (face confidence, clustering threshold, burst
window, trash auto-delete).

Also tooltip the toolbar buttons in photo detail (Rotate, Info, Album,
Delete, Close) for clarity.

**Checkpoint**: Settings have inline help tooltips.

---

## C5. Persistent status bar

**Modified files:**
- `src/app/views.rs` (status bar already exists, expand it)

The existing status bar at the bottom of `app/views.rs` shows scan,
face processing, etc. Make sure it always shows when ANY background
op is active, including:
- Suggestion detection (`suggestion_detection_running`)
- Insights loading (`insights_loading`)
- Search loading (`search_loading`)

Each shown as a separate small label with spinner.

Also: when no ops active, show last-completed message briefly:
"Indexed 4,832 photos" for 5 seconds via toast pattern.

**Checkpoint**: Status bar always informative.

---

## C6. Shared UI components

**New files:**
- `src/components/ui.rs`

Extract repeated patterns:

```rust
//! Shared UI primitives — buttons, headers, empty states.

pub fn primary_button(label: &str, msg: Message, theme: AppTheme) -> Button<'static, Message> {
    // Amber bg, white text, hover brighter
}

pub fn secondary_button(label: &str, msg: Message, theme: AppTheme) -> Button<'static, Message> {
    // Bordered, transparent bg, hover bg_hover
}

pub fn danger_button(label: &str, msg: Message, theme: AppTheme) -> Button<'static, Message> {
    // Red text, transparent bg, hover red faint bg
}

pub fn page_title(label: &str, theme: AppTheme) -> Element<'static, Message> {
    text(label.to_owned()).size(28).color(/* primary */).into()
}

pub fn section_header(label: &str, theme: AppTheme) -> Element<'static, Message> {
    text(label.to_owned()).size(14).color(/* secondary */).into()
}

pub fn empty_state(
    title: &str,
    description: &str,
    action: Option<(&str, Message)>,
    theme: AppTheme,
) -> Element<'static, Message> {
    // Padded, centered, optional action button
}

pub fn icon_button(symbol: &str, msg: Message, theme: AppTheme) -> Button<'static, Message> {
    // For ×, ‹, › etc - small square button
}
```

Then refactor existing views to use these. Do this opportunistically —
not every button needs migration in one sweep. Start with most-repeated
patterns.

**Checkpoint**: Visual consistency improves over time.

---

## C7. Phase C commit

```
git commit -m "Phase C: confirm dialogs, undo toasts, empty states, polish

- Unified confirmation overlay for destructive actions (album delete,
  trash empty, permanent delete, cluster merge); Esc cancels, Enter confirms
- Undo toast after Timeline trash with 5s window
- Empty states added for Trash and People views
- Tooltips on settings sliders and obscure toolbar buttons
- Status bar shows all background ops including suggestions and insights
- Shared UI components: primary_button, page_title, section_header,
  empty_state, icon_button — refactor opportunistically"
```

---

# Cross-cutting concerns

## Critical iced rules (still apply)
- NEVER `height(Length::Fill)` inside scrollable
- Use `container` (not `button`) for cards with interactive children
- Empty states use padding for vertical centering, not Fill height

## Performance considerations
- Toast tick: 500ms is plenty (don't go to 16ms — wasteful)
- Spinner tick: 120ms gives smooth animation without burning CPU
- Skeleton screens: don't allocate huge grids — render only what's
  visible (e.g., 8x4 = 32 cells max)
- Focus order: rebuilt on navigate, not on every render
- Confirmation overlay: only renders when `pending_confirmation.is_some()`

## What this plan explicitly does NOT cover

- Drag-and-drop (Iced 0.13 limitation, would need custom event handling)
- Right-click context menus (Iced 0.13 limitation)
- File menu / native menu bar (not built into Iced 0.13)
- Multi-window support (one-window apps only)
- High-contrast theme variant (defer)
- Font size adjustability (defer)
- Page transition animations (Iced lacks tweening — would need custom tick-based animation per element)

## Implementation sequence

Execute the three phases in order. Each phase is a single commit
with multiple sub-steps. Within a phase, the sub-steps can be done in
the listed order; each is a compilable checkpoint.

Estimated total scope: ~1500 LOC new + refactors. The biggest single
piece is the toast system (Phase A1), the rest are smaller incremental
improvements.

After all three phases land, the app should feel like a finished
desktop product: errors visible, navigation discoverable, dangerous
actions protected, everything keyboard-reachable.
