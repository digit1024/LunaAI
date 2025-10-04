use cosmic::{
    iced::{Length, Padding},
    widget::{button, container, row, column, text, scrollable},
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
    pub fn new(tool_name: String, parameters: String, status: ToolCallStatus) -> Self {
        Self {
            tool_name,
            parameters,
            status,
            result: None,
            error: None,
            is_expanded: false,
        }
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::ToggleExpanded => {
                self.is_expanded = !self.is_expanded;
            }
        }
    }

    pub fn view(&self) -> Element<Message> {
        let status_icon = match self.status {
            ToolCallStatus::Started => "⏳",
            ToolCallStatus::Completed => "✅",
            ToolCallStatus::Error => "❌",
        };

        let status_text = match self.status {
            ToolCallStatus::Started => "Running",
            ToolCallStatus::Completed => "Completed",
            ToolCallStatus::Error => "Failed",
        };

        let expand_icon = if self.is_expanded { "▼" } else { "▶" };

        container(
            column::with_capacity(4)
                .push(
                    // Header row with tool name, status, and expand button
                    row::with_capacity(3)
                        .push(
                            text("🔧")
                                .size(14)
                        )
                        .push(
                            text(format!("Agent function called: {}", self.tool_name))
                                .size(12)
                                .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.2, 0.6, 1.0)))
                        )
                        .push(
                            button::text(expand_icon)
                                .on_press(Message::ToggleExpanded)
                                .class(cosmic::style::Button::Text)
                        )
                        .spacing(6)
                        .align_y(cosmic::iced::Alignment::Center)
                )
                .push(
                    // Status indicator
                    row::with_capacity(2)
                        .push(
                            text(status_icon)
                                .size(12)
                        )
                        .push(
                            text(status_text)
                                .size(12)
                                .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.6, 0.6, 0.6)))
                        )
                        .spacing(3)
                        .align_y(cosmic::iced::Alignment::Center)
                )
                .push(
                    // Expanded content (parameters and result)
                    if self.is_expanded {
                        column::with_capacity(3)
                            .push(
                                text("Parameters:")
                                    .size(10)
                                    .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.5, 0.5, 0.5)))
                            )
                            .push(
                                container(
                                    scrollable(
                                        text(&self.parameters)
                                            .size(12)
                                            .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.4, 0.4, 0.4)))
                                    )
                                    .height(Length::Fixed(60.0))
                                )
                                .padding(8)
                                .class(cosmic::style::Container::Card)
                            )
                            .push(
                                if let Some(ref result) = self.result {
                                    column::with_capacity(2)
                                        .push(
                                            text("Result:")
                                                .size(12)
                                                .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.0, 0.7, 0.0)))
                                        )
                                        .push(
                                            container(
                                                scrollable(
                                                    text(result)
                                                        .size(12)
                                                        .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.0, 0.5, 0.0)))
                                                )
                                                .height(Length::Fixed(80.0))
                                            )
                                            .padding(8)
                                            .class(cosmic::style::Container::Card)
                                        )
                                } else if let Some(ref error) = self.error {
                                    column::with_capacity(2)
                                        .push(
                                            text("Error:")
                                                .size(12)
                                                .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.8, 0.2, 0.2)))
                                        )
                                        .push(
                                            container(
                                                scrollable(
                                                    text(error)
                                                        .size(12)
                                                        .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.7, 0.1, 0.1)))
                                                )
                                                .height(Length::Fixed(60.0))
                                            )
                                            .padding(8)
                                            .class(cosmic::style::Container::Card)
                                        )
                                } else {
                                    column::with_capacity(0)
                                }
                            )
                            .spacing(6)
                    } else {
                        column::with_capacity(0)
                    }
                )
                .spacing(6)
        )
        .padding(Padding::from([8, 12]))
        .class(cosmic::style::Container::Card)
        .width(Length::Fill)
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
