pub mod page;

pub use page::{Message, Page};

use crate::ui::app::CosmicLlmApp;
use cosmic::Element;

/// View function that delegates to Page::view
pub fn history_view(app: &CosmicLlmApp) -> Element<crate::ui::app::Message> {
    app.history_page
        .view(&app.storage)
        .map(|msg| crate::ui::app::Message::HistoryPage(msg))
}
