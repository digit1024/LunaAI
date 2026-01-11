//! Menu bar widget
//!
//! Extracted from app.rs for better modularity.

use crate::ui::app::{MenuAction, Message};
use cosmic::{Element, widget::{menu, RcElementWrapper}};

/// Create the application menu bar
pub fn create_menu_bar(key_binds: &std::collections::HashMap<menu::KeyBind, MenuAction>) -> Element<Message> {
    use cosmic::widget::menu::{items, root, Item, ItemHeight, ItemWidth, MenuBar, Tree};

    MenuBar::new(vec![
        Tree::with_children(
            RcElementWrapper::new(Element::from(root("File"))),
            items(
                key_binds,
                vec![
                    Item::Button("New Conversation", None, MenuAction::NewConversation),
                    Item::Button("Quit", None, MenuAction::Quit),
                ],
            ),
        ),
        Tree::with_children(
            RcElementWrapper::new(Element::from(root("View"))),
            items(
                key_binds,
                vec![Item::Button("Settings", None, MenuAction::Settings)],
            ),
        ),
        Tree::with_children(
            RcElementWrapper::new(Element::from(root("Help"))),
            items(
                key_binds,
                vec![Item::Button("About", None, MenuAction::About)],
            ),
        ),
    ])
    .item_height(ItemHeight::Dynamic(40))
    .item_width(ItemWidth::Uniform(200))
    .spacing(4.0)
    .into()
}





