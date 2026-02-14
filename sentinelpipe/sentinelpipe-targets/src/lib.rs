use async_trait::async_trait;
use reqwest::Client;
use sentinelpipe_core::{
    Finding, Result, RunConfig, Scenario, SentinelPipeError, Signal, TargetProvider,
};
use serde_json::json;
use std::sync::Arc;
use std::time::Instant;

fn scenario_messages(s: &Scenario) -> Vec<serde_json::Value> {
    let mut msgs = Vec::new();
    if let Some(sp) = &s.system_prompt {
        msgs.push(json!({"role":"system","content": sp}));
    }
    msgs.push(json!({"role":"user","content": s.prompt}));
    msgs
}

pub struct OpenAiCompatibleExecutor {
    client: Client,
}

impl OpenAiCompatibleExecutor {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    pub fn boxed() -> Arc<dyn sentinelpipe_pipeline::Executor> {
        Arc::new(Self::new())
    }
}

pub struct OllamaExecutor {
    client: Client,
}

impl OllamaExecutor {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    pub fn boxed() -> Arc<dyn sentinelpipe_pipeline::Executor> {
        Arc::new(Self::new())
    }
}

#[async_trait]
impl sentinelpipe_pipeline::Executor for OpenAiCompatibleExecutor {
    async fn execute_one(&self, cfg: &RunConfig, scenario: Scenario) -> Result<Finding> {
        if cfg.target.provider != TargetProvider::OpenAi {
            return Err(SentinelPipeError::InvalidConfig(
                "OpenAiCompatibleExecutor used with non-openAi provider".to_string(),
            ));
        }

        let start = Instant::now();

        let base = cfg.target.base_url.trim_end_matches('/');
        let url = if base.ends_with("/v1") {
            format!("{}/chat/completions", base)
        } else {
            format!("{}/v1/chat/completions", base)
        };

        let body = json!({
            "model": cfg.target.model,
            "messages": scenario_messages(&scenario),
            "max_tokens": cfg.max_tokens,
            "temperature": 0,
        });

        let mut req = self.client.post(url).json(&body);
        if let Some(key) = &cfg.target.api_key {
            req = req.bearer_auth(key);
        }

        let resp = req
            .timeout(std::time::Duration::from_millis(cfg.timeout_ms))
            .send()
            .await
            .map_err(|e| SentinelPipeError::Target(e.to_string()))?;

        let status = resp.status();
        let value: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| SentinelPipeError::Target(e.to_string()))?;

        if !status.is_success() {
            return Err(SentinelPipeError::Target(format!(
                "target returned {}: {}",
                status, value
            )));
        }

        let response_text = value
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or_default()
            .to_string();

        let tokens_used = value
            .get("usage")
            .and_then(|u| u.get("total_tokens"))
            .and_then(|t| t.as_u64())
            .unwrap_or(0) as u32;

        let mut finding = Finding {
            scenario_id: scenario.id,
            category: scenario.category,
            prompt: scenario.prompt,
            system_prompt: scenario.system_prompt,
            response_text,
            signals: Vec::new(),
            scores: Default::default(),
            total_risk: 0.0,
        };

        if let Some(canary) = scenario.canary {
            if finding.response_text.contains(&canary) {
                finding.signals.push(Signal::CanaryLeak { marker: canary });
            }
        }

        finding
            .scores
            .insert("tokensUsed".to_string(), tokens_used as f64);
        finding
            .scores
            .insert("latencyMs".to_string(), start.elapsed().as_millis() as f64);

        Ok(finding)
    }
}

