use cosmic::{
    font,
    iced::{Length, Padding},
    widget::{button, container, row, column, text, scrollable, Space},
    Element,
};

#[derive(Debug, Clone)]
pub enum Message {
    ToggleExpanded,
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

#[derive(Debug, Clone, PartialEq)]
pub enum ToolCallStatus {
    Started,
    Completed,
    Error,
}

impl ToolCallWidget {
    pub fn update(&mut self, message: Message) {
        match message {
            Message::ToggleExpanded => {
                self.is_expanded = !self.is_expanded;
            }
        }
    }

    pub fn view(&self) -> Element<Message> {
        let (status_icon, status_text, status_color) = match self.status {
            ToolCallStatus::Started => ("...", "Executing", cosmic::iced::Color::from_rgb(0.5, 0.5, 0.5)),
            ToolCallStatus::Completed => ("✓", "Completed", cosmic::iced::Color::from_rgb(0.2, 0.7, 0.2)),
            ToolCallStatus::Error => ("✕", "Error", cosmic::iced::Color::from_rgb(0.8, 0.2, 0.2)),
        };

        let expand_icon = if self.is_expanded { "▼" } else { "▶" };

        let header = row()
            .push(text(status_icon).size(16).class(cosmic::theme::Text::Color(status_color)))
            .push(text(&self.tool_name).font(font::Font::MONOSPACE))
            .push(Space::with_width(Length::Fill))
            .push(text(status_text).class(cosmic::theme::Text::Color(status_color)))
            .push(
                button::text(expand_icon)
                    .on_press(Message::ToggleExpanded)
                    .class(cosmic::theme::Button::Text)
            )
            .spacing(10)
            .align_y(cosmic::iced::Alignment::Center)
            .width(Length::Fill);

        let mut content = column().push(header).spacing(10);

        if self.is_expanded {
            let params_widget = column()
                .push(text("Parameters").size(14).class(cosmic::theme::Text::Color(cosmic::iced::Color::from_rgb(0.6, 0.6, 0.6))))
                .push(
                    container(
                        scrollable(
                            text(&self.parameters)
                                .size(12)
                                .font(font::Font::MONOSPACE)
                        )
                        .height(Length::Fixed(80.0))
                    )
                    .class(cosmic::theme::Container::Card)
                    .padding(8)
                )
                .spacing(5);
            content = content.push(params_widget);

            if let Some(ref result) = self.result {
                let result_widget = column()
                    .push(text("Result").size(14).class(cosmic::theme::Text::Color(cosmic::iced::Color::from_rgb(0.2, 0.7, 0.2))))
                    .push(
                        container(
                            scrollable(
                                text(result)
                                    .size(12)
                                    .font(font::Font::MONOSPACE)
                            )
                            .height(Length::Fixed(120.0))
                        )
                        .class(cosmic::theme::Container::Card)
                        .padding(8)
                    )
                    .spacing(5);
                content = content.push(result_widget);
            } else if let Some(ref error) = self.error {
                let error_widget = column()
                    .push(text("Error").size(14).class(cosmic::theme::Text::Color(cosmic::iced::Color::from_rgb(0.8, 0.2, 0.2))))
                    .push(
                        container(
                            scrollable(
                                text(error)
                                    .size(12)
                                    .font(font::Font::MONOSPACE)
                            )
                            .height(Length::Fixed(80.0))
                        )
                        .class(cosmic::theme::Container::Card)
                        .padding(8)
                    )
                    .spacing(5);
                content = content.push(error_widget);
            }
        }

        container(content)
            .width(Length::Fill)
            .padding(Padding::from([10, 15]))
            .class(cosmic::theme::Container::Card)
            .into()
    }
}

impl From<&crate::ui::app::ToolCallInfo> for ToolCallWidget {
    fn from(tool_call: &crate::ui::app::ToolCallInfo) -> Self {
        let status = match tool_call.status {
            crate::ui::app::ToolCallStatus::Started => ToolCallStatus::Started,
            crate::ui::app::ToolCallStatus::Completed => ToolCallStatus::Completed,
            crate::ui::app::ToolCallStatus::Error => ToolCallStatus::Error,
        };

        Self {
            tool_name: tool_call.tool_name.clone(),
            parameters: tool_call.parameters.clone(),
            status,
            result: tool_call.result.clone(),
            error: tool_call.error.clone(),
            is_expanded: false,
        }
    }
}
