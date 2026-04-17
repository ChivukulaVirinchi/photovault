//! Small helper for consistent tooltips.

use iced::widget::{container, text, tooltip};
use iced::Element;

use crate::app::Message;

pub fn with_tooltip<'a>(
    content: Element<'a, Message>,
    label: impl Into<String>,
) -> Element<'a, Message> {
    let label = label.into();
    if label.trim().is_empty() {
        return content;
    }

    tooltip(
        content,
        container(text(label).size(11)).padding([6, 8]),
        tooltip::Position::Top,
    )
    .gap(6)
    .into()
}
