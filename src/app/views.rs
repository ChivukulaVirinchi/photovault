//! Render tree for the application.

use iced::widget::{button, column, container, row, text};
use iced::{Element, Length};

use crate::components::{ScanProgressView, Sidebar};
use crate::services::ScanProgress;
use crate::theme::colors;
use crate::views::{
    BurstsView, CullView, DocumentsView, DuplicatesView, FaceReviewView, PeopleView,
    PhotoDetailView, SearchView, SettingsView, TimelineView, TrashView, WelcomeView,
};

use super::messages::Message;
use super::state::{PhotoVault, View};

/// Render the application
pub(crate) fn view(app: &PhotoVault) -> Element<'_, Message> {
    // Show scanning progress if scanning
    if app.current_view == View::Scanning {
        if let Some(ref state) = app.scan_state {
            return ScanProgressView::view(&state.progress, app.config.theme);
        } else {
            return ScanProgressView::view(&ScanProgress::default(), app.config.theme);
        }
    }

    // Photo detail view (full-screen overlay, no sidebar)
    if app.current_view == View::PhotoDetail {
        if let Some(idx) = app.selected_photo_index {
            if let Some(photo) = app.photos.get(idx) {
                let nav = app.photo_detail_navigation_list();
                let nav_idx = nav
                    .iter()
                    .position(|p| p.id == photo.id)
                    .unwrap_or(idx.min(nav.len().saturating_sub(1)));
                let has_prev = nav_idx > 0;
                let has_next = nav_idx + 1 < nav.len();
                if app.selected_drive.is_some() {
                    // Build GPU handle from the pre-decoded, orientation-corrected,
                    // resolution-capped image stored in app state.
                    let handle = if let Some(ref img) = app.current_display_image {
                        let rgba = img.as_rgba8().map_or_else(
                            || {
                                let converted = img.to_rgba8();
                                let (w, h) = (converted.width(), converted.height());
                                (converted.into_raw(), w, h)
                            },
                            |rgba| (rgba.as_raw().clone(), rgba.width(), rgba.height()),
                        );
                        Some(iced::widget::image::Handle::from_rgba(
                            rgba.1, rgba.2, rgba.0,
                        ))
                    } else {
                        None
                    };
                    let detail = PhotoDetailView::view(
                        app,
                        photo,
                        has_prev,
                        has_next,
                        &app.current_photo_people,
                        app.current_photo_face_count,
                        handle.as_ref(),
                        app.show_metadata_panel,
                        app.config.theme,
                    );
                    if app.album_picker_open {
                        let overlay = crate::views::albums::album_picker_overlay(
                            &app.albums,
                            app.album_picker_target_ids.len(),
                            app.album_picker_creating,
                            &app.album_picker_new_name,
                            app.config.theme,
                        );
                        return iced::widget::stack![detail, overlay].into();
                    }
                    return detail;
                }
            }
        }
        // Fallback: shouldn't happen, but return to timeline
        return TimelineView::view(app.config.theme);
    }

    // If no drive selected, show welcome screen
    if app.selected_drive.is_none() {
        return WelcomeView::view(&app.drives, app.config.theme);
    }

    // Main layout: sidebar (collapsible) + content
    let sidebar: Element<'_, Message> = if app.config.sidebar_collapsed {
        // Collapsed: thin strip with expand arrow
        let p = colors::palette(app.config.theme);
        let bg = p.bg_secondary;
        let tc = p.text_secondary;
        let hover_bg = p.bg_hover;
        container(
            iced::widget::button(
                text("\u{00BB}").size(16).color(tc), // » double right arrow
            )
            .padding(iced::Padding::from([8, 8]))
            .style(move |_t: &iced::Theme, s| iced::widget::button::Style {
                background: match s {
                    iced::widget::button::Status::Hovered => Some(hover_bg.into()),
                    _ => None,
                },
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .on_press(Message::ToggleSidebar),
        )
        .width(Length::Fixed(36.0))
        .height(Length::Fill)
        .padding(iced::Padding::from([16, 4]))
        .style(move |_t: &iced::Theme| container::Style {
            background: Some(bg.into()),
            ..Default::default()
        })
        .into()
    } else {
        Sidebar::view(&app.current_view, app.config.theme, app.face_review_pending)
    };

    let content = match app.current_view {
        View::Welcome => WelcomeView::view(&app.drives, app.config.theme),
        View::Scanning => unreachable!(), // Handled above
        View::Timeline => {
            let mem_banner =
                crate::views::memories::memories_banner(&app.memories, app.config.theme);
            let sug_banner =
                crate::views::albums::suggestions_banner(&app.album_suggestions, app.config.theme);
            // Combine: show suggestions banner first, then memories banner
            let banner: Option<Element<'_, Message>> = match (sug_banner, mem_banner) {
                (Some(s), Some(m)) => Some(column![s, m].into()),
                (Some(s), None) => Some(s),
                (None, Some(m)) => Some(m),
                (None, None) => None,
            };
            if app.photos.is_empty() {
                if app.photos_loading {
                    let available_width = (app.window_width - 200.0 - 32.0).max(168.0);
                    let columns = (available_width / 168.0).floor().max(2.0) as usize;
                    TimelineView::skeleton(columns, app.config.theme)
                } else {
                    TimelineView::view(app.config.theme)
                }
            } else {
                // Calculate responsive column count:
                // Sidebar is ~200px, padding 32px total, each thumb is 160+8px gap
                let available_width = (app.window_width - 200.0 - 32.0).max(168.0);
                let columns = (available_width / 168.0).floor().max(2.0) as usize;
                let highlighted_photo_id = app
                    .timeline_highlight_index
                    .and_then(|i| app.photos.get(i))
                    .map(|p| p.id);
                let timeline = TimelineView::view_with_photos(
                    &app.photos,
                    columns,
                    &app.selected_timeline_photo_ids,
                    app.hovered_timeline_photo_id,
                    app.hovered_timeline_day_key.as_deref(),
                    highlighted_photo_id,
                    banner,
                    app.config.theme,
                );

                if app.selected_timeline_photo_ids.is_empty() {
                    timeline
                } else {
                    let p = colors::palette(app.config.theme);
                    let selected_count = app.selected_timeline_photo_ids.len();

                    let clear_hover = p.bg_hover;
                    let clear_border = p.border_subtle;
                    let clear_text = p.text_secondary;
                    let clear_btn = button(text("x").size(14).color(clear_text))
                        .padding([2, 8])
                        .style(move |_theme: &iced::Theme, status| button::Style {
                            background: match status {
                                button::Status::Hovered => Some(clear_hover.into()),
                                _ => None,
                            },
                            border: iced::Border {
                                color: clear_border,
                                width: 1.0,
                                radius: 999.0.into(),
                            },
                            ..Default::default()
                        })
                        .on_press(Message::ClearTimelinePhotoSelection);

                    let delete_bg = p.semantic_danger;

                    let delete_btn = button(text("Delete").size(12).color(iced::Color::WHITE))
                        .padding([6, 12])
                        .style(move |_theme: &iced::Theme, status| button::Style {
                            background: Some(match status {
                                button::Status::Hovered => iced::Color {
                                    r: (delete_bg.r * 0.9).clamp(0.0, 1.0),
                                    g: (delete_bg.g * 0.9).clamp(0.0, 1.0),
                                    b: (delete_bg.b * 0.9).clamp(0.0, 1.0),
                                    a: 1.0,
                                }
                                .into(),
                                _ => delete_bg.into(),
                            }),
                            border: iced::Border {
                                color: iced::Color::TRANSPARENT,
                                width: 0.0,
                                radius: 6.0.into(),
                            },
                            ..Default::default()
                        })
                        .on_press(Message::TrashPhotos(
                            app.selected_timeline_photo_ids
                                .iter()
                                .copied()
                                .collect::<Vec<_>>(),
                        ));

                    let album_accent = p.accent_primary;
                    let album_hover = p.bg_hover;
                    let album_bg_btn = p.bg_elevated;
                    let album_btn = button(text("Add to Album").size(12).color(album_accent))
                        .padding([6, 12])
                        .style(move |_theme: &iced::Theme, status| button::Style {
                            background: Some(match status {
                                button::Status::Hovered => album_hover.into(),
                                _ => album_bg_btn.into(),
                            }),
                            border: iced::Border {
                                radius: 6.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        })
                        .on_press(Message::OpenAlbumPicker(
                            app.selected_timeline_photo_ids
                                .iter()
                                .copied()
                                .collect::<Vec<_>>(),
                        ));

                    let bar_bg = p.bg_secondary;
                    let bar_border = p.border_subtle;
                    let top_bar = container(
                        row![
                            clear_btn,
                            text(format!("{} selected", selected_count))
                                .size(12)
                                .color(p.text_secondary),
                            iced::widget::Space::with_width(Length::Fill),
                            album_btn,
                            iced::widget::Space::with_width(8),
                            delete_btn,
                        ]
                        .align_y(iced::Alignment::Center)
                        .spacing(10),
                    )
                    .padding([8, 20])
                    .style(move |_theme: &iced::Theme| container::Style {
                        background: Some(bar_bg.into()),
                        border: iced::Border {
                            color: bar_border,
                            width: 1.0,
                            radius: 0.0.into(),
                        },
                        ..Default::default()
                    });

                    column![top_bar, timeline].into()
                }
            }
        }
        View::Map => crate::views::map::map_view(app),
        View::People => PeopleView::view_with_clusters(
            &app.face_clusters,
            app.editing_cluster_id,
            &app.edit_cluster_name,
            app.face_processing_active,
            app.face_processing_progress.as_ref(),
            app.face_processing_error.as_deref(),
            app.merge_mode_active,
            &app.merge_selected_clusters,
            app.people_highlight_index,
            app.ml_available,
            app.config.theme,
        ),
        View::ClusterDetail => {
            // Find the selected cluster record for display
            let cluster = app
                .selected_cluster_id
                .and_then(|id| app.face_clusters.iter().find(|c| c.id == id));

            if let Some(cluster) = cluster {
                let is_editing = app.editing_cluster_id == Some(cluster.id);
                let available_width = (app.window_width - 200.0 - 32.0).max(168.0);
                let columns = (available_width / 168.0).floor().max(2.0) as usize;

                PeopleView::view_cluster_detail(
                    cluster,
                    &app.cluster_photos,
                    is_editing,
                    &app.edit_cluster_name,
                    columns,
                    app.config.theme,
                )
            } else {
                // Cluster not found — fallback to People view
                PeopleView::view_with_clusters(
                    &app.face_clusters,
                    app.editing_cluster_id,
                    &app.edit_cluster_name,
                    app.face_processing_active,
                    app.face_processing_progress.as_ref(),
                    app.face_processing_error.as_deref(),
                    app.merge_mode_active,
                    &app.merge_selected_clusters,
                    app.people_highlight_index,
                    app.ml_available,
                    app.config.theme,
                )
            }
        }
        View::FaceReview => {
            let empty_state = crate::app::state::FaceReviewState::new(Vec::new());
            let state = app.face_review_state.as_ref().unwrap_or(&empty_state);
            FaceReviewView::view(state, app.selected_drive.as_deref(), app.config.theme)
        }
        View::Memories => crate::views::memories::memories_view(&app.memories, app.config.theme),
        View::MemoryDetail => {
            let card = app
                .selected_memory_id
                .as_ref()
                .and_then(|id| app.memories.iter().find(|c| &c.id == id))
                .cloned();
            match card {
                Some(card) => crate::views::memories::memory_detail_view(
                    &card,
                    &app.memory_photos,
                    app.selected_drive.as_deref(),
                    app.memory_slideshow_index,
                    app.memory_slideshow_paused,
                    app.config.theme,
                ),
                None => crate::views::memories::memories_view(&app.memories, app.config.theme),
            }
        }
        View::Search => SearchView::view(
            &app.search_query,
            app.search_results.as_ref(),
            &app.recent_searches,
            app.search_loading,
            app.selected_drive.as_deref(),
            &app.photos,
            app.spinner_phase,
            app.config.theme,
        ),
        View::Documents => {
            let available_width = (app.window_width - 200.0 - 32.0).max(168.0);
            let columns = (available_width / 168.0).floor().max(2.0) as usize;
            let docs = DocumentsView::view(
                &app.documents,
                &app.documents_query,
                app.documents_filter,
                &app.selected_timeline_photo_ids,
                columns,
                app.hovered_timeline_photo_id,
                app.documents_highlight_index,
                app.hovered_timeline_day_key.as_deref(),
                app.config.theme,
            );

            if app.selected_timeline_photo_ids.is_empty() {
                docs
            } else {
                let p = colors::palette(app.config.theme);
                let selected_count = app.selected_timeline_photo_ids.len();

                let clear_hover = p.bg_hover;
                let clear_border = p.border_subtle;
                let clear_text = p.text_secondary;
                let clear_btn = button(text("x").size(14).color(clear_text))
                    .padding([2, 8])
                    .style(move |_theme: &iced::Theme, status| button::Style {
                        background: match status {
                            button::Status::Hovered => Some(clear_hover.into()),
                            _ => None,
                        },
                        border: iced::Border {
                            color: clear_border,
                            width: 1.0,
                            radius: 999.0.into(),
                        },
                        ..Default::default()
                    })
                    .on_press(Message::ClearTimelinePhotoSelection);

                let delete_bg = p.semantic_danger;

                let delete_btn = button(text("Delete").size(12).color(iced::Color::WHITE))
                    .padding([6, 12])
                    .style(move |_theme: &iced::Theme, status| button::Style {
                        background: Some(match status {
                            button::Status::Hovered => iced::Color {
                                r: (delete_bg.r * 0.9).clamp(0.0, 1.0),
                                g: (delete_bg.g * 0.9).clamp(0.0, 1.0),
                                b: (delete_bg.b * 0.9).clamp(0.0, 1.0),
                                a: 1.0,
                            }
                            .into(),
                            _ => delete_bg.into(),
                        }),
                        border: iced::Border {
                            color: iced::Color::TRANSPARENT,
                            width: 0.0,
                            radius: 6.0.into(),
                        },
                        ..Default::default()
                    })
                    .on_press(Message::TrashPhotos(
                        app.selected_timeline_photo_ids
                            .iter()
                            .copied()
                            .collect::<Vec<_>>(),
                    ));

                let doc_album_accent = p.accent_primary;
                let doc_album_hover = p.bg_hover;
                let doc_album_bg_btn = p.bg_elevated;
                let doc_album_btn = button(text("Add to Album").size(12).color(doc_album_accent))
                    .padding([6, 12])
                    .style(move |_theme: &iced::Theme, status| button::Style {
                        background: Some(match status {
                            button::Status::Hovered => doc_album_hover.into(),
                            _ => doc_album_bg_btn.into(),
                        }),
                        border: iced::Border {
                            radius: 6.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .on_press(Message::OpenAlbumPicker(
                        app.selected_timeline_photo_ids
                            .iter()
                            .copied()
                            .collect::<Vec<_>>(),
                    ));

                let bar_bg = p.bg_secondary;
                let bar_border = p.border_subtle;
                let top_bar = container(
                    row![
                        clear_btn,
                        text(format!("{} selected", selected_count))
                            .size(12)
                            .color(p.text_secondary),
                        iced::widget::Space::with_width(Length::Fill),
                        doc_album_btn,
                        iced::widget::Space::with_width(8),
                        delete_btn,
                    ]
                    .align_y(iced::Alignment::Center)
                    .spacing(10),
                )
                .padding([8, 20])
                .style(move |_theme: &iced::Theme| container::Style {
                    background: Some(bar_bg.into()),
                    border: iced::Border {
                        color: bar_border,
                        width: 1.0,
                        radius: 0.0.into(),
                    },
                    ..Default::default()
                });

                column![top_bar, docs].into()
            }
        }
        View::Cull => {
            if let Some(ref state) = app.cull_state {
                CullView::view(
                    state,
                    "Quick Cull",
                    &app.photos,
                    app.selected_drive.as_deref(),
                    app.cull_confirm_pending,
                    app.config.theme,
                )
            } else {
                SearchView::view(
                    &app.search_query,
                    app.search_results.as_ref(),
                    &app.recent_searches,
                    app.search_loading,
                    app.selected_drive.as_deref(),
                    &app.photos,
                    app.spinner_phase,
                    app.config.theme,
                )
            }
        }
        View::Trash => TrashView::view(
            &app.trash_items,
            &app.trash_stats,
            &app.selected_trash_ids,
            app.selected_drive.as_deref(),
            app.config.theme,
        ),
        View::Settings => SettingsView::view(
            &app.config,
            app.geocoding_progress,
            app.rotated_data_regen_active,
            app.tile_cache
                .as_ref()
                .map(|c| c.current_size_bytes() as f64 / 1024.0 / 1024.0)
                .unwrap_or(0.0),
            app.map_cache_limit_bytes_display()
                .parse::<u32>()
                .unwrap_or(500),
        ),
        View::Duplicates => DuplicatesView::view(
            &app.duplicate_groups,
            app.duplicate_wasted_space,
            app.duplicate_detection_running,
            app.selected_drive.as_deref(),
            &app.photos,
            &app.duplicate_overview,
            app.duplicates_highlight_index,
            app.spinner_phase,
            app.config.theme,
        ),
        View::DuplicateDetail => {
            if let Some(ref group) = app.selected_duplicate_group {
                if let Some(ref drive_path) = app.selected_drive {
                    DuplicatesView::group_detail_view(
                        group,
                        &app.selected_duplicate_members,
                        drive_path,
                        app.config.theme,
                    )
                } else {
                    DuplicatesView::view(
                        &app.duplicate_groups,
                        app.duplicate_wasted_space,
                        app.duplicate_detection_running,
                        app.selected_drive.as_deref(),
                        &app.photos,
                        &app.duplicate_overview,
                        app.duplicates_highlight_index,
                        app.spinner_phase,
                        app.config.theme,
                    )
                }
            } else {
                DuplicatesView::view(
                    &app.duplicate_groups,
                    app.duplicate_wasted_space,
                    app.duplicate_detection_running,
                    app.selected_drive.as_deref(),
                    &app.photos,
                    &app.duplicate_overview,
                    app.duplicates_highlight_index,
                    app.spinner_phase,
                    app.config.theme,
                )
            }
        }
        View::Bursts => BurstsView::view(
            &app.burst_groups,
            app.burst_saveable_count,
            app.burst_detection_running,
            app.selected_drive.as_deref(),
            &app.photos,
            &app.burst_overview_previews,
            app.bursts_highlight_index,
            app.spinner_phase,
            app.config.theme,
        ),
        View::BurstDetail => {
            if let Some(ref group) = app.selected_burst_group {
                BurstsView::group_detail_view(
                    group,
                    &app.selected_burst_members,
                    app.selected_drive.as_deref(),
                    &app.photos,
                    app.config.theme,
                )
            } else {
                BurstsView::view(
                    &app.burst_groups,
                    app.burst_saveable_count,
                    app.burst_detection_running,
                    app.selected_drive.as_deref(),
                    &app.photos,
                    &app.burst_overview_previews,
                    app.bursts_highlight_index,
                    app.spinner_phase,
                    app.config.theme,
                )
            }
        }
        View::Insights => crate::views::insights::insights_view(
            app.insights_data.as_ref(),
            app.insights_selected_year,
            app.insights_loading,
            app.spinner_phase,
            app.config.theme,
        ),
        View::Albums => crate::views::albums::albums_view(
            &app.albums,
            app.selected_drive.as_deref(),
            app.album_picker_creating,
            &app.album_picker_new_name,
            &app.album_suggestions,
            app.accepting_suggestion_id,
            &app.accepting_suggestion_name,
            app.albums_highlight_index,
            app.config.theme,
        ),
        View::AlbumDetail => {
            if let Some(album_id) = app.selected_album_id {
                let album = app.albums.iter().find(|a| a.id == album_id);
                if let Some(album) = album {
                    let available_width = (app.window_width - 200.0 - 32.0).max(168.0);
                    let columns = (available_width / 168.0).floor().max(2.0) as usize;
                    crate::views::albums::album_detail_view(
                        album,
                        &app.album_photos,
                        columns,
                        app.editing_album_id == Some(album_id),
                        &app.edit_album_name,
                        &app.selected_timeline_photo_ids,
                        app.hovered_timeline_photo_id,
                        app.config.theme,
                    )
                } else {
                    crate::views::albums::albums_view(
                        &app.albums,
                        app.selected_drive.as_deref(),
                        app.album_picker_creating,
                        &app.album_picker_new_name,
                        &app.album_suggestions,
                        app.accepting_suggestion_id,
                        &app.accepting_suggestion_name,
                        app.albums_highlight_index,
                        app.config.theme,
                    )
                }
            } else {
                crate::views::albums::albums_view(
                    &app.albums,
                    app.selected_drive.as_deref(),
                    app.album_picker_creating,
                    &app.album_picker_new_name,
                    &app.album_suggestions,
                    app.accepting_suggestion_id,
                    &app.accepting_suggestion_name,
                    app.albums_highlight_index,
                    app.config.theme,
                )
            }
        }
        View::PhotoDetail => unreachable!(), // Handled above
    };

    // Wrap content with album picker overlay if open
    let content = if app.album_picker_open {
        let overlay = crate::views::albums::album_picker_overlay(
            &app.albums,
            app.album_picker_target_ids.len(),
            app.album_picker_creating,
            &app.album_picker_new_name,
            app.config.theme,
        );
        iced::widget::stack![content, overlay].into()
    } else {
        content
    };

    let main_row = row![sidebar, content,];

    // Build status bar if any background operations are active
    let has_status = app.scan_state.is_some()
        || app.face_processing_active
        || app.duplicate_detection_running
        || app.burst_detection_running
        || app.geocoding_progress.is_some()
        || app.document_analysis_active
        || app.suggestion_detection_running
        || app.insights_loading
        || app.search_loading
        || app.photos_loading;

    let base: Element<'_, Message> = if has_status {
        let mut status_parts: Vec<String> = Vec::new();

        if let Some(ref state) = app.scan_state {
            let p = &state.progress;
            if p.is_complete {
                status_parts.push(format!("Scan complete: {} files", p.files_processed));
            } else {
                status_parts.push(format!(
                    "Scanning: {}/{} files",
                    p.files_processed, p.files_found
                ));
            }
        }

        if app.face_processing_active {
            if let Some(ref prog) = app.face_processing_progress {
                let eta = if prog.processed > 0 && prog.total > 0 && prog.elapsed_secs > 0.5 {
                    let frac = prog.processed as f64 / prog.total as f64;
                    if frac < 1.0 {
                        let rem = prog.elapsed_secs / frac * (1.0 - frac);
                        if rem > 60.0 {
                            format!(" ~{}m", (rem / 60.0).ceil() as u32)
                        } else {
                            format!(" ~{}s", rem.ceil() as u32)
                        }
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };
                status_parts.push(format!(
                    "Faces: {}/{} ({} found){}",
                    prog.processed, prog.total, prog.faces_found, eta
                ));
            } else {
                status_parts.push("Faces: initializing...".to_string());
            }
        }

        if app.duplicate_detection_running {
            status_parts.push("Detecting duplicates...".to_string());
        }

        if app.burst_detection_running {
            status_parts.push("Detecting bursts...".to_string());
        }

        if let Some((processed, total)) = app.geocoding_progress {
            if total > 0 {
                status_parts.push(format!("Geocoding: {}/{}", processed, total));
            }
        }

        if app.document_analysis_active {
            if let Some(ref prog) = app.ocr_progress {
                status_parts.push(format!(
                    "Documents: {}/{} analyzed ({} docs)",
                    prog.processed, prog.total, prog.documents_found
                ));
            } else {
                status_parts.push("Documents: analyzing...".to_string());
            }
        }

        if app.suggestion_detection_running {
            status_parts.push("Detecting album suggestions...".to_string());
        }

        if app.insights_loading {
            status_parts.push("Computing insights...".to_string());
        }

        if app.search_loading {
            status_parts.push("Searching...".to_string());
        }

        if app.photos_loading {
            status_parts.push("Loading photos...".to_string());
        }

        let status_text = status_parts.join("  |  ");

        let p = colors::palette(app.config.theme);
        let status_bg = p.bg_secondary;
        let status_border = p.border_subtle;
        let status_text_color = p.text_secondary;

        let status_bar = container(text(status_text).size(11).color(status_text_color))
            .width(Length::Fill)
            .padding([4, 16])
            .style(move |_theme: &iced::Theme| container::Style {
                background: Some(status_bg.into()),
                border: iced::Border {
                    color: status_border,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            });

        let layout = column![main_row, status_bar];
        let bg = p.bg_primary;

        container(layout)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_theme: &iced::Theme| container::Style {
                background: Some(bg.into()),
                ..Default::default()
            })
            .into()
    } else {
        let bg = colors::palette(app.config.theme).bg_primary;
        container(main_row)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_theme: &iced::Theme| container::Style {
                background: Some(bg.into()),
                ..Default::default()
            })
            .into()
    };

    // Layer overlays above the main layout.
    // Confirmation dialog (blocks everything behind it)
    let base = if let Some(ref pending) = app.pending_confirmation {
        let overlay = crate::components::confirm::overlay(pending, app.config.theme);
        iced::widget::stack![base, overlay].into()
    } else {
        base
    };

    // Shortcuts overlay (`?` key)
    let base = if app.shortcuts_overlay_open {
        let overlay = crate::views::shortcuts::overlay(&app.current_view, app.config.theme);
        iced::widget::stack![base, overlay].into()
    } else {
        base
    };

    // Toasts (topmost)
    if !app.toasts.is_empty() {
        let toast_overlay = crate::components::toast::toast_stack(&app.toasts, app.config.theme);
        iced::widget::stack![base, toast_overlay].into()
    } else {
        base
    }
}
