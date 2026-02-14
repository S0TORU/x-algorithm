use async_trait::async_trait;
use futures::future::join_all;
use sentinelpipe_core::{Finding, Result, RunConfig, Scenario, SentinelPipeError};
use std::sync::Arc;
use tokio::sync::Semaphore;

#[async_trait]
pub trait QueryHydrator: Send + Sync {
    async fn hydrate(&self, cfg: &RunConfig) -> Result<RunConfig>;
}

#[async_trait]
pub trait Source: Send + Sync {
    async fn generate(&self, cfg: &RunConfig) -> Result<Vec<Scenario>>;
}

#[async_trait]
pub trait Hydrator: Send + Sync {
    async fn hydrate(&self, cfg: &RunConfig, scenarios: &[Scenario]) -> Result<Vec<Scenario>>;

    fn update(&self, scenario: &mut Scenario, hydrated: Scenario);

    fn update_all(&self, scenarios: &mut [Scenario], hydrated: Vec<Scenario>) {
        for (s, h) in scenarios.iter_mut().zip(hydrated) {
            self.update(s, h);
        }
    }
}

#[async_trait]
pub trait Filter: Send + Sync {
    async fn filter(&self, cfg: &RunConfig, scenarios: Vec<Scenario>) -> Result<FilterResult>;
}

#[derive(Debug, Clone)]
pub struct FilterResult {
    pub kept: Vec<Scenario>,
    pub removed: Vec<Scenario>,
}

#[async_trait]
pub trait Executor: Send + Sync {
    async fn execute_one(&self, cfg: &RunConfig, scenario: Scenario) -> Result<Finding>;
}

#[async_trait]
pub trait Scorer: Send + Sync {
    async fn score(&self, cfg: &RunConfig, findings: &[Finding]) -> Result<Vec<Finding>>;

    fn update(&self, finding: &mut Finding, scored: Finding);

    fn update_all(&self, findings: &mut [Finding], scored: Vec<Finding>) {
        for (f, s) in findings.iter_mut().zip(scored) {
            self.update(f, s);
        }
    }
}

pub trait Selector: Send + Sync {
    fn select(&self, cfg: &RunConfig, findings: Vec<Finding>) -> Vec<Finding>;
}

#[async_trait]
pub trait SideEffect: Send + Sync {
    async fn run(&self, cfg: Arc<RunConfig>, findings: Arc<Vec<Finding>>) -> Result<()>;
}

pub struct Pipeline {
    pub query_hydrators: Vec<Box<dyn QueryHydrator>>,
    pub sources: Vec<Box<dyn Source>>,
    pub hydrators: Vec<Box<dyn Hydrator>>,
    pub filters: Vec<Box<dyn Filter>>,
    pub executor: Arc<dyn Executor>,
    pub scorers: Vec<Box<dyn Scorer>>,
    pub selector: Box<dyn Selector>,
    pub side_effects: Vec<Box<dyn SideEffect>>,
}