#[async_trait]
impl sentinelpipe_pipeline::Executor for OllamaExecutor {
    async fn execute_one(&self, cfg: &RunConfig, scenario: Scenario) -> Result<Finding> {
        if cfg.target.provider != TargetProvider::Ollama {
            return Err(SentinelPipeError::InvalidConfig(
                "OllamaExecutor used with non-ollama provider".to_string(),
            ));
        }

        let start = Instant::now();

        let base = cfg.target.base_url.trim_end_matches('/');
        let url = format!("{}/api/chat", base);

        let body = json!({
            "model": cfg.target.model,
            "messages": scenario_messages(&scenario),
            "stream": false
        });

        let resp = self
            .client
            .post(url)
            .json(&body)
            .timeout(std::time::Duration::from_millis(cfg.timeout_ms))
            .send()
            .await
            .map_err(|e| SentinelPipeError::Target(e.to_string()))?;

        let status = resp.status();
        let value: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| SentinelPipeError::Target(e.to_string()))?;

        if !status.is_success() {
            return Err(SentinelPipeError::Target(format!(
                "target returned {}: {}",
                status, value
            )));
        }

        let response_text = value
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or_default()
            .to_string();

        let tokens_used = value
            .get("eval_count")
            .and_then(|t| t.as_u64())
            .unwrap_or(0) as u32;

        let mut finding = Finding {
            scenario_id: scenario.id,
            category: scenario.category,
            prompt: scenario.prompt,
            system_prompt: scenario.system_prompt,
            response_text,
            signals: Vec::new(),
            scores: Default::default(),
            total_risk: 0.0,
        };

        if let Some(canary) = scenario.canary {
            if finding.response_text.contains(&canary) {
                finding.signals.push(Signal::CanaryLeak { marker: canary });
            }
        }

        finding
            .scores
            .insert("tokensUsed".to_string(), tokens_used as f64);
        finding
            .scores
            .insert("latencyMs".to_string(), start.elapsed().as_millis() as f64);

        Ok(finding)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::Method::POST;
    use httpmock::MockServer;
    use sentinelpipe_pipeline::Executor;

    fn run_config(base_url: &str, provider: TargetProvider) -> RunConfig {
        RunConfig {
            run_id: uuid::Uuid::new_v4(),
            target: sentinelpipe_core::TargetConfig {
                provider,
                base_url: base_url.to_string(),
                model: "test-model".to_string(),
                api_key: None,
            },
            packs: vec![],
            concurrency: 1,
            timeout_ms: 2000,
            max_tokens: 8,
            gate: Default::default(),
            top_k: None,
            artifacts_dir: "gazetent/runs".to_string(),
            metadata: Default::default(),
        }
    }

    // scenario_messages is exercised indirectly by the integration tests below.

    #[tokio::test]
    async fn openai_executor_hits_v1_path() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/v1/chat/completions");
            then.status(200).json_body(serde_json::json!({
                "choices": [{"message": {"content": "ok"}}],
                "usage": {"total_tokens": 12}
            }));
        });

        let exec = OpenAiCompatibleExecutor::new();
        let scenario = Scenario {
            id: "s1".to_string(),
            category: "cat".to_string(),
            prompt: "hello".to_string(),
            system_prompt: None,
            canary: None,
        };

        let finding = exec
            .execute_one(&run_config(&server.base_url(), TargetProvider::OpenAi), scenario)
            .await
            .expect("execute");

        mock.assert();
        assert_eq!(finding.response_text, "ok");
    }

    #[tokio::test]
    async fn ollama_executor_parses_message_content() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/api/chat");
            then.status(200).json_body(serde_json::json!({
                "message": {"role": "assistant", "content": "ok"},
                "eval_count": 42
            }));
        });

        let exec = OllamaExecutor::new();
        let scenario = Scenario {
            id: "s1".to_string(),
            category: "cat".to_string(),
            prompt: "hello".to_string(),
            system_prompt: None,
            canary: None,
        };

        let finding = exec
            .execute_one(&run_config(&server.base_url(), TargetProvider::Ollama), scenario)
            .await
            .expect("execute");

        assert_eq!(finding.response_text, "ok");
        assert_eq!(*finding.scores.get("tokensUsed").unwrap(), 42.0);
    }
}
