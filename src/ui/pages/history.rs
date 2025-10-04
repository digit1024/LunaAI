use cosmic::{
    iced::{Alignment, Length, Subscription},
    widget::{self, scrollable},
    Element, theme,
};

#[derive(Debug, Clone)]
pub enum Message {
    LoadConversations,
    SelectConversation(usize),
    DeleteConversation(usize),
    SearchChanged(String),
}

pub struct HistoryPage {
    conversations: Vec<Conversation>,
    selected_index: Option<usize>,
    search_query: String,
}

#[derive(Debug, Clone)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub preview: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub message_count: usize,
}

impl HistoryPage {
    pub fn new() -> Self {
        Self {
            conversations: Vec::new(),
            selected_index: None,
            search_query: String::new(),
        }
    }

    pub fn update(&mut self, message: Message) -> Vec<cosmic::Task<Message>> {
        let mut tasks = vec![];
        
        match message {
            Message::LoadConversations => {
                // TODO: Load conversations from storage
                self.conversations = vec![
                    Conversation {
                        id: "1".to_string(),
                        title: "Sample Conversation".to_string(),
                        preview: "Hello, how can I help you today?".to_string(),
                        created_at: chrono::Utc::now(),
                        message_count: 5,
                    },
                    Conversation {
                        id: "2".to_string(),
                        title: "Another Chat".to_string(),
                        preview: "What's the weather like?".to_string(),
                        created_at: chrono::Utc::now() - chrono::Duration::hours(2),
                        message_count: 3,
                    },
                ];
            }
            Message::SelectConversation(index) => {
                self.selected_index = Some(index);
            }
            Message::DeleteConversation(index) => {
                if index < self.conversations.len() {
                    self.conversations.remove(index);
                    if self.selected_index == Some(index) {
                        self.selected_index = None;
                    }
                }
            }
            Message::SearchChanged(query) => {
                self.search_query = query;
            }
        }
        
        tasks
    }

    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::none()
    }

    pub fn view(&self) -> Element<Message> {
        cosmic::widget::column::with_capacity(3)
            .push(
                // Header
                cosmic::widget::row::with_capacity(2)
                    .push(
                        cosmic::widget::text("Conversation History")
                            .size(24)
                    )
                    .push(
                        cosmic::widget::button::suggested("Refresh")
                            .on_press(Message::LoadConversations)
                    )
                    .align_y(Alignment::Center)
                    .spacing(16)
                    .padding(16)
            )
            .push(
                // Search bar
                cosmic::widget::text_input("Search conversations...", &self.search_query)
                    .on_input(Message::SearchChanged)
                    .padding(16)
            )
            .push(
                // Conversations list
                {
                    let mut column = cosmic::widget::column::with_capacity(self.conversations.len());
                    for (index, conv) in self.conversations.iter().enumerate() {
                        let is_selected = self.selected_index == Some(index);
                        
                        column = column.push(
                            cosmic::widget::container(
                                cosmic::widget::column::with_capacity(3)
                                    .push(
                                        cosmic::widget::row::with_capacity(2)
                                            .push(
                                                cosmic::widget::text(&conv.title)
                                                    .size(16)
                                            )
                                            .push(
                                                cosmic::widget::text(conv.created_at.format("%Y-%m-%d %H:%M").to_string())
                                                    .size(12)
                                                    .class(cosmic::style::Text::Color(
                                                        theme::active().cosmic().palette.neutral_6.into()
                                                    ))
                                            )
                                            .align_y(Alignment::Center)
                                            .spacing(8)
                                    )
                                    .push(
                                        cosmic::widget::text(&conv.preview)
                                            .size(14)
                                            .class(cosmic::style::Text::Color(
                                                theme::active().cosmic().palette.neutral_7.into()
                                            ))
                                    )
                                    .push(
                                        cosmic::widget::text(format!("{} messages", conv.message_count))
                                            .size(12)
                                            .class(cosmic::style::Text::Color(
                                                theme::active().cosmic().palette.neutral_6.into()
                                            ))
                                    )
                                    .spacing(4)
                                    .padding(12)
                            )
                            .padding(8)
                        );
                    }
                    scrollable(column)
                }
                .height(Length::Fill)
                .width(Length::Fill)
            )
            .into()
    }
}
