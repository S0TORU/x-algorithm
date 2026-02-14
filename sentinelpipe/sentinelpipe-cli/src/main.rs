use anyhow::Context;
use clap::{Parser, Subcommand};
use sentinelpipe_core::{RunConfig, RunSummary, TargetProvider};
use sentinelpipe_pipeline::{Pipeline, Source};
use std::sync::Arc;

use sentinelpipe_cli::DiverseTopKSelector;

#[derive(Parser, Debug)]
#[command(name = "gazetent")]
#[command(about = "Gazetent: continuous LLM red-teaming pipeline", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Run {
        #[arg(long)]
        config: String,

        #[arg(long)]
        top_k: Option<usize>,

        #[arg(long, default_value_t = true)]
        print: bool,

        #[arg(long, default_value_t = false)]
        details: bool,
    },
    DryRun {
        #[arg(long)]
        config: String,

        #[arg(long, default_value_t = true)]
        list: bool,

        #[arg(long, default_value_t = false)]
        show_prompts: bool,
    },
}

struct NoopQueryHydrator;

#[async_trait::async_trait]
impl sentinelpipe_pipeline::QueryHydrator for NoopQueryHydrator {
    async fn hydrate(&self, cfg: &RunConfig) -> sentinelpipe_core::Result<RunConfig> {
        Ok(cfg.clone())
    }
}

struct PackSource;

#[async_trait::async_trait]
impl sentinelpipe_pipeline::Source for PackSource {
    async fn generate(&self, cfg: &RunConfig) -> sentinelpipe_core::Result<Vec<sentinelpipe_core::Scenario>> {
        let mut scenarios = Vec::new();
        for pack_path in &cfg.packs {
            scenarios.extend(sentinelpipe_packs::load_pack_file(pack_path)?);
        }
        Ok(scenarios)
    }
}

struct NoopHydrator;

#[async_trait::async_trait]
impl sentinelpipe_pipeline::Hydrator for NoopHydrator {
    async fn hydrate(
        &self,
        _cfg: &RunConfig,
        scenarios: &[sentinelpipe_core::Scenario],
    ) -> sentinelpipe_core::Result<Vec<sentinelpipe_core::Scenario>> {
        Ok(scenarios.to_vec())
    }

    fn update(&self, scenario: &mut sentinelpipe_core::Scenario, hydrated: sentinelpipe_core::Scenario) {
        *scenario = hydrated;
    }
}

struct NoopFilter;

#[async_trait::async_trait]
impl sentinelpipe_pipeline::Filter for NoopFilter {
    async fn filter(
        &self,
        _cfg: &RunConfig,
        scenarios: Vec<sentinelpipe_core::Scenario>,
    ) -> sentinelpipe_core::Result<sentinelpipe_pipeline::FilterResult> {
        Ok(sentinelpipe_pipeline::FilterResult {
            kept: scenarios,
            removed: vec![],
        })
    }
}

struct ArtifactWriter;

