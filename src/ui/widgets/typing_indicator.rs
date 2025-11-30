use cosmic::{
    iced::Length,
    widget::{container, text, Row},
    Element,
};

/// Typing indicator widget that shows animated three dots
pub fn typing_indicator(animation_progress: f32) -> Element<'static, ()> {
    // Create three dots with staggered animation
    let dots: Vec<Element<'static, ()>> = (0..3)
        .map(|i| {
            let offset = i as f32 * 0.2;
            let raw_progress = (animation_progress + offset) % 1.0;
            let progress = if raw_progress <= 0.5 {
                raw_progress * 2.0
            } else {
                (1.0 - raw_progress) * 2.0
            };
            
            // Opacity from 0.5 to 1.0 for animation - ensure minimum visibility
            let opacity = (0.5 + (progress * 0.5)).max(0.5);

            // Create a visible dot - use a larger, more visible character
            text("●")
                .size(18)
                .class(cosmic::style::Text::Color(cosmic::iced::Color {
                    r: 0.7,
                    g: 0.7,
                    b: 0.7,
                    a: opacity,
                }))
                .into()
        })
        .collect();

    container(
        Row::with_children(vec![
            text("Typing")
                .size(14)
                .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.7, 0.7, 0.7)))
                .into(),
            Row::with_children(dots).spacing(4).into(),
        ])
        .spacing(8)
        .align_y(cosmic::iced::Alignment::Center)
    )
    .padding(12)
    .width(Length::Shrink)
    .class(cosmic::style::Container::Card)
    .into()
}


