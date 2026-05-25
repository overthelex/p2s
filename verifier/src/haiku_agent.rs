use crate::agent::{build_agent_prompt, parse_agent_response, AgentAssessment, VerificationAgent};
use anyhow::Result;

/// Production verification agent backed by Claude Haiku via the Anthropic Messages API.
pub struct HaikuAgent {
    api_key: String,
    model: String,
}

impl HaikuAgent {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            model: "claude-haiku-4-5-20251001".into(),
        }
    }

    pub fn with_model(api_key: String, model: String) -> Self {
        Self { api_key, model }
    }
}

impl VerificationAgent for HaikuAgent {
    async fn assess(
        &self,
        card_endpoint: &str,
        card_domain: &str,
        card_label: Option<&str>,
        sanitized_content: &str,
    ) -> Result<AgentAssessment> {
        let (system_prompt, user_prompt) =
            build_agent_prompt(card_endpoint, card_domain, card_label, sanitized_content);

        let request_body = serde_json::json!({
            "model": self.model,
            "max_tokens": 1024,
            "system": system_prompt,
            "messages": [
                {"role": "user", "content": user_prompt}
            ]
        });

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        let response = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic API returned {status}: {body}");
        }

        let resp: serde_json::Value = response.json().await?;

        let text = resp["content"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|block| block["text"].as_str())
            .ok_or_else(|| anyhow::anyhow!("unexpected API response structure"))?;

        parse_agent_response(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn haiku_agent_constructs() {
        let agent = HaikuAgent::new("test-key".into());
        assert_eq!(agent.model, "claude-haiku-4-5-20251001");
    }

    #[test]
    fn haiku_agent_custom_model() {
        let agent = HaikuAgent::with_model("test-key".into(), "claude-sonnet-4-6".into());
        assert_eq!(agent.model, "claude-sonnet-4-6");
    }
}
