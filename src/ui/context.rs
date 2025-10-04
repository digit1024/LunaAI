/// Context drawer page variants for cosmic_llm
/// 
/// Pattern based on msToDO's context.rs implementation
/// Provides type-safe context page management with dynamic titles

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContextPage {
    About,
    // Future: Settings, MCPServers, etc.
}

impl ContextPage {
    /// Get the title for the context drawer
    /// In the future, this could use localization (i18n)
    pub fn title(&self) -> String {
        match self {
            Self::About => "About".to_string(),
            // Future: Self::Settings => "Settings".to_string(),
        }
    }
}

impl Default for ContextPage {
    fn default() -> Self {
        Self::About
    }
}

