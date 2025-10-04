use anyhow::Result;
use std::fs::OpenOptions;
use std::io::Write;

pub struct ToolLogger {
    log_file: String,
}

impl ToolLogger {
    pub fn new(log_file: String) -> Self {
        Self { log_file }
    }
    
    pub fn log_iteration_start(&self, iteration: u32) -> Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_file)?;
        
        writeln!(file, "\n=== ITERATION {} ===", iteration)?;
        Ok(())
    }
    
    pub fn log_tool_call(&self, tool_call: &crate::llm::ToolCall, iteration: u32) -> Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_file)?;
        
        writeln!(file, "🔧 Tool Call #{}: {}", iteration, tool_call.name)?;
        writeln!(file, "   ID: {}", tool_call.id)?;
        writeln!(file, "   Parameters: {}", tool_call.parameters)?;
        Ok(())
    }
    
    pub fn log_tool_result(&self, tool_call: &crate::llm::ToolCall, result: &str, is_error: bool, iteration: u32) -> Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_file)?;
        
        let status = if is_error { "❌ ERROR" } else { "✅ SUCCESS" };
        writeln!(file, "{} Tool Result #{}: {}", status, iteration, tool_call.name)?;
        writeln!(file, "   Result: {}", result)?;
        Ok(())
    }
    
    pub fn log_final_response(&self, response: &str, iteration: u32) -> Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_file)?;
        
        writeln!(file, "🎯 Final Response (after {} iterations): {}", iteration, response)?;
        Ok(())
    }

    pub fn log_begin_turn(&self, iteration: u32) -> Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_file)?;
        writeln!(file, "--- Begin Turn {} ---", iteration)?;
        Ok(())
    }

    pub fn log_end_turn(&self, iteration: u32) -> Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_file)?;
        writeln!(file, "--- End Turn {} ---", iteration)?;
        Ok(())
    }
}
