use cosmic::widget;
use cosmic::Element;

pub struct SettingsSection;

impl SettingsSection {
    pub fn new() -> Self {
        Self
    }

    pub fn create_section(title: &str, items: Vec<Element<'static, ()>>) -> Element<'static, ()> {
        let mut section = widget::settings::section().title(title);
        
        for item in items {
            section = section.add(item);
        }
        
        section.into()
    }
}
