//! Info banner widget - dismissible inline info display (e.g. summarization started/finished)

use cosmic::{
    iced::Length,
    widget::{button, container, row, text, Space},
    Element,
};

/// Create an info banner widget (neutral/blue style, not error)
pub fn info_banner<M: Clone + 'static>(message: &str, on_dismiss: M) -> Element<'static, M> {
    let text_content = message.to_string();

    let content = row::with_children(vec![
        text("ℹ️").size(16).into(),
        text(text_content).size(14).into(),
        Space::new().width(Length::Fill).into(),
        button::standard("Dismiss")
            .on_press(on_dismiss)
            .padding([4, 12])
            .into(),
    ])
    .spacing(12)
    .align_y(cosmic::iced::Alignment::Center);

    container(content)
        .padding(12)
        .width(Length::Fill)
        .style(|_theme| cosmic::widget::container::Style {
            background: Some(cosmic::iced::Background::Color(cosmic::iced::Color::from_rgb(
                0.2, 0.35, 0.5,
            ))),
            border: cosmic::iced::Border {
                color: cosmic::iced::Color::from_rgb(0.3, 0.5, 0.7),
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .into()
}
