//! Tool call widget - expandable card showing tool execution
//!
//! Matches the original app's tool_call.rs styling.

use cosmic::{
    font,
    iced::{Length, Padding},
    widget::{button, container, scrollable, text, Column, Row, Space},
    Element,
};
use crate::ui::icons;

#[derive(Debug, Clone)]
pub enum ToolCallMessage {
    ToggleExpanded,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToolCallStatus {
    Planned,
    Running,
    Completed,
    Error,
}

#[derive(Debug, Clone)]
pub struct ToolCallWidget {
    pub tool_name: String,
    pub parameters: String,
    pub status: ToolCallStatus,
    pub result: Option<String>,
    pub error: Option<String>,
    pub is_expanded: bool,
}

impl ToolCallWidget {
    pub fn new(tool_name: String, parameters: String) -> Self {
        Self {
            tool_name,
            parameters,
            status: ToolCallStatus::Planned,
            result: None,
            error: None,
            is_expanded: false,
        }
    }

    /// Render the tool call widget content (matches original app styling)
    pub fn content<M: Clone + 'static>(&self, on_toggle: M) -> Element<'static, M> {
        // Status icon handle (no text labels)
        let status_icon_handle = match self.status {
            ToolCallStatus::Planned => icons::get_handle("charge-symbolic", 16),
            ToolCallStatus::Running => icons::get_handle("charge-symbolic", 16),
            ToolCallStatus::Completed => icons::get_handle("check-round-outline2-symbolic", 16),
            ToolCallStatus::Error => icons::get_handle("circle-crossed-symbolic", 16),
        };

        // Toggle icon handle
        let toggle_icon_handle = if self.is_expanded {
            icons::get_handle("toggle-on-symbolic", 16)
        } else {
            icons::get_handle("toggle-off-symbolic", 16)
        };

        // Clone for ownership
        let tool_name = self.tool_name.clone();
        let parameters = self.parameters.clone();
        let result = self.result.clone();
        let error = self.error.clone();
        let is_expanded = self.is_expanded;

        // Header row: status icon, tool name, expand button (no text labels)
        let header = Row::new()
            .push(
                cosmic::widget::icon::icon(status_icon_handle)
                    .size(16),
            )
            .push(
                text(tool_name)
                    .font(font::Font::MONOSPACE)
                    .size(14),
            )
            .push(Space::new().width(Length::Fill))
            .push(
                button::icon(toggle_icon_handle)
                    .on_press(on_toggle)
                    .class(cosmic::theme::Button::Text),
            )
            .spacing(10)
            .align_y(cosmic::iced::Alignment::Center)
            .width(Length::Fill);

        let mut content = Column::new().push(header).spacing(10);

        if is_expanded {
            // Parameters section
            let params_widget = Column::new()
                .push(
                    text("Parameters")
                        .size(14)
                        .class(cosmic::theme::Text::Color(cosmic::iced::Color::from_rgb(
                            0.6, 0.6, 0.6,
                        ))),
                )
                .push(
                    container(
                        scrollable(
                            text(parameters)
                                .size(12)
                                .font(font::Font::MONOSPACE),
                        )
                        .height(Length::Fixed(80.0)),
                    )
                    .class(cosmic::theme::Container::Card)
                    .padding(8),
                )
                .spacing(5);
            content = content.push(params_widget);

            // Result section (if completed)
            if let Some(result_text) = result {
                let result_widget = Column::new()
                    .push(
                        text("Result")
                            .size(14)
                            .class(cosmic::theme::Text::Color(cosmic::iced::Color::from_rgb(
                                0.2, 0.7, 0.2,
                            ))),
                    )
                    .push(
                        container(
                            scrollable(
                                text(result_text)
                                    .size(12)
                                    .font(font::Font::MONOSPACE),
                            )
                            .height(Length::Fixed(120.0)),
                        )
                        .class(cosmic::theme::Container::Card)
                        .padding(8),
                    )
                    .spacing(5);
                content = content.push(result_widget);
            }

            // Error section (if error)
            if let Some(error_text) = error {
                let error_widget = Column::new()
                    .push(
                        text("Error")
                            .size(14)
                            .class(cosmic::theme::Text::Color(cosmic::iced::Color::from_rgb(
                                0.8, 0.2, 0.2,
                            ))),
                    )
                    .push(
                        container(
                            scrollable(
                                text(error_text)
                                    .size(12)
                                    .font(font::Font::MONOSPACE),
                            )
                            .height(Length::Fixed(80.0)),
                        )
                        .class(cosmic::theme::Container::Card)
                        .padding(8),
                    )
                    .spacing(5);
                content = content.push(error_widget);
            }
        }

        content.padding(Padding::from([10, 15])).into()
    }

    /// Render the full widget wrapped in a container (for standalone use)
    pub fn view<M: Clone + 'static>(&self, on_toggle: M) -> Element<'static, M> {
        container(self.content(on_toggle))
            .width(Length::Fill)
            .class(cosmic::theme::Container::Card)
            .into()
    }
}
