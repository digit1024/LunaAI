use super::{LlmError, RateLimitInfo};
use crate::config::LlmProfile;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;

/// Handles rate limiting and retry logic for LLM API calls
pub struct RateLimitHandler {
    profile: LlmProfile,
}

impl RateLimitHandler {
    pub fn new(profile: LlmProfile) -> Self {
        Self { profile }
    }

    /// Determine if we should retry based on attempt count
    pub fn should_retry(&self, attempt_count: u32) -> bool {
        attempt_count < self.profile.get_max_retries()
    }

    /// Calculate exponential backoff delay with jitter
    /// Returns delay in seconds, with max of 60 seconds
    pub fn calculate_backoff_delay(&self, attempt_count: u32) -> u64 {
        if attempt_count == 0 {
            return 1; // First retry after 1 second
        }

        let base = self.profile.get_retry_backoff_base();
        let delay = base.powi(attempt_count as i32) as u64;
        
        // Add jitter: ±25% randomization
        let jitter_range = (delay as f32 * 0.25) as u64;
        let jitter = fastrand::u64(0..=jitter_range * 2);
        let jittered_delay = delay + jitter - jitter_range;
        
        // Cap at 60 seconds
        std::cmp::min(jittered_delay, 60)
    }

    /// Parse retry-after header from HTTP response
    pub fn parse_retry_after_header(&self, retry_after: &str) -> Option<u64> {
        // Try parsing as seconds first
        if let Ok(seconds) = retry_after.parse::<u64>() {
            return Some(seconds);
        }

        // Try parsing as HTTP date format (RFC 7231)
        if let Ok(date) = httpdate::parse_http_date(retry_after) {
            let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;
            let retry_time = date.duration_since(UNIX_EPOCH).ok()?;
            if retry_time > now {
                return Some((retry_time - now).as_secs());
            }
        }

        None
    }

    /// Extract rate limit information from HTTP response headers
    pub fn extract_rate_limit_info(
        &self,
        headers: &reqwest::header::HeaderMap,
        attempt_count: u32,
    ) -> RateLimitInfo {
        let retry_after = headers
            .get("retry-after")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| self.parse_retry_after_header(s));

        let remaining_requests = headers
            .get("x-ratelimit-remaining")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());

        let reset_time = headers
            .get("x-ratelimit-reset")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());

        RateLimitInfo {
            retry_after_seconds: retry_after,
            remaining_requests,
            reset_time,
            provider: self.profile.backend.clone(),
            attempt_count,
        }
    }

    /// Sleep for the specified duration and log the retry attempt
    pub async fn sleep_and_log(&self, delay_seconds: u64, attempt_count: u32) {
        log::warn!(
            "⏱️ Rate limit reached for {} provider. Retrying in {} seconds (attempt {}/{}).",
            self.profile.backend,
            delay_seconds,
            attempt_count + 1,
            self.profile.get_max_retries()
        );
        sleep(Duration::from_secs(delay_seconds)).await;
    }

    /// Handle a rate limit error with retry logic
    pub async fn handle_rate_limit_error(
        &self,
        rate_limit_info: RateLimitInfo,
    ) -> Result<(), LlmError> {
        let attempt_count = rate_limit_info.attempt_count;

        if !self.should_retry(attempt_count) {
            return Err(LlmError::RateLimit(rate_limit_info));
        }

        // Use retry-after header if available, otherwise use exponential backoff
        let delay = rate_limit_info
            .retry_after_seconds
            .unwrap_or_else(|| self.calculate_backoff_delay(attempt_count));

        self.sleep_and_log(delay, attempt_count).await;
        Ok(())
    }

    /// Check if an HTTP status code indicates a rate limit error
    pub fn is_rate_limit_error(status: u16) -> bool {
        status == 429
    }

    /// Parse provider-specific error response to detect rate limiting
    pub fn parse_rate_limit_error(&self, error_text: &str) -> bool {
        match self.profile.backend.as_str() {
            "openai" => {
                error_text.contains("rate_limit_error") || 
                error_text.contains("rate limit") ||
                error_text.contains("quota_exceeded")
            }
            "anthropic" => {
                error_text.contains("rate_limit_error") ||
                error_text.contains("rate limit") ||
                error_text.contains("too_many_requests")
            }
            "gemini" => {
                error_text.contains("RESOURCE_EXHAUSTED") ||
                error_text.contains("quota") ||
                error_text.contains("rate limit")
            }
            "ollama" => {
                // Ollama typically doesn't have rate limits, but check anyway
                error_text.contains("rate limit") ||
                error_text.contains("too many requests")
            }
            _ => {
                // Generic fallback
                error_text.contains("rate limit") ||
                error_text.contains("too many requests") ||
                error_text.contains("429")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_profile() -> LlmProfile {
        LlmProfile {
            backend: "openai".to_string(),
            api_key: "test".to_string(),
            model: "gpt-3.5-turbo".to_string(),
            endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
            temperature: Some(0.7),
            max_tokens: Some(1000),
            context_window_size: Some(128000),
            summarize_threshold: Some(0.7),
            rate_limit_tpm: None,
            max_retries: Some(3),
            retry_backoff_base: Some(2.0),
        }
    }

    #[test]
    fn test_should_retry() {
        let profile = create_test_profile();
        let handler = RateLimitHandler::new(profile);

        assert!(handler.should_retry(0));
        assert!(handler.should_retry(1));
        assert!(handler.should_retry(2));
        assert!(!handler.should_retry(3));
        assert!(!handler.should_retry(4));
    }

    #[test]
    fn test_calculate_backoff_delay() {
        let profile = create_test_profile();
        let handler = RateLimitHandler::new(profile);

        // Test that delays increase exponentially (with some jitter tolerance)
        let delay_1 = handler.calculate_backoff_delay(1);
        let delay_2 = handler.calculate_backoff_delay(2);
        let delay_3 = handler.calculate_backoff_delay(3);

        assert!(delay_1 >= 1 && delay_1 <= 3); // 2^1 ± jitter
        assert!(delay_2 >= 3 && delay_2 <= 5); // 2^2 ± jitter  
        assert!(delay_3 >= 7 && delay_3 <= 9); // 2^3 ± jitter
    }

    #[test]
    fn test_parse_rate_limit_error() {
        let profile = create_test_profile();
        let handler = RateLimitHandler::new(profile);

        assert!(handler.parse_rate_limit_error("rate_limit_error"));
        assert!(handler.parse_rate_limit_error("rate limit exceeded"));
        assert!(handler.parse_rate_limit_error("quota_exceeded"));
        assert!(!handler.parse_rate_limit_error("invalid_api_key"));
    }

    #[test]
    fn test_is_rate_limit_error() {
        assert!(RateLimitHandler::is_rate_limit_error(429));
        assert!(!RateLimitHandler::is_rate_limit_error(200));
        assert!(!RateLimitHandler::is_rate_limit_error(500));
    }
}
