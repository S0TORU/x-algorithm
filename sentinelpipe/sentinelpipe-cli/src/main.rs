use anyhow::Context;
use clap::{Parser, Subcommand};
use reqwest::{Client, StatusCode};
use serde::Serialize;
use sentinelpipe_core::{Finding, RunConfig, RunSummary, Scenario, TargetProvider};
use sentinelpipe_pipeline::{Pipeline, Source};
use std::path::{Path, PathBuf};
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

        #[arg(long, default_value_t = false)]
        json: bool,
    },
    DryRun {
        #[arg(long)]
        config: String,

        #[arg(long, default_value_t = true)]
        list: bool,

        #[arg(long, default_value_t = false)]
        show_prompts: bool,

        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Doctor {
        #[arg(long)]
        config: String,

        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Batch {
        #[arg(long = "config", required = true)]
        configs: Vec<String>,

        #[arg(long)]
        top_k: Option<usize>,

        #[arg(long, default_value_t = false)]
        details: bool,

        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

#[derive(Serialize)]
struct DryRunOutput {
    dry_run_id: String,
    scenarios_loaded: usize,
    packs: usize,
    scenarios: Vec<DryRunScenario>,
}

#[derive(Serialize)]
struct DryRunScenario {
    id: String,
    category: String,
    prompt: Option<String>,
}

#[derive(Serialize, Clone)]
struct RunOutput {
    run_id: String,
    provider: String,
    model: String,
    base_url: String,
    packs: usize,
    loaded_scenarios: usize,
    summary: RunSummary,
    findings: Vec<Finding>,
}

#[derive(Serialize)]
struct DoctorOutput {
    ok: bool,
    provider: String,
    base_url: String,
    model: String,
    endpoint: String,
    status: u16,
    status_text: String,
    model_available: bool,
    available_models: Vec<String>,
    detail: String,
}

#[derive(Serialize)]
struct BatchOutput {
    batch_id: String,
    total_runs: usize,
    passed_runs: usize,
    failed_runs: usize,
    items: Vec<BatchItem>,
}

#[derive(Serialize)]
struct BatchItem {
    label: String,
    config_path: String,
    ok: bool,
    error: Option<String>,
    run: Option<RunOutput>,
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
    async fn generate(&self, cfg: &RunConfig) -> sentinelpipe_core::Result<Vec<Scenario>> {
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
        scenarios: &[Scenario],
    ) -> sentinelpipe_core::Result<Vec<Scenario>> {
        Ok(scenarios.to_vec())
    }

    fn update(&self, scenario: &mut Scenario, hydrated: Scenario) {
        *scenario = hydrated;
    }
}

struct NoopFilter;

#[async_trait::async_trait]
impl sentinelpipe_pipeline::Filter for NoopFilter {
    async fn filter(
        &self,
        _cfg: &RunConfig,
        scenarios: Vec<Scenario>,
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
        findings: Arc<Vec<Finding>>,
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

fn load_config(path: &str) -> anyhow::Result<RunConfig> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed reading config {}", path))?;
    let mut cfg: RunConfig = serde_yaml::from_str(&content)
        .with_context(|| format!("failed parsing yaml {}", path))?;
    let config_dir = Path::new(path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let workspace_root = find_workspace_root(&config_dir).unwrap_or_else(|| config_dir.clone());

    cfg.packs = cfg
        .packs
        .iter()
        .map(|pack| {
            let pack_path = Path::new(pack);
            if pack_path.is_absolute() {
                pack.to_string()
            } else {
                let from_config = config_dir.join(pack_path);
                if from_config.exists() {
                    from_config.to_string_lossy().to_string()
                } else {
                    workspace_root.join(pack_path).to_string_lossy().to_string()
                }
            }
        })
        .collect();

    let artifacts_dir = Path::new(&cfg.artifacts_dir);
    if !artifacts_dir.is_absolute() {
        cfg.artifacts_dir = workspace_root.join(artifacts_dir).to_string_lossy().to_string();
    }

    if cfg.run_id.is_nil() {
        cfg.run_id = uuid::Uuid::new_v4();
    }
    Ok(cfg)
}

fn find_workspace_root(start: &Path) -> Option<PathBuf> {
    for dir in start.ancestors() {
        if dir.join("Cargo.toml").exists() {
            return Some(dir.to_path_buf());
        }
    }
    None
}

async fn load_scenarios(cfg: &RunConfig) -> anyhow::Result<Vec<Scenario>> {
    PackSource
        .generate(cfg)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))
}

fn create_pipeline(cfg: &RunConfig) -> Pipeline {
    let executor = match cfg.target.provider {
        TargetProvider::OpenAi => sentinelpipe_targets::OpenAiCompatibleExecutor::boxed(),
        TargetProvider::Ollama => sentinelpipe_targets::OllamaExecutor::boxed(),
    };

    Pipeline {
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
    }
}

async fn execute_run(cfg: RunConfig) -> anyhow::Result<RunOutput> {
    let scenarios = load_scenarios(&cfg).await?;
    let pipeline = create_pipeline(&cfg);
    let findings = pipeline
        .run(cfg.clone())
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let summary = RunSummary::from_findings(&cfg, &findings);
    Ok(RunOutput {
        run_id: cfg.run_id.to_string(),
        provider: format!("{:?}", cfg.target.provider),
        model: cfg.target.model,
        base_url: cfg.target.base_url,
        packs: cfg.packs.len(),
        loaded_scenarios: scenarios.len(),
        summary,
        findings,
    })
}

fn print_run_human(cfg: &RunConfig, scenarios: &[Scenario], out: &RunOutput, details: bool) {
    println!(
        "loaded_scenarios={} packs={} provider={:?} model={} baseUrl={}",
        scenarios.len(),
        cfg.packs.len(),
        cfg.target.provider,
        cfg.target.model,
        cfg.target.base_url
    );
    for s in scenarios {
        println!("- {} [{}]", s.id, s.category);
        if details {
            println!("  prompt: {}", s.prompt);
        }
    }

    println!(
        "run_id={} findings={} canary_leaks={} total_risk={:.2} gate_pass={}",
        out.summary.run_id,
        out.summary.findings_total,
        out.summary.canary_leaks,
        out.summary.total_risk,
        out.summary.gate_pass
    );

    println!("\nResults:");
    for f in &out.findings {
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

fn batch_label(path: &str, idx: usize) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("run-{:02}", idx + 1))
}

async fn probe_openai(client: &Client, cfg: &RunConfig) -> anyhow::Result<DoctorOutput> {
    let base = cfg.target.base_url.trim_end_matches('/');
    let candidates = if base.ends_with("/v1") {
        vec![format!("{}/models", base)]
    } else {
        vec![format!("{}/v1/models", base), format!("{}/models", base)]
    };

    let mut last_status = StatusCode::INTERNAL_SERVER_ERROR;
    let mut last_detail = String::new();

    for endpoint in candidates {
        let mut req = client.get(&endpoint);
        if let Some(key) = &cfg.target.api_key {
            req = req.bearer_auth(key);
        }
        let resp = match req
            .timeout(std::time::Duration::from_millis(cfg.timeout_ms))
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(err) => {
                last_detail = err.to_string();
                continue;
            }
        };

        last_status = resp.status();
        let value: serde_json::Value = resp
            .json()
            .await
            .unwrap_or_else(|_| serde_json::json!({"detail":"invalid json response"}));

        if !last_status.is_success() {
            last_detail = value.to_string();
            continue;
        }

        let models = value
            .get("data")
            .and_then(|x| x.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.get("id").and_then(|v| v.as_str()).map(ToOwned::to_owned))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let model_available = models.iter().any(|m| m == &cfg.target.model);
        let ok = model_available || models.is_empty();
        return Ok(DoctorOutput {
            ok,
            provider: format!("{:?}", cfg.target.provider),
            base_url: cfg.target.base_url.clone(),
            model: cfg.target.model.clone(),
            endpoint,
            status: last_status.as_u16(),
            status_text: last_status.to_string(),
            model_available,
            available_models: models,
            detail: if ok {
                "target reachable".to_string()
            } else {
                "endpoint reachable but configured model was not listed".to_string()
            },
        });
    }

    Ok(DoctorOutput {
        ok: false,
        provider: format!("{:?}", cfg.target.provider),
        base_url: cfg.target.base_url.clone(),
        model: cfg.target.model.clone(),
        endpoint: cfg.target.base_url.clone(),
        status: last_status.as_u16(),
        status_text: if last_status == StatusCode::INTERNAL_SERVER_ERROR {
            "request_failed".to_string()
        } else {
            last_status.to_string()
        },
        model_available: false,
        available_models: vec![],
        detail: if last_detail.is_empty() {
            "target probe failed".to_string()
        } else {
            last_detail
        },
    })
}

async fn probe_ollama(client: &Client, cfg: &RunConfig) -> anyhow::Result<DoctorOutput> {
    let endpoint = format!("{}/api/tags", cfg.target.base_url.trim_end_matches('/'));
    let resp = client
        .get(&endpoint)
        .timeout(std::time::Duration::from_millis(cfg.timeout_ms))
        .send()
        .await;

    match resp {
        Ok(resp) => {
            let status = resp.status();
            let value: serde_json::Value = resp
                .json()
                .await
                .unwrap_or_else(|_| serde_json::json!({"detail":"invalid json response"}));
            if !status.is_success() {
                return Ok(DoctorOutput {
                    ok: false,
                    provider: format!("{:?}", cfg.target.provider),
                    base_url: cfg.target.base_url.clone(),
                    model: cfg.target.model.clone(),
                    endpoint,
                    status: status.as_u16(),
                    status_text: status.to_string(),
                    model_available: false,
                    available_models: vec![],
                    detail: value.to_string(),
                });
            }

            let models = value
                .get("models")
                .and_then(|x| x.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.get("name").and_then(|v| v.as_str()).map(ToOwned::to_owned))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let model_available = models.iter().any(|m| m == &cfg.target.model);
            let ok = model_available || models.is_empty();
            Ok(DoctorOutput {
                ok,
                provider: format!("{:?}", cfg.target.provider),
                base_url: cfg.target.base_url.clone(),
                model: cfg.target.model.clone(),
                endpoint,
                status: status.as_u16(),
                status_text: status.to_string(),
                model_available,
                available_models: models,
                detail: if ok {
                    "target reachable".to_string()
                } else {
                    "endpoint reachable but configured model was not listed".to_string()
                },
            })
        }
        Err(err) => Ok(DoctorOutput {
            ok: false,
            provider: format!("{:?}", cfg.target.provider),
            base_url: cfg.target.base_url.clone(),
            model: cfg.target.model.clone(),
            endpoint,
            status: 0,
            status_text: "request_failed".to_string(),
            model_available: false,
            available_models: vec![],
            detail: err.to_string(),
        }),
    }
}

async fn doctor_config(cfg: &RunConfig) -> anyhow::Result<DoctorOutput> {
    let client = Client::new();
    match cfg.target.provider {
        TargetProvider::OpenAi => probe_openai(&client, cfg).await,
        TargetProvider::Ollama => probe_ollama(&client, cfg).await,
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
            json,
        } => {
            let cfg = load_config(&config)?;
            let scenarios = load_scenarios(&cfg).await?;
            if json {
                let out = DryRunOutput {
                    dry_run_id: cfg.run_id.to_string(),
                    scenarios_loaded: scenarios.len(),
                    packs: cfg.packs.len(),
                    scenarios: scenarios
                        .iter()
                        .map(|s| DryRunScenario {
                            id: s.id.clone(),
                            category: s.category.clone(),
                            prompt: show_prompts.then(|| s.prompt.clone()),
                        })
                        .collect(),
                };
                println!("{}", serde_json::to_string_pretty(&out)?);
                return Ok(());
            }

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
            json,
        } => {
            let mut cfg = load_config(&config)?;
            if let Some(v) = top_k {
                cfg.top_k = Some(v);
            }
            let scenarios = if print && !json {
                Some(load_scenarios(&cfg).await?)
            } else {
                None
            };
            let out = execute_run(cfg.clone()).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else if print {
                print_run_human(&cfg, scenarios.as_deref().unwrap_or(&[]), &out, details);
            }
            if !out.summary.gate_pass {
                return Err(anyhow::anyhow!("gate failed"));
            }
        }
        Commands::Doctor { config, json } => {
            let cfg = load_config(&config)?;
            let out = doctor_config(&cfg).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else {
                println!(
                    "provider={:?} endpoint={} status={} model_available={} ok={}",
                    cfg.target.provider,
                    out.endpoint,
                    out.status_text,
                    out.model_available,
                    out.ok
                );
                if !out.available_models.is_empty() {
                    println!("available_models={}", out.available_models.join(", "));
                }
                println!("detail={}", out.detail);
            }
            if !out.ok {
                return Err(anyhow::anyhow!("doctor failed"));
            }
        }
        Commands::Batch {
            configs,
            top_k,
            details,
            json,
        } => {
            let batch_id = uuid::Uuid::new_v4().to_string();
            let mut passed_runs = 0usize;
            let mut failed_runs = 0usize;
            let mut items = Vec::new();

            for (idx, path) in configs.iter().enumerate() {
                let label = batch_label(path, idx);
                match load_config(path) {
                    Ok(mut cfg) => {
                        if let Some(v) = top_k {
                            cfg.top_k = Some(v);
                        }
                        match execute_run(cfg).await {
                            Ok(run) => {
                                let ok = run.summary.gate_pass;
                                if ok {
                                    passed_runs += 1;
                                } else {
                                    failed_runs += 1;
                                }
                                if !json {
                                    println!(
                                        "label={} run_id={} gate_pass={} findings={} total_risk={:.2}",
                                        label,
                                        run.run_id,
                                        run.summary.gate_pass,
                                        run.summary.findings_total,
                                        run.summary.total_risk
                                    );
                                    if details {
                                        for f in &run.findings {
                                            println!(
                                                "  - {} [{}] risk={:.1}",
                                                f.scenario_id, f.category, f.total_risk
                                            );
                                        }
                                    }
                                }
                                items.push(BatchItem {
                                    label,
                                    config_path: path.clone(),
                                    ok,
                                    error: if ok { None } else { Some("gate failed".to_string()) },
                                    run: Some(run),
                                });
                            }
                            Err(err) => {
                                failed_runs += 1;
                                if !json {
                                    println!("label={} error={}", label, err);
                                }
                                items.push(BatchItem {
                                    label,
                                    config_path: path.clone(),
                                    ok: false,
                                    error: Some(err.to_string()),
                                    run: None,
                                });
                            }
                        }
                    }
                    Err(err) => {
                        failed_runs += 1;
                        if !json {
                            println!("label={} error={}", label, err);
                        }
                        items.push(BatchItem {
                            label,
                            config_path: path.clone(),
                            ok: false,
                            error: Some(err.to_string()),
                            run: None,
                        });
                    }
                }
            }

            let out = BatchOutput {
                batch_id,
                total_runs: items.len(),
                passed_runs,
                failed_runs,
                items,
            };
            if json {
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else {
                println!(
                    "batch_id={} total_runs={} passed_runs={} failed_runs={}",
                    out.batch_id, out.total_runs, out.passed_runs, out.failed_runs
                );
            }
            if out.failed_runs > 0 {
                return Err(anyhow::anyhow!("batch had failures"));
            }
        }
    }

    Ok(())
}
