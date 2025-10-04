pub mod app;
pub mod context;
pub mod dialogs;
pub mod icons;
pub mod pages;
pub mod widgets;

pub use app::CosmicLlmApp;

pub fn settings() -> cosmic::app::Settings {
    cosmic::app::Settings::default()
        .antialiasing(true)
        .client_decorations(true)
        .size_limits(cosmic::iced::Limits::NONE.min_width(800.0).min_height(600.0))
        .size(cosmic::iced::Size::new(1200.0, 800.0))
}

pub fn flags() -> () {
    ()
}