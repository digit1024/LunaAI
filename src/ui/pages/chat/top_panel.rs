use cosmic::{
    iced::Length,
    widget,
    Element,
};
use crate::ui::app::{Message, CosmicLlmApp};

pub fn top_panel(app: &CosmicLlmApp) -> Element<Message> {
    // Count enabled/disabled tools
    let total_tools = app.available_mcp_tools.len();
    let enabled_count = app.available_mcp_tools.iter()
        .filter(|tool| app.tool_states.get(&tool.name).copied().unwrap_or(true))
        .count();
    
    // Conversation info
    let (title, created_text, msg_count) = if let Some(id) = app.current_conversation_id {
        if let Ok(Some(conv)) = app.storage.get_conversation(&id) {
            let created = conv.created_at.format("%Y-%m-%d %H:%M").to_string();
            // Prefer the latest title from the on-disk index (updated by background tasks)
            let index = app.storage.list_conversations_from_index().unwrap_or_else(|e| {
                eprintln!("Failed to list conversations: {}", e);
                Vec::new()
            });
            let latest_title = index
                .into_iter()
                .find(|ci| ci.id == id)
                .map(|ci| ci.title)
                .unwrap_or_else(|| conv.title.clone());
            (latest_title, Some(created), conv.messages.len())
        } else {
            ("New Chat".to_string(), None, app.messages.len())
        }
    } else {
        ("New Chat".to_string(), None, app.messages.len())
    };
    
    let _created_label = created_text.unwrap_or_else(|| "".to_string());
    
    cosmic::widget::container(
        cosmic::widget::column::with_capacity(2)
            .push(
                // Top row: Title, Messages count, New chat icon
                cosmic::widget::row::with_capacity(3)
                    .push(
                        cosmic::widget::text(title)
                            .size(18)
                    )
                    .push(
                        cosmic::widget::text(format!("{} messages", msg_count))
                            .size(12)
                            .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.4, 0.4, 0.4)))
                    )
                    .push(cosmic::widget::Space::with_width(Length::Fill))
                    .push(
                        // New chat icon button
                        widget::button::icon(crate::ui::icons::get_handle("plus-circle-filled-symbolic", 16))
                            .class(widget::button::ButtonClass::Suggested)
                            .on_press(Message::NewConversation)
                    )
                    .spacing(12)
                    .align_y(cosmic::iced::Alignment::Center)
            )
            .push(
                // Bottom row: Model select, Tools summary with icons
                cosmic::widget::row::with_capacity(4)
                    .push(
                        // Profile selection dropdown
                        {
                            let mut names: Vec<String> = app.config.profiles.keys().cloned().collect();
                            names.sort();
                            let idx = names.iter().position(|k| k == &app.config.default);
                            widget::dropdown(names, idx, Message::ChangeDefaultProfile)
                        }
                    )
                    .push(cosmic::widget::Space::with_width(Length::Fill))
                    .push(
                        // Tools summary with toggle and configure icons
                        if total_tools == 0 {
                            // Show configure button when no tools
                            cosmic::widget::row::with_capacity(2)
                                .push(
                                    cosmic::widget::text("No tools configured")
                                        .size(12)
                                        .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.5, 0.5, 0.5)))
                                )
                                .push(
                                    widget::button::icon(crate::ui::icons::get_handle("configure-symbolic", 16))
                                        .on_press(Message::ShowToolsContext)
                                )
                                .spacing(8)
                                .align_y(cosmic::iced::Alignment::Center)
                        } else {
                            // Tool controls with icons
                            cosmic::widget::row::with_capacity(4)
                                .push(
                                    cosmic::widget::text(format!("{} / {} tools", enabled_count, total_tools))
                                        .size(12)
                                )
                                .push(
                                    // Toggle all tools button (toggler widget)
                                    cosmic::widget::toggler(enabled_count == total_tools)
                                        .on_toggle(|enabled| Message::ToggleAllTools(enabled))
                                )
                                .push(
                                    // Configure tools button (icon)
                                    widget::button::icon(crate::ui::icons::get_handle("configure-symbolic", 16))
                                        .on_press(Message::ShowToolsContext)
                                )
                                .spacing(8)
                                .align_y(cosmic::iced::Alignment::Center)
                        }
                    )
                    .spacing(12)
                    .align_y(cosmic::iced::Alignment::Center)
            )
            .spacing(8)
    )
    .padding(12)
    .class(cosmic::style::Container::Card)
    .into()
}