impl Pipeline {
    pub async fn run(&self, cfg: RunConfig) -> Result<Vec<Finding>> {
        let mut cfg = cfg;

        for qh in &self.query_hydrators {
            cfg = qh.hydrate(&cfg).await?;
        }

        // sources in parallel
        let futs = self.sources.iter().map(|s| s.generate(&cfg));
        let results = join_all(futs).await;
        let mut scenarios = Vec::new();
        for r in results {
            scenarios.extend(r?);
        }

        // hydrators sequential
        for h in &self.hydrators {
            let hydrated = h.hydrate(&cfg, &scenarios).await?;
            if hydrated.len() == scenarios.len() {
                h.update_all(&mut scenarios, hydrated);
            }
        }

        // filters sequential
        for f in &self.filters {
            let res = f.filter(&cfg, scenarios).await?;
            scenarios = res.kept;
        }

        // execute concurrently (bounded)
        let sem = Arc::new(Semaphore::new(cfg.concurrency));
        let exec = Arc::clone(&self.executor);
        let cfg_arc = Arc::new(cfg);

        let tasks = scenarios.into_iter().map(|scenario| {
            let sem = Arc::clone(&sem);
            let exec = Arc::clone(&exec);
            let cfg = Arc::clone(&cfg_arc);
            tokio::spawn(async move {
                let _permit = sem.acquire().await.expect("semaphore closed");
                exec.execute_one(&cfg, scenario).await
            })
        });

        let mut findings = Vec::new();
        for jr in join_all(tasks).await {
            let res = jr.map_err(|e| SentinelPipeError::Target(e.to_string()))?;
            findings.push(res?);
        }

        // scorers sequential
        for s in &self.scorers {
            let scored = s.score(&cfg_arc, &findings).await?;
            if scored.len() == findings.len() {
                s.update_all(&mut findings, scored);
            }
        }

        // selection
        let selected = self.selector.select(&cfg_arc, findings);

        // side effects best-effort, inline
        let cfg_arc2 = Arc::clone(&cfg_arc);
        let findings_arc = Arc::new(selected.clone());
        for se in &self.side_effects {
            let _ = se.run(Arc::clone(&cfg_arc2), Arc::clone(&findings_arc)).await;
        }

        Ok(selected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinelpipe_core::{GateConfig, TargetConfig, TargetProvider};

    struct TestSource;
    #[async_trait]
    impl Source for TestSource {
        async fn generate(&self, _cfg: &RunConfig) -> Result<Vec<Scenario>> {
            Ok(vec![
                Scenario {
                    id: "a".to_string(),
                    category: "cat".to_string(),
                    prompt: "p1".to_string(),
                    system_prompt: None,
                    canary: None,
                },
                Scenario {
                    id: "b".to_string(),
                    category: "cat".to_string(),
                    prompt: "p2".to_string(),
                    system_prompt: None,
                    canary: None,
                },
            ])
        }
    }

    struct DropFilter;
    #[async_trait]
    impl Filter for DropFilter {
        async fn filter(&self, _cfg: &RunConfig, scenarios: Vec<Scenario>) -> Result<FilterResult> {
            let mut kept = Vec::new();
            let mut removed = Vec::new();
            for s in scenarios {
                if s.id == "a" {
                    kept.push(s);
                } else {
                    removed.push(s);
                }
            }
            Ok(FilterResult { kept, removed })
        }
    }

    struct TestExec;
    #[async_trait]
    impl Executor for TestExec {
        async fn execute_one(&self, _cfg: &RunConfig, scenario: Scenario) -> Result<Finding> {
            Ok(Finding {
                scenario_id: scenario.id,
                category: scenario.category,
                prompt: scenario.prompt,
                system_prompt: scenario.system_prompt,
                response_text: "ok".to_string(),
                signals: vec![],
                scores: Default::default(),
                total_risk: 0.0,
            })
        }
    }

    struct IdentitySelector;
    impl Selector for IdentitySelector {
        fn select(&self, _cfg: &RunConfig, findings: Vec<Finding>) -> Vec<Finding> {
            findings
        }
    }

    #[tokio::test]
    async fn pipeline_filters() {
        let pipeline = Pipeline {
            query_hydrators: vec![],
            sources: vec![Box::new(TestSource)],
            hydrators: vec![],
            filters: vec![Box::new(DropFilter)],
            executor: Arc::new(TestExec),
            scorers: vec![],
            selector: Box::new(IdentitySelector),
            side_effects: vec![],
        };

        let cfg = RunConfig {
            run_id: uuid::Uuid::new_v4(),
            target: TargetConfig {
                provider: TargetProvider::OpenAi,
                base_url: "http://localhost".to_string(),
                model: "m".to_string(),
                api_key: None,
            },
            packs: vec![],
            concurrency: 2,
            timeout_ms: 1000,
            max_tokens: 8,
            gate: GateConfig::default(),
            top_k: None,
            artifacts_dir: "gazetent/runs".to_string(),
            metadata: Default::default(),
        };

        let out = pipeline.run(cfg).await.expect("run");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].scenario_id, "a");
    }
}
