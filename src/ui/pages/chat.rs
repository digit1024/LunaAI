use cosmic::{
    iced::{Alignment, Length, Subscription},
    widget::{self, text_input, scrollable},
    Element, theme,
};

#[derive(Debug, Clone)]
pub enum Message {
    InputChanged(String),
    SendMessage,
    MessageReceived(String),
    ClearInput,
}

pub struct ChatPage {
    input: String,
    messages: Vec<ChatMessage>,
    input_id: widget::Id,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub content: String,
    pub is_user: bool,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl ChatPage {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            messages: Vec::new(),
            input_id: widget::Id::unique(),
        }
    }

    pub fn update(&mut self, message: Message) -> Vec<cosmic::Task<Message>> {
        let mut tasks = vec![];
        
        match message {
            Message::InputChanged(input) => {
                self.input = input;
            }
            Message::SendMessage => {
                if !self.input.trim().is_empty() {
                    // Add user message
                    self.messages.push(ChatMessage {
                        content: self.input.clone(),
                        is_user: true,
                        timestamp: chrono::Utc::now(),
                    });
                    

                    self.messages.push(ChatMessage {
                        content: format!("Echo: {}", self.input),
                        is_user: false,
                        timestamp: chrono::Utc::now(),
                    });
                    
                    self.input.clear();
                }
            }
            Message::MessageReceived(content) => {
                
                self.messages.push(ChatMessage {
                    content,
                    is_user: false,
                    timestamp: chrono::Utc::now(),
                });
            }
            Message::ClearInput => {
                self.input.clear();
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
                // Messages area
                {
                    let mut column = cosmic::widget::column::with_capacity(self.messages.len());
                    for msg in &self.messages {
                        column = column.push(
                            cosmic::widget::row::with_capacity(2)
                                .push(
                                    cosmic::widget::text(if msg.is_user { "You" } else { "AI" })
                                        .size(12)
                                        .class(cosmic::style::Text::Color(
                                            if msg.is_user { 
                                                theme::active().cosmic().accent_color().into()
                                            } else { 
                                                theme::active().cosmic().palette.neutral_9.into()
                                            }
                                        ))
                                )
                                .push(
                                    cosmic::widget::text(&msg.content)
                                        .width(Length::Fill)
                                )
                                .align_y(Alignment::Start)
                                .spacing(8)
                                .padding(8)
                        );
                    }
                    scrollable(column)
                }
                .height(Length::Fill)
                .width(Length::Fill)
            )
            .push(
                // Input area
                cosmic::widget::row::with_capacity(2)
                    .push(
                        text_input("Type your message...", &self.input)
                            .id(self.input_id.clone())
                            .on_input(Message::InputChanged)
                            .on_submit(|_| Message::SendMessage)
                            .width(Length::Fill)
                    )
                    .push(
                        cosmic::widget::button::suggested("Send")
                            .on_press(Message::SendMessage)
                    )
                    .spacing(8)
                    .padding(16)
            )
            .into()
    }
}
