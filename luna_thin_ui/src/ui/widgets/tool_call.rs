//! Tool call widget - expandable card showing tool execution
//!
//! Matches the original app's tool_call.rs styling.

use cosmic::{
    font,
    iced::{Length, Padding},
    widget::{button, column, container, row, scrollable, text, Space},
    Element,
};

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
        // Status icon, text, and color (matching original app)
        let (status_icon, status_text, status_color) = match self.status {
            ToolCallStatus::Planned => (
                "...",
                "Planned",
                cosmic::iced::Color::from_rgb(0.5, 0.5, 0.5),
            ),
            ToolCallStatus::Running => (
                "...",
                "Executing",
                cosmic::iced::Color::from_rgb(0.5, 0.5, 0.5),
            ),
            ToolCallStatus::Completed => (
                "✓",
                "Completed",
                cosmic::iced::Color::from_rgb(0.2, 0.7, 0.2),
            ),
            ToolCallStatus::Error => (
                "✕",
                "Error",
                cosmic::iced::Color::from_rgb(0.8, 0.2, 0.2),
            ),
        };

        let expand_icon = if self.is_expanded { "▼" } else { "▶" };

        // Clone for ownership
        let tool_name = self.tool_name.clone();
        let parameters = self.parameters.clone();
        let result = self.result.clone();
        let error = self.error.clone();
        let is_expanded = self.is_expanded;

        // Header row: status icon, tool name, status text, expand button
        let header = row()
            .push(
                text(status_icon)
                    .size(16)
                    .class(cosmic::theme::Text::Color(status_color)),
            )
            .push(
                text(tool_name)
                    .font(font::Font::MONOSPACE)
                    .size(14),
            )
            .push(Space::with_width(Length::Fill))
            .push(
                text(status_text)
                    .size(12)
                    .class(cosmic::theme::Text::Color(status_color)),
            )
            .push(
                button::text(expand_icon)
                    .on_press(on_toggle)
                    .class(cosmic::theme::Button::Text),
            )
            .spacing(10)
            .align_y(cosmic::iced::Alignment::Center)
            .width(Length::Fill);

        let mut content = column().push(header).spacing(10);

        if is_expanded {
            // Parameters section
            let params_widget = column()
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
                let result_widget = column()
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
                let error_widget = column()
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
