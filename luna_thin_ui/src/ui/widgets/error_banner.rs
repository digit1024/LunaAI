//! Error banner widget - dismissible inline error display

use cosmic::{
    iced::Length,
    widget::{button, container, row, text, Space},
    Element,
};

/// Create an error banner widget
pub fn error_banner<M: Clone + 'static>(error: &str, on_dismiss: M) -> Element<'static, M> {
    let error_text = error.to_string();
    
    let content = row::with_children(vec![
        text("⚠️").size(16).into(),
        text(error_text).size(14).into(),
        Space::with_width(Length::Fill).into(),
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
                0.5, 0.2, 0.2,
            ))),
            border: cosmic::iced::Border {
                width: 1.0,
                color: cosmic::iced::Color::from_rgb(0.7, 0.3, 0.3),
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .into()
}

