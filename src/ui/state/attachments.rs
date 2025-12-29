//! Attachment state management
//!
//! Manages file attachment state:
//! - Attached files list
//! - Pending LLM messages with attachments

use crate::llm::Message as LlmMessage;
use anyhow::{Context, Result};
use tracing::{debug, error};

/// Attachment state
pub struct AttachmentState {
    /// Attached files (file paths)
    pub attached_files: Vec<String>,
    
    /// Store prepared LLM messages with attachments for the current request
    pub pending_llm_messages: Option<Vec<LlmMessage>>,
}

impl AttachmentState {
    /// Create a new attachment state
    pub fn new() -> Self {
        Self {
            attached_files: Vec::new(),
            pending_llm_messages: None,
        }
    }

    /// Add a file attachment
    pub fn add_file(&mut self, file_path: String) -> Result<()> {
        // Validate file exists
        if !std::path::Path::new(&file_path).exists() {
            return Err(anyhow::anyhow!("File does not exist: {}", file_path)
                .context("Failed to add attachment"));
        }

        // Check if already attached
        if self.attached_files.contains(&file_path) {
            debug!(file_path = %file_path, "File already attached");
            return Ok(());
        }

        self.attached_files.push(file_path.clone());
        debug!(file_path = %file_path, "Added file attachment");

        Ok(())
    }

    /// Remove a file attachment
    pub fn remove_file(&mut self, file_path: &str) {
        self.attached_files.retain(|f| f != file_path);
        debug!(file_path = %file_path, "Removed file attachment");
    }

    /// Check if a file is attached
    pub fn has_file(&self, file_path: &str) -> bool {
        self.attached_files.iter().any(|f| f == file_path)
    }

    /// Get all attached files
    pub fn get_files(&self) -> &[String] {
        &self.attached_files
    }

    /// Clear all attachments
    pub fn clear(&mut self) {
        self.attached_files.clear();
        self.pending_llm_messages = None;
        debug!("Cleared all attachments");
    }

    /// Create attachments for LLM from attached files
    pub fn create_llm_attachments(&self) -> Result<Vec<crate::llm::Attachment>> {
        let mut attachments = Vec::new();

        for file_path in &self.attached_files {
            match crate::llm::file_utils::create_attachment(file_path) {
                Ok(attachment) => {
                    // Validate file for LLM
                    if let Err(e) = crate::llm::file_utils::validate_file_for_llm(&attachment) {
                        error!(
                            file_path = %file_path,
                            error = %e,
                            "File validation failed"
                        );
                        return Err(e).context(format!("File validation failed for {}", file_path));
                    }
                    attachments.push(attachment);
                }
                Err(e) => {
                    error!(
                        file_path = %file_path,
                        error = %e,
                        "Failed to create attachment"
                    );
                    return Err(e).context(format!("Failed to create attachment for {}", file_path));
                }
            }
        }

        debug!(
            attachment_count = attachments.len(),
            "Created LLM attachments"
        );

        Ok(attachments)
    }

    /// Set pending LLM messages (with attachments)
    pub fn set_pending_llm_messages(&mut self, messages: Vec<LlmMessage>) {
        self.pending_llm_messages = Some(messages);
    }

    /// Get pending LLM messages
    pub fn get_pending_llm_messages(&self) -> Option<&Vec<LlmMessage>> {
        self.pending_llm_messages.as_ref()
    }

    /// Clear pending LLM messages
    pub fn clear_pending_llm_messages(&mut self) {
        self.pending_llm_messages = None;
    }
}

impl Default for AttachmentState {
    fn default() -> Self {
        Self::new()
    }
}

