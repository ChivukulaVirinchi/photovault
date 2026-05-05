//! Face review deck: interactive "same person?" confirmations.
//!
//! Each user decision updates galleries (sticky user_confirmed members) or
//! records cluster_cannot_merge constraints. Resolves the face_review_queue
//! entry so it doesn't resurface on the next scan.

use iced::Task;

use crate::db::{Database, FaceRepo, ReviewItem};

use super::super::messages::Message;
use super::super::state::{FaceReviewState, PhotoVault, ReviewDecision, View};

const REVIEW_BATCH: usize = 30;

pub(crate) fn enter(app: &mut PhotoVault) -> Task<Message> {
    let Some(drive_path) = app.selected_drive.clone() else {
        return Task::none();
    };

    app.current_view = View::FaceReview;

    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || load_queue(&drive_path))
                .await
                .unwrap_or_default()
        },
        Message::FaceReviewLoaded,
    )
}

fn load_queue(drive_path: &std::path::Path) -> Vec<ReviewItem> {
    let db = match Database::open_for_drive(drive_path) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("face_review load_queue: open db failed: {}", e);
            return Vec::new();
        }
    };
    let repo = FaceRepo::new(&db.conn);
    repo.get_review_queue_items(REVIEW_BATCH)
        .unwrap_or_default()
}

pub(crate) fn loaded(app: &mut PhotoVault, items: Vec<ReviewItem>) -> Task<Message> {
    app.face_review_pending = items.len() as i64;
    app.face_review_state = Some(FaceReviewState::new(items));
    Task::none()
}

pub(crate) fn same(app: &mut PhotoVault) -> Task<Message> {
    let Some(drive_path) = app.selected_drive.clone() else {
        return Task::none();
    };
    let queue_id = match current_queue_id(app) {
        Some(id) => id,
        None => return Task::none(),
    };

    std::thread::spawn(move || {
        if let Err(e) = apply_same(&drive_path, queue_id) {
            tracing::warn!(
                "resolve_review_same failed for queue_id={}: {}",
                queue_id,
                e
            );
        }
    });

    if let Some(state) = app.face_review_state.as_mut() {
        state.confirmed += 1;
        state.undo_stack.push((queue_id, ReviewDecision::Same));
        state.advance();
    }
    // Optimistic badge update — the background thread above will
    // resolve the queue row, but the user sees the count drop now
    // rather than waiting for `finish` to run a recount.
    app.face_review_pending = (app.face_review_pending - 1).max(0);
    Task::none()
}

pub(crate) fn different(app: &mut PhotoVault) -> Task<Message> {
    let Some(drive_path) = app.selected_drive.clone() else {
        return Task::none();
    };
    let queue_id = match current_queue_id(app) {
        Some(id) => id,
        None => return Task::none(),
    };

    std::thread::spawn(move || {
        if let Err(e) = apply_different(&drive_path, queue_id) {
            tracing::warn!(
                "resolve_review_different failed for queue_id={}: {}",
                queue_id,
                e
            );
        }
    });

    if let Some(state) = app.face_review_state.as_mut() {
        state.rejected += 1;
        state.undo_stack.push((queue_id, ReviewDecision::Different));
        state.advance();
    }
    app.face_review_pending = (app.face_review_pending - 1).max(0);
    Task::none()
}

pub(crate) fn skip(app: &mut PhotoVault) -> Task<Message> {
    let Some(drive_path) = app.selected_drive.clone() else {
        return Task::none();
    };
    let queue_id = match current_queue_id(app) {
        Some(id) => id,
        None => return Task::none(),
    };

    std::thread::spawn(move || {
        if let Err(e) = apply_skip(&drive_path, queue_id) {
            tracing::warn!(
                "resolve_review_skip failed for queue_id={}: {}",
                queue_id,
                e
            );
        }
    });

    if let Some(state) = app.face_review_state.as_mut() {
        state.skipped += 1;
        state.undo_stack.push((queue_id, ReviewDecision::Skip));
        state.advance();
    }
    app.face_review_pending = (app.face_review_pending - 1).max(0);
    Task::none()
}

pub(crate) fn undo(app: &mut PhotoVault) -> Task<Message> {
    let Some(drive_path) = app.selected_drive.clone() else {
        return Task::none();
    };
    let Some(state) = app.face_review_state.as_mut() else {
        return Task::none();
    };
    let Some((queue_id, decision)) = state.undo_stack.pop() else {
        return Task::none();
    };

    match decision {
        ReviewDecision::Same => state.confirmed = state.confirmed.saturating_sub(1),
        ReviewDecision::Different => state.rejected = state.rejected.saturating_sub(1),
        ReviewDecision::Skip => state.skipped = state.skipped.saturating_sub(1),
    }
    state.current_index = state.current_index.saturating_sub(1);

    std::thread::spawn(move || {
        if let Err(e) = apply_unresolve(&drive_path, queue_id) {
            tracing::warn!("unresolve_review failed for queue_id={}: {}", queue_id, e);
        }
    });
    // Mirror the optimistic decrement from same/different/skip —
    // undo'ing a resolution should bump the badge back up.
    app.face_review_pending += 1;

    Task::none()
}

pub(crate) fn finish(app: &mut PhotoVault) -> Task<Message> {
    app.face_review_state = None;
    app.current_view = View::People;
    refresh_pending(app);
    Task::none()
}

fn current_queue_id(app: &PhotoVault) -> Option<i64> {
    app.face_review_state
        .as_ref()
        .and_then(|s| s.current().map(|item| item.queue_id))
}

fn apply_same(drive_path: &std::path::Path, queue_id: i64) -> Result<(), String> {
    let db = open_db(drive_path)?;
    FaceRepo::new(&db.conn)
        .resolve_review_same(queue_id)
        .map_err(|e| e.to_string())
}

fn apply_different(drive_path: &std::path::Path, queue_id: i64) -> Result<(), String> {
    let db = open_db(drive_path)?;
    FaceRepo::new(&db.conn)
        .resolve_review_different(queue_id)
        .map_err(|e| e.to_string())
}

fn apply_skip(drive_path: &std::path::Path, queue_id: i64) -> Result<(), String> {
    let db = open_db(drive_path)?;
    FaceRepo::new(&db.conn)
        .resolve_review_skip(queue_id)
        .map_err(|e| e.to_string())
}

fn apply_unresolve(drive_path: &std::path::Path, queue_id: i64) -> Result<(), String> {
    let db = open_db(drive_path)?;
    FaceRepo::new(&db.conn)
        .unresolve_review(queue_id)
        .map_err(|e| e.to_string())
}

fn open_db(drive_path: &std::path::Path) -> Result<Database, String> {
    Database::open_for_drive(drive_path).map_err(|e| e.to_string())
}

fn refresh_pending(app: &mut PhotoVault) {
    let Some(ref drive_path) = app.selected_drive else {
        return;
    };
    if let Ok(db) = Database::open_for_drive(drive_path) {
        if let Ok(n) = FaceRepo::new(&db.conn).review_queue_size() {
            app.face_review_pending = n;
        }
    }
}