#[async_trait::async_trait]
impl sentinelpipe_pipeline::SideEffect for ArtifactWriter {
    async fn run(
        &self,
        cfg: Arc<RunConfig>,
        findings: Arc<Vec<sentinelpipe_core::Finding>>,
    ) -> sentinelpipe_core::Result<()> {
        let run_dir = std::path::Path::new(&cfg.artifacts_dir).join(cfg.run_id.to_string());
        std::fs::create_dir_all(&run_dir)?;

        let findings_path = run_dir.join("findings.jsonl");
        let mut f = std::fs::File::create(&findings_path)?;
        use std::io::Write;
        for finding in findings.iter() {
            writeln!(f, "{}", serde_json::to_string(finding)?)?;
        }

        let summary = RunSummary::from_findings(&cfg, &findings);
        let summary_path = run_dir.join("summary.json");
        std::fs::write(summary_path, serde_json::to_vec_pretty(&summary)?)?;

        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::DryRun {
            config,
            list,
            show_prompts,
        } => {
            let content = std::fs::read_to_string(&config)
                .with_context(|| format!("failed reading config {}", config))?;
            let mut cfg: RunConfig = serde_yaml::from_str(&content)
                .with_context(|| format!("failed parsing yaml {}", config))?;
            if cfg.run_id.is_nil() {
                cfg.run_id = uuid::Uuid::new_v4();
            }

            let scenarios = PackSource.generate(&cfg).await?;
            println!(
                "dry_run_id={} scenarios_loaded={} packs={}",
                cfg.run_id,
                scenarios.len(),
                cfg.packs.len()
            );

            if list {
                for s in &scenarios {
                    println!("- {} [{}]", s.id, s.category);
                    if show_prompts {
                        println!("  prompt: {}", s.prompt);
                    }
                }
            }
        }
        Commands::Run {
            config,
            top_k,
            print,
            details,
        } => {
            let content = std::fs::read_to_string(&config)
                .with_context(|| format!("failed reading config {}", config))?;
            let mut cfg: RunConfig = serde_yaml::from_str(&content)
                .with_context(|| format!("failed parsing yaml {}", config))?;
            if let Some(v) = top_k {
                cfg.top_k = Some(v);
            }
            if cfg.run_id.is_nil() {
                cfg.run_id = uuid::Uuid::new_v4();
            }

            let scenarios = PackSource.generate(&cfg).await?;
            println!(
                "loaded_scenarios={} packs={} provider={:?} model={} baseUrl={}",
                scenarios.len(),
                cfg.packs.len(),
                cfg.target.provider,
                cfg.target.model,
                cfg.target.base_url
            );
            for s in &scenarios {
                println!("- {} [{}]", s.id, s.category);
                if details {
                    println!("  prompt: {}", s.prompt);
                }
            }

            let executor = match cfg.target.provider {
                TargetProvider::OpenAi => sentinelpipe_targets::OpenAiCompatibleExecutor::boxed(),
                TargetProvider::Ollama => sentinelpipe_targets::OllamaExecutor::boxed(),
            };

            let pipeline = Pipeline {
                query_hydrators: vec![Box::new(NoopQueryHydrator)],
                sources: vec![Box::new(PackSource)],
                hydrators: vec![Box::new(NoopHydrator)],
                filters: vec![Box::new(NoopFilter)],
                executor,
                scorers: vec![
                    Box::new(sentinelpipe_scorers::CanaryLeakScorer::new()),
                    Box::new(sentinelpipe_scorers::PromptInjectionHeuristicScorer::new()),
                    Box::new(sentinelpipe_scorers::WeightedRiskScorer::new()),
                ],
                selector: Box::new(DiverseTopKSelector),
                side_effects: vec![Box::new(ArtifactWriter)],
            };

            let findings = pipeline
                .run(cfg.clone())
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;

            let summary = RunSummary::from_findings(&cfg, &findings);
            println!(
                "run_id={} findings={} canary_leaks={} total_risk={:.2} gate_pass={}",
                summary.run_id,
                summary.findings_total,
                summary.canary_leaks,
                summary.total_risk,
                summary.gate_pass
            );

            if print {
                println!("\nResults:");
                for f in &findings {
                    let inj = *f.scores.get("injHeuristic").unwrap_or(&0.0);
                    let leak = *f.scores.get("canaryLeak").unwrap_or(&0.0);
                    println!(
                        "- {} [{}] risk={:.1} inj={:.0} leak={:.0}",
                        f.scenario_id, f.category, f.total_risk, inj, leak
                    );
                    if details {
                        println!("  prompt: {}", f.prompt);
                        println!("  response: {}", f.response_text);
                    }
                }
            }

            if !summary.gate_pass {
                return Err(anyhow::anyhow!("gate failed"));
            }
        }
    }

    Ok(())
}
