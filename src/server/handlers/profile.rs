use super::ServerHandler;
use crate::server::dto::ServerEvent;
use anyhow::Result;

impl ServerHandler {
    pub(super) async fn handle_change_profile(&mut self, profile: String) -> Result<()> {
        self.session.update_profile(&profile, &self.ctx.config, &self.ctx.mcp_registry).await?;
        self.send_event(ServerEvent::ProfileChanged { profile: profile.clone() })?;
        
        // Update active conversation's profile in database if there is one
        if let Some(conv_id) = self.session.active_conversation_id {
            let storage = self.ctx.storage.lock().await;
            if let Err(e) = storage.update_conversation_profile(&conv_id, Some(&profile)) {
                tracing::error!(
                    conversation_id = %conv_id,
                    error = %e,
                    "Failed to update active conversation profile"
                );
            }
        }
        
        Ok(())
    }
    pub(super) async fn handle_list_profiles(&self) -> Result<()> {
        let mut profiles: Vec<String> = self.ctx.config.profiles
            .iter()
            .filter(|(_, p)| !p.hidden)
            .map(|(name, _)| name.clone())
            .collect();
        profiles.sort();
        self.send_event(ServerEvent::ProfilesList {
            profiles,
            default_profile: self.ctx.config.default.clone(),
        })?;
        Ok(())
    }
}
