//! Face review deck: "Is this the same person?" confirmation UI.

use std::path::{Path, PathBuf};

use iced::widget::{button, column, container, image as iced_image, row, text, Space};
use iced::{Alignment, ContentFit, Element, Length, Padding};

use crate::app::state::FaceReviewState;
use crate::app::Message;
use crate::config::AppTheme;
use crate::theme::colors;

pub struct FaceReviewView;

impl FaceReviewView {
    pub fn view(
        state: &FaceReviewState,
        drive_path: Option<&Path>,
        theme: AppTheme,
    ) -> Element<'static, Message> {
        let p = colors::palette(theme);

        let header = row![
            text("Review Faces").size(20).color(p.text_primary),
            Space::with_width(Length::Fill),
            button(text("Finish").size(12).color(p.text_primary))
                .on_press(Message::FaceReviewFinish)
                .padding(Padding::from([6, 14])),
        ]
        .align_y(Alignment::Center)
        .spacing(8);

        if state.queue.is_empty() {
            return container(
                column![
                    header,
                    Space::with_height(48),
                    text("No faces to review.")
                        .size(18)
                        .color(p.text_secondary),
                    Space::with_height(8),
                    text("Run face processing, or come back later.")
                        .size(13)
                        .color(p.text_tertiary),
                ]
                .spacing(0)
                .align_x(Alignment::Center),
            )
            .padding(24)
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
        }

        if state.is_finished() {
            return container(
                column![
                    header,
                    Space::with_height(48),
                    text("All done for this session.")
                        .size(18)
                        .color(p.text_secondary),
                    Space::with_height(8),
                    text(format!(
                        "Confirmed: {} · Rejected: {} · Skipped: {}",
                        state.confirmed, state.rejected, state.skipped
                    ))
                    .size(13)
                    .color(p.text_tertiary),
                    Space::with_height(24),
                    button(text("Back to People").size(13).color(p.text_primary))
                        .on_press(Message::FaceReviewFinish)
                        .padding(Padding::from([8, 18])),
                ]
                .spacing(0)
                .align_x(Alignment::Center),
            )
            .padding(24)
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
        }

        let item = match state.current() {
            Some(it) => it,
            None => {
                return container(header).padding(24).into();
            }
        };

        let total = state.queue.len();
        let current_num = state.current_index + 1;

        let stats_text = text(format!(
            "{} of {} · same {} · different {} · skipped {}",
            current_num, total, state.confirmed, state.rejected, state.skipped
        ))
        .size(13)
        .color(p.text_secondary);

        let question = text("Is this the same person?")
            .size(22)
            .color(p.text_primary);

        // Left side: the face being reviewed (unassigned face)
        let query_path = face_crop_path(drive_path, item.face_id);
        let query_image = face_box(query_path, 280.0, p.bg_elevated, p.border_subtle);

        // Right side: candidate cluster samples (up to 4) + name
        let candidate_name = item
            .candidate_cluster_name
            .clone()
            .unwrap_or_else(|| format!("Person #{}", item.candidate_cluster_id));

        let mut sample_row = row![].spacing(8).align_y(Alignment::Center);
        for face_id in &item.candidate_sample_face_ids {
            let path = face_crop_path(drive_path, *face_id);
            sample_row = sample_row.push(face_box(path, 120.0, p.bg_elevated, p.border_subtle));
        }

        let candidate_column = column![
            text(candidate_name).size(16).color(p.text_primary),
            text(format!(
                "{} faces · score {:.2}",
                item.candidate_cluster_size, item.score
            ))
            .size(12)
            .color(p.text_tertiary),
            Space::with_height(12),
            sample_row,
        ]
        .spacing(4)
        .align_x(Alignment::Start);

        let pair = row![
            column![
                text("Unassigned face").size(12).color(p.text_tertiary),
                Space::with_height(6),
                query_image,
            ]
            .align_x(Alignment::Center),
            Space::with_width(32),
            column![
                text("Candidate match").size(12).color(p.text_tertiary),
                Space::with_height(6),
                candidate_column,
            ]
            .align_x(Alignment::Start),
        ]
        .align_y(Alignment::Start);

        let same_btn = button(
            text("Same person  (Y)")
                .size(14)
                .color(p.text_primary),
        )
        .on_press(Message::FaceReviewSame)
        .padding(Padding::from([10, 22]));

        let different_btn = button(
            text("Different  (N)")
                .size(14)
                .color(p.text_primary),
        )
        .on_press(Message::FaceReviewDifferent)
        .padding(Padding::from([10, 22]));

        let skip_btn = button(text("Skip  (S)").size(13).color(p.text_secondary))
            .on_press(Message::FaceReviewSkip)
            .padding(Padding::from([10, 18]));

        let undo_btn = button(text("Undo  (U)").size(13).color(p.text_secondary))
            .on_press(Message::FaceReviewUndo)
            .padding(Padding::from([10, 18]));

        let actions = row![
            same_btn,
            Space::with_width(12),
            different_btn,
            Space::with_width(24),
            skip_btn,
            Space::with_width(8),
            undo_btn,
        ]
        .align_y(Alignment::Center);

        container(
            column![
                header,
                Space::with_height(8),
                stats_text,
                Space::with_height(16),
                question,
                Space::with_height(24),
                pair,
                Space::with_height(32),
                actions,
            ]
            .spacing(0),
        )
        .padding(24)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
}

fn face_crop_path(drive_path: Option<&Path>, face_id: i64) -> Option<PathBuf> {
    let drive = drive_path?;
    let path = drive
        .join(".photovault")
        .join("faces")
        .join(format!("{}.jpg", face_id));
    path.exists().then_some(path)
}

fn face_box(
    path: Option<PathBuf>,
    size: f32,
    bg: iced::Color,
    border: iced::Color,
) -> Element<'static, Message> {
    let inner: Element<'static, Message> = match path {
        Some(p) => iced_image(p.to_string_lossy().to_string())
            .content_fit(ContentFit::Cover)
            .width(Length::Fixed(size))
            .height(Length::Fixed(size))
            .into(),
        None => container(text("no crop").size(11).color(iced::Color::from_rgb8(128, 128, 128)))
            .width(Length::Fixed(size))
            .height(Length::Fixed(size))
            .center_x(Length::Fixed(size))
            .center_y(Length::Fixed(size))
            .into(),
    };

    container(inner)
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(bg.into()),
            border: iced::Border {
                color: border,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .into()
}
