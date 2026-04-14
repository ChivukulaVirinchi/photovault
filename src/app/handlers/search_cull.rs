//! Search and cull-mode handlers.

use iced::Task;

use crate::db::Database;
use crate::services::SearchService;
use crate::views::CullState;

use super::super::messages::Message;
use super::super::state::{PhotoVault, View};

pub(crate) fn search_input_changed(app: &mut PhotoVault, input: String) -> Task<Message> {
    app.search_query = input.clone();

    if let Some(ref drive_path) = app.selected_drive {
        let drive_path = drive_path.clone();
        return Task::perform(
            async move {
                if input.trim().is_empty() {
                    return Vec::new();
                }
                match Database::open_for_drive(&drive_path) {
                    Ok(db) => SearchService::get_suggestions(&db.conn, &input)
                        .unwrap_or_default(),
                    Err(_) => Vec::new(),
                }
            },
            Message::SearchSuggestionsLoaded,
        );
    }

    Task::none()
}

pub(crate) fn search_suggestion_selected(app: &mut PhotoVault, value: String) -> Task<Message> {
    app.search_query = value;
    super::handle(app, Message::ExecuteSearch)
}

pub(crate) fn search_suggestions_loaded(
    app: &mut PhotoVault,
    suggestions: Vec<String>,
) -> Task<Message> {
    app.search_suggestions = suggestions;
    Task::none()
}

pub(crate) fn execute_search(app: &mut PhotoVault) -> Task<Message> {
    let Some(ref drive_path) = app.selected_drive else {
        return Task::none();
    };

    let query_text = app.search_query.clone();
    let drive_path = drive_path.clone();
    app.search_loading = true;

    Task::perform(
        async move {
            match Database::open_for_drive(&drive_path) {
                Ok(db) => {
                    let parsed = crate::search::QueryParser::parse(&query_text);
                    let rows = SearchService::search(&db.conn, &parsed)
                        .unwrap_or_default();
                    let ids = rows.iter().map(|r| r.photo_id).collect::<Vec<_>>();
                    let groups = SearchService::group_by_date(rows);
                    (groups, ids)
                }
                Err(_) => (Vec::new(), Vec::new()),
            }
        },
        |(groups, ids)| Message::SearchComplete(groups, ids),
    )
}

pub(crate) fn search_complete(
    app: &mut PhotoVault,
    groups: Vec<crate::services::SearchResultGroup>,
    ids: Vec<i64>,
) -> Task<Message> {
    app.search_loading = false;
    app.search_results = Some(groups);
    app.search_result_photo_ids = ids;
    Task::none()
}

pub(crate) fn enter_cull_from_search(app: &mut PhotoVault) -> Task<Message> {
    if app.search_result_photo_ids.is_empty() {
        return Task::none();
    }
    super::handle(app, Message::EnterCullMode(app.search_result_photo_ids.clone()))
}

pub(crate) fn enter_cull_mode(app: &mut PhotoVault, photo_ids: Vec<i64>) -> Task<Message> {
    if photo_ids.is_empty() {
        return Task::none();
    }
    app.cull_return_view = Some(app.current_view.clone());
    app.cull_state = Some(CullState::new(photo_ids));
    app.cull_confirm_pending = false;
    app.current_view = View::Cull;
    Task::none()
}

pub(crate) fn exit_cull_mode(app: &mut PhotoVault) -> Task<Message> {
    app.cull_state = None;
    app.cull_confirm_pending = false;
    app.current_view = app.cull_return_view.take().unwrap_or(View::Timeline);
    Task::none()
}

pub(crate) fn cull_next(app: &mut PhotoVault) -> Task<Message> {
    if let Some(ref mut cull) = app.cull_state {
        cull.next();
        app.cull_confirm_pending = false;
    }
    Task::none()
}

pub(crate) fn cull_prev(app: &mut PhotoVault) -> Task<Message> {
    if let Some(ref mut cull) = app.cull_state {
        cull.prev();
        app.cull_confirm_pending = false;
    }
    Task::none()
}

pub(crate) fn cull_toggle_trash(app: &mut PhotoVault) -> Task<Message> {
    if let Some(ref mut cull) = app.cull_state {
        cull.toggle_trash();
        app.cull_confirm_pending = false;
    }
    Task::none()
}

pub(crate) fn cull_undo(app: &mut PhotoVault) -> Task<Message> {
    if let Some(ref mut cull) = app.cull_state {
        cull.undo();
        app.cull_confirm_pending = false;
    }
    Task::none()
}

pub(crate) fn cull_finish(app: &mut PhotoVault) -> Task<Message> {
    if let Some(ref cull) = app.cull_state {
        if cull.marked_for_trash.is_empty() {
            return super::handle(app, Message::ExitCullMode);
        }
        app.cull_confirm_pending = true;
        return Task::none();
    }
    Task::none()
}

pub(crate) fn cull_confirm_trash(app: &mut PhotoVault) -> Task<Message> {
    if let Some(ref cull) = app.cull_state {
        let ids = cull.marked_for_trash.iter().copied().collect::<Vec<_>>();
        app.cull_confirm_pending = false;
        return super::handle(app, Message::TrashPhotos(ids));
    }
    Task::none()
}
