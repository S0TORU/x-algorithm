use anyhow::Context;
use clap::{Parser, Subcommand};
use reqwest::{Client, StatusCode};
use sentinelpipe_core::{Finding, RunConfig, RunSummary, Scenario, TargetProvider};
use sentinelpipe_pipeline::{Pipeline, Source};
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::{self, IsTerminal, Write};
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
    Init {
        #[arg(long, default_value = "sentinelpipe.generated.yaml")]
        output: String,

        #[arg(long)]
        provider: Option<String>,

        #[arg(long)]
        base_url: Option<String>,

        #[arg(long)]
        model: Option<String>,

        #[arg(long)]
        preset: Option<String>,

        #[arg(long = "pack")]
        packs: Vec<String>,

        #[arg(long)]
        concurrency: Option<usize>,

        #[arg(long)]
        timeout_ms: Option<u64>,

        #[arg(long)]
        max_tokens: Option<u32>,

        #[arg(long)]
        top_k: Option<usize>,

        #[arg(long)]
        max_canary_leaks: Option<u32>,

        #[arg(long)]
        max_total_risk: Option<f64>,

        #[arg(long, default_value_t = false)]
        force: bool,

        #[arg(long, default_value_t = false)]
        json: bool,
    },
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
    ListPacks {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    ListRuns {
        #[arg(long)]
        artifacts_dir: Option<String>,

        #[arg(long, default_value_t = 20)]
        limit: usize,

        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Compare {
        #[arg(long = "run-id", required = true)]
        run_ids: Vec<String>,

        #[arg(long)]
        artifacts_dir: Option<String>,

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

#[derive(Serialize)]
struct InitOutput {
    config_path: String,
    provider: String,
    base_url: String,
    model: String,
    packs: Vec<String>,
    field_impacts: BTreeMap<String, String>,
    next_steps: Vec<String>,
}

#[derive(Serialize)]
struct PackListOutput {
    presets: Vec<PresetOutput>,
    packs: Vec<PackOutput>,
}

#[derive(Serialize)]
struct PresetOutput {
    name: String,
    impact: String,
    pack_count: usize,
}

#[derive(Serialize)]
struct PackOutput {
    path: String,
    title: String,
    blurb: String,
    suites: Vec<String>,
}

#[derive(Serialize)]
struct RunsListOutput {
    artifacts_dir: String,
    runs: Vec<StoredRunListItem>,
}

#[derive(Serialize)]
struct StoredRunListItem {
    run_id: String,
    modified_ms: u64,
    scenarios_total: Option<usize>,
    summary: Option<RunSummary>,
    provider: Option<String>,
    model: Option<String>,
}

#[derive(Serialize)]
struct CompareOutput {
    artifacts_dir: String,
    base: CompareItem,
    items: Vec<CompareDiff>,
}

#[derive(Serialize)]
struct CompareItem {
    run_id: String,
    scenarios_total: Option<usize>,
    summary: RunSummary,
    provider: Option<String>,
    model: Option<String>,
}

#[derive(Serialize)]
struct CompareDiff {
    run_id: String,
    scenarios_total: Option<usize>,
    summary: RunSummary,
    provider: Option<String>,
    model: Option<String>,
    delta_total_risk: f64,
    delta_canary_leaks: i64,
    delta_findings_total: i64,
    gate_changed: bool,
}

#[derive(Clone)]
struct StoredRunRecord {
    run_id: String,
    modified_ms: u64,
    scenarios_total: Option<usize>,
    summary: Option<RunSummary>,
    config: Option<RunConfig>,
}

struct BuiltinPackMeta {
    path: &'static str,
    title: &'static str,
    blurb: &'static str,
    suites: &'static [&'static str],
}

const BUILTIN_PACKS: &[BuiltinPackMeta] = &[
    BuiltinPackMeta {
        path: "examples/packs/authority_spoofing.yaml",
        title: "Authority Spoofing",
        blurb:
            "Executive, legal, and compliance pressure prompts that impersonate privileged users.",
        suites: &["adversarial"],
    },
    BuiltinPackMeta {
        path: "examples/packs/basic_injection.yaml",
        title: "Core Injection",
        blurb: "Direct override attempts against system and developer instructions.",
        suites: &["core", "adversarial"],
    },
    BuiltinPackMeta {
        path: "examples/packs/canary_leak.yaml",
        title: "Canary Leakage",
        blurb: "Synthetic secret exfiltration tests with deterministic canary checks.",
        suites: &["core", "leakage"],
    },
    BuiltinPackMeta {
        path: "examples/packs/delegation_hijack.yaml",
        title: "Delegation Hijack",
        blurb: "Sub-agent and helper-agent prompts that try to route around policy.",
        suites: &["adversarial"],
    },
    BuiltinPackMeta {
        path: "examples/packs/encoding_evasion.yaml",
        title: "Encoding Evasion",
        blurb: "Base64 and transformation prompts that try to bypass guardrails indirectly.",
        suites: &["adversarial"],
    },
    BuiltinPackMeta {
        path: "examples/packs/jailbreak_suffix.yaml",
        title: "Jailbreak Suffix",
        blurb: "Suffix and override strings meant to flip behavior late in the prompt.",
        suites: &["adversarial"],
    },
    BuiltinPackMeta {
        path: "examples/packs/long_context_override.yaml",
        title: "Long Context Override",
        blurb: "Late-context and long-input instructions designed to outrank earlier policy.",
        suites: &["core", "adversarial"],
    },
    BuiltinPackMeta {
        path: "examples/packs/markdown_injection.yaml",
        title: "Markdown Injection",
        blurb:
            "Markdown, HTML, and code-block formatting prompts that try to expose hidden values.",
        suites: &["adversarial"],
    },
    BuiltinPackMeta {
        path: "examples/packs/memory_poisoning.yaml",
        title: "Memory Poisoning",
        blurb: "Prompts that try to persist unsafe instructions or hidden values across turns.",
        suites: &["core", "adversarial"],
    },
    BuiltinPackMeta {
        path: "examples/packs/multilingual_evasion.yaml",
        title: "Multilingual Evasion",
        blurb: "Non-English and translation-style prompts that try to slip through policy.",
        suites: &["adversarial"],
    },
    BuiltinPackMeta {
        path: "examples/packs/persistent_session.yaml",
        title: "Persistent Session",
        blurb: "Session carryover prompts that try to store and reuse hidden values later.",
        suites: &["core", "adversarial"],
    },
    BuiltinPackMeta {
        path: "examples/packs/policy_override.yaml",
        title: "Policy Override",
        blurb: "Admin/debug framing designed to coerce policy exceptions.",
        suites: &["adversarial"],
    },
    BuiltinPackMeta {
        path: "examples/packs/rag_exfiltration.yaml",
        title: "RAG Exfiltration",
        blurb: "Retrieval-style prompts that ask the model to reveal hidden context.",
        suites: &["core", "leakage"],
    },
    BuiltinPackMeta {
        path: "examples/packs/roleplay_bypass.yaml",
        title: "Roleplay Bypass",
        blurb: "Persona-shift and roleplay prompts that try to weaken refusal behavior.",
        suites: &["adversarial"],
    },
    BuiltinPackMeta {
        path: "examples/packs/schema_bypass.yaml",
        title: "Schema Bypass",
        blurb:
            "Structured output and tool-payload prompts that try to sneak hidden fields through.",
        suites: &["adversarial"],
    },
    BuiltinPackMeta {
        path: "examples/packs/summarization_leakage.yaml",
        title: "Summarization Leakage",
        blurb: "Summaries and synthesis prompts that try to pull internal values into output.",
        suites: &["leakage", "adversarial"],
    },
    BuiltinPackMeta {
        path: "examples/packs/tool_abuse.yaml",
        title: "Tool Abuse",
        blurb:
            "Tool-call prompts that try to expose hidden arguments, connectors, or internal steps.",
        suites: &["core", "adversarial"],
    },
];

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
    let content =
        std::fs::read_to_string(path).with_context(|| format!("failed reading config {}", path))?;
    let mut cfg: RunConfig =
        serde_yaml::from_str(&content).with_context(|| format!("failed parsing yaml {}", path))?;
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
        cfg.artifacts_dir = workspace_root
            .join(artifacts_dir)
            .to_string_lossy()
            .to_string();
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

fn current_workspace_root() -> anyhow::Result<PathBuf> {
    let cwd = std::env::current_dir().context("failed to resolve current directory")?;
    Ok(find_workspace_root(&cwd).unwrap_or(cwd))
}

fn list_builtin_packs() -> Vec<PackOutput> {
    BUILTIN_PACKS
        .iter()
        .map(|meta| PackOutput {
            path: meta.path.to_string(),
            title: meta.title.to_string(),
            blurb: meta.blurb.to_string(),
            suites: meta.suites.iter().map(|suite| suite.to_string()).collect(),
        })
        .collect()
}

fn packs_for_preset(preset: &str) -> anyhow::Result<Vec<String>> {
    let preset = preset.trim().to_lowercase();
    let packs = match preset.as_str() {
        "core" => BUILTIN_PACKS
            .iter()
            .filter(|meta| meta.suites.contains(&"core"))
            .map(|meta| meta.path.to_string())
            .collect(),
        "leakage" => BUILTIN_PACKS
            .iter()
            .filter(|meta| meta.suites.contains(&"leakage"))
            .map(|meta| meta.path.to_string())
            .collect(),
        "adversarial" => BUILTIN_PACKS
            .iter()
            .filter(|meta| meta.suites.contains(&"adversarial"))
            .map(|meta| meta.path.to_string())
            .collect(),
        "all" => BUILTIN_PACKS
            .iter()
            .map(|meta| meta.path.to_string())
            .collect(),
        other => return Err(anyhow::anyhow!("unknown preset: {}", other)),
    };
    Ok(packs)
}

fn normalize_provider(input: &str) -> anyhow::Result<TargetProvider> {
    let normalized = input.trim().to_lowercase();
    match normalized.as_str() {
        "ollama" => Ok(TargetProvider::Ollama),
        "openai" | "openai-compatible" | "openai_compatible" | "openaicompatible" => {
            Ok(TargetProvider::OpenAi)
        }
        _ => Err(anyhow::anyhow!(
            "unsupported provider: {} (use ollama or openai-compatible)",
            input
        )),
    }
}

fn provider_label(provider: TargetProvider) -> &'static str {
    match provider {
        TargetProvider::OpenAi => "openAi",
        TargetProvider::Ollama => "ollama",
    }
}

fn provider_display(provider: TargetProvider) -> &'static str {
    match provider {
        TargetProvider::OpenAi => "OpenAI-compatible",
        TargetProvider::Ollama => "Ollama",
    }
}

fn prompt_line(label: &str, impact: &str, default: Option<&str>) -> anyhow::Result<String> {
    let mut stdout = io::stdout();
    match default {
        Some(default) if !default.is_empty() => {
            write!(stdout, "{} [{}] — {}: ", label, default, impact)?;
        }
        _ => {
            write!(stdout, "{} — {}: ", label, impact)?;
        }
    }
    stdout.flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let value = input.trim();
    if value.is_empty() {
        Ok(default.unwrap_or_default().to_string())
    } else {
        Ok(value.to_string())
    }
}

fn prompt_parse<T>(label: &str, impact: &str, default: T) -> anyhow::Result<T>
where
    T: std::str::FromStr + ToString + Copy,
    <T as std::str::FromStr>::Err: std::fmt::Display,
{
    loop {
        let input = prompt_line(label, impact, Some(&default.to_string()))?;
        match input.parse::<T>() {
            Ok(value) => return Ok(value),
            Err(err) => eprintln!("invalid {}: {}", label, err),
        }
    }
}

fn build_init_output(output: &str, cfg: &RunConfig) -> InitOutput {
    let field_impacts = BTreeMap::from([
        ("base_url".to_string(), "where requests go".to_string()),
        ("model".to_string(), "what gets tested".to_string()),
        ("packs".to_string(), "attack coverage".to_string()),
        ("timeout_ms".to_string(), "request timeout".to_string()),
        ("top_k".to_string(), "scenario cap".to_string()),
        ("max_canary_leaks".to_string(), "allowed leaks".to_string()),
        (
            "max_total_risk".to_string(),
            "aggregate risk gate".to_string(),
        ),
    ]);
    InitOutput {
        config_path: output.to_string(),
        provider: provider_label(cfg.target.provider).to_string(),
        base_url: cfg.target.base_url.clone(),
        model: cfg.target.model.clone(),
        packs: cfg.packs.clone(),
        field_impacts,
        next_steps: vec![
            format!("gazetent doctor --config {}", output),
            format!("gazetent dry-run --config {} --json", output),
            format!("gazetent run --config {} --json", output),
        ],
    }
}

fn build_init_config(
    output: &str,
    provider: TargetProvider,
    base_url: String,
    model: String,
    packs: Vec<String>,
    concurrency: usize,
    timeout_ms: u64,
    max_tokens: u32,
    top_k: Option<usize>,
    max_canary_leaks: u32,
    max_total_risk: f64,
) -> RunConfig {
    let mut metadata = BTreeMap::new();
    metadata.insert(
        "generated_by".to_string(),
        "sentinelpipe-cli init".to_string(),
    );
    metadata.insert("output_path".to_string(), output.to_string());

    RunConfig {
        run_id: uuid::Uuid::nil(),
        target: sentinelpipe_core::TargetConfig {
            provider,
            base_url,
            model,
            api_key: None,
        },
        packs,
        concurrency,
        timeout_ms,
        max_tokens,
        gate: sentinelpipe_core::GateConfig {
            max_canary_leaks,
            max_total_risk,
        },
        top_k,
        artifacts_dir: "gazetent/runs".to_string(),
        metadata,
    }
}

fn resolve_artifacts_dir(input: Option<&str>) -> anyhow::Result<PathBuf> {
    let workspace_root = current_workspace_root()?;
    let path = match input {
        Some(value) => {
            let value_path = Path::new(value);
            if value_path.is_absolute() {
                value_path.to_path_buf()
            } else {
                workspace_root.join(value_path)
            }
        }
        None => workspace_root.join("gazetent").join("runs"),
    };
    Ok(path)
}

fn list_stored_runs(artifacts_dir: &Path) -> anyhow::Result<Vec<StoredRunRecord>> {
    let mut out = Vec::new();
    let rd = match std::fs::read_dir(artifacts_dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(err) => return Err(err.into()),
    };

    for ent in rd {
        let ent = ent?;
        let path = ent.path();
        if !path.is_dir() {
            continue;
        }
        let run_id = match path.file_name().and_then(|value| value.to_str()) {
            Some(value) if uuid::Uuid::parse_str(value).is_ok() => value.to_string(),
            _ => continue,
        };

        let modified_ms = ent
            .metadata()
            .ok()
            .and_then(|md| md.modified().ok())
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0);

        let summary = std::fs::read(path.join("summary.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<RunSummary>(&bytes).ok());
        let config = std::fs::read(path.join("config.redacted.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<RunConfig>(&bytes).ok());
        let scenarios_total = config
            .as_ref()
            .and_then(|cfg| cfg.metadata.get("scenarios_total"))
            .and_then(|value| value.parse::<usize>().ok());

        out.push(StoredRunRecord {
            run_id,
            modified_ms,
            scenarios_total,
            summary,
            config,
        });
    }

    out.sort_by(|a, b| b.modified_ms.cmp(&a.modified_ms));
    Ok(out)
}

fn compare_runs(run_ids: &[String], artifacts_dir: &Path) -> anyhow::Result<CompareOutput> {
    if run_ids.len() < 2 {
        return Err(anyhow::anyhow!("compare needs at least 2 run ids"));
    }
    if run_ids.len() > 5 {
        return Err(anyhow::anyhow!("compare supports at most 5 run ids"));
    }

    let mut loaded = Vec::new();
    for run_id in run_ids {
        let run_dir = artifacts_dir.join(run_id);
        let summary_bytes = std::fs::read(run_dir.join("summary.json"))
            .with_context(|| format!("missing summary for run {}", run_id))?;
        let summary = serde_json::from_slice::<RunSummary>(&summary_bytes)
            .with_context(|| format!("invalid summary for run {}", run_id))?;
        let config = std::fs::read(run_dir.join("config.redacted.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<RunConfig>(&bytes).ok());
        let scenarios_total = config
            .as_ref()
            .and_then(|cfg| cfg.metadata.get("scenarios_total"))
            .and_then(|value| value.parse::<usize>().ok());
        loaded.push((run_id.clone(), summary, scenarios_total, config));
    }

    let (base_run_id, base_summary, base_scenarios, base_config) = loaded.remove(0);
    let base = CompareItem {
        run_id: base_run_id,
        scenarios_total: base_scenarios,
        summary: base_summary.clone(),
        provider: base_config
            .as_ref()
            .map(|cfg| provider_label(cfg.target.provider).to_string()),
        model: base_config.as_ref().map(|cfg| cfg.target.model.clone()),
    };

    let items = loaded
        .into_iter()
        .map(|(run_id, summary, scenarios_total, config)| CompareDiff {
            run_id,
            scenarios_total,
            provider: config
                .as_ref()
                .map(|cfg| provider_label(cfg.target.provider).to_string()),
            model: config.as_ref().map(|cfg| cfg.target.model.clone()),
            delta_total_risk: summary.total_risk - base.summary.total_risk,
            delta_canary_leaks: i64::from(summary.canary_leaks)
                - i64::from(base.summary.canary_leaks),
            delta_findings_total: summary.findings_total as i64
                - base.summary.findings_total as i64,
            gate_changed: summary.gate_pass != base.summary.gate_pass,
            summary,
        })
        .collect();

    Ok(CompareOutput {
        artifacts_dir: artifacts_dir.to_string_lossy().to_string(),
        base,
        items,
    })
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
                    .filter_map(|item| {
                        item.get("id")
                            .and_then(|v| v.as_str())
                            .map(ToOwned::to_owned)
                    })
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
                        .filter_map(|item| {
                            item.get("name")
                                .and_then(|v| v.as_str())
                                .map(ToOwned::to_owned)
                        })
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
        Commands::Init {
            output,
            provider,
            base_url,
            model,
            preset,
            packs,
            concurrency,
            timeout_ms,
            max_tokens,
            top_k,
            max_canary_leaks,
            max_total_risk,
            force,
            json,
        } => {
            let output_path = PathBuf::from(&output);
            if output_path.exists() && !force {
                return Err(anyhow::anyhow!(
                    "output already exists: {} (use --force to overwrite)",
                    output
                ));
            }

            let interactive = io::stdin().is_terminal();
            let provider = match provider {
                Some(value) => normalize_provider(&value)?,
                None if interactive => {
                    let raw = prompt_line("provider", "where requests go", Some("ollama"))?;
                    normalize_provider(&raw)?
                }
                None => TargetProvider::Ollama,
            };

            let default_base_url = match provider {
                TargetProvider::Ollama => "http://localhost:11434",
                TargetProvider::OpenAi => "http://localhost:8000",
            };
            let base_url = match base_url {
                Some(value) => value,
                None if interactive => {
                    prompt_line("base_url", "where requests go", Some(default_base_url))?
                }
                None => default_base_url.to_string(),
            };

            let default_model = match provider {
                TargetProvider::Ollama => "llama3.2:1b",
                TargetProvider::OpenAi => "gpt-4.1-mini",
            };
            let model = match model {
                Some(value) => value,
                None if interactive => {
                    prompt_line("model", "what gets tested", Some(default_model))?
                }
                None => default_model.to_string(),
            };

            let preset_value = match preset {
                Some(value) => value,
                None if !packs.is_empty() => "custom".to_string(),
                None if interactive => prompt_line("preset", "attack coverage", Some("core"))?,
                None => "core".to_string(),
            };

            let mut selected_packs = if packs.is_empty() {
                if preset_value.eq_ignore_ascii_case("custom") {
                    vec![]
                } else {
                    packs_for_preset(&preset_value)?
                }
            } else {
                packs
            };

            if interactive && selected_packs.is_empty() {
                let available = list_builtin_packs();
                println!("available packs:");
                for pack in &available {
                    println!(
                        "- {} [{}] — {}",
                        pack.path,
                        pack.suites.join(", "),
                        pack.blurb
                    );
                }
                let raw = prompt_line(
                    "packs",
                    "attack coverage",
                    Some("examples/packs/basic_injection.yaml"),
                )?;
                selected_packs = raw
                    .split(',')
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
                    .collect();
            }

            if selected_packs.is_empty() {
                return Err(anyhow::anyhow!("no packs selected"));
            }

            let concurrency = match concurrency {
                Some(value) => value,
                None if interactive => prompt_parse("concurrency", "parallel requests", 16usize)?,
                None => 16,
            };
            let timeout_ms = match timeout_ms {
                Some(value) => value,
                None if interactive => prompt_parse("timeout_ms", "request timeout", 60_000u64)?,
                None => 60_000,
            };
            let max_tokens = match max_tokens {
                Some(value) => value,
                None if interactive => prompt_parse("max_tokens", "response cap", 256u32)?,
                None => 256,
            };
            let top_k = match top_k {
                Some(value) => Some(value),
                None if interactive => Some(prompt_parse("top_k", "scenario cap", 50usize)?),
                None => Some(50),
            };
            let max_canary_leaks = match max_canary_leaks {
                Some(value) => value,
                None if interactive => prompt_parse("max_canary_leaks", "allowed leaks", 0u32)?,
                None => 0,
            };
            let max_total_risk = match max_total_risk {
                Some(value) => value,
                None if interactive => {
                    prompt_parse("max_total_risk", "aggregate risk gate", 50.0f64)?
                }
                None => 50.0,
            };

            let cfg = build_init_config(
                &output,
                provider,
                base_url,
                model,
                selected_packs,
                concurrency,
                timeout_ms,
                max_tokens,
                top_k,
                max_canary_leaks,
                max_total_risk,
            );

            if let Some(parent) = output_path
                .parent()
                .filter(|path| !path.as_os_str().is_empty())
            {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&output_path, serde_yaml::to_string(&cfg)?)?;

            let out = build_init_output(&output, &cfg);
            if json {
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else {
                println!("wrote config={}", out.config_path);
                println!(
                    "provider={} model={} base_url={}",
                    provider_display(cfg.target.provider),
                    cfg.target.model,
                    cfg.target.base_url
                );
                println!("packs={}", out.packs.join(", "));
                println!("field impacts:");
                for (field, impact) in &out.field_impacts {
                    println!("- {}: {}", field, impact);
                }
                println!("next:");
                for step in &out.next_steps {
                    println!("- {}", step);
                }
            }
        }
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
                    cfg.target.provider, out.endpoint, out.status_text, out.model_available, out.ok
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
                                    error: if ok {
                                        None
                                    } else {
                                        Some("gate failed".to_string())
                                    },
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
        Commands::ListPacks { json } => {
            let out = PackListOutput {
                presets: vec![
                    PresetOutput {
                        name: "core".to_string(),
                        impact: "starter deterministic coverage".to_string(),
                        pack_count: packs_for_preset("core")?.len(),
                    },
                    PresetOutput {
                        name: "leakage".to_string(),
                        impact: "focus on exfiltration and summaries".to_string(),
                        pack_count: packs_for_preset("leakage")?.len(),
                    },
                    PresetOutput {
                        name: "adversarial".to_string(),
                        impact: "broader jailbreak and bypass pressure".to_string(),
                        pack_count: packs_for_preset("adversarial")?.len(),
                    },
                    PresetOutput {
                        name: "all".to_string(),
                        impact: "maximum built-in coverage".to_string(),
                        pack_count: packs_for_preset("all")?.len(),
                    },
                ],
                packs: list_builtin_packs(),
            };
            if json {
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else {
                println!("presets:");
                for preset in &out.presets {
                    println!(
                        "- {} [{} packs] — {}",
                        preset.name, preset.pack_count, preset.impact
                    );
                }
                println!("\nbuilt-in packs:");
                for pack in &out.packs {
                    println!(
                        "- {} [{}] — {}",
                        pack.path,
                        pack.suites.join(", "),
                        pack.blurb
                    );
                }
            }
        }
        Commands::ListRuns {
            artifacts_dir,
            limit,
            json,
        } => {
            let artifacts_dir = resolve_artifacts_dir(artifacts_dir.as_deref())?;
            let mut runs = list_stored_runs(&artifacts_dir)?;
            runs.truncate(limit);
            let out = RunsListOutput {
                artifacts_dir: artifacts_dir.to_string_lossy().to_string(),
                runs: runs
                    .into_iter()
                    .map(|run| StoredRunListItem {
                        run_id: run.run_id,
                        modified_ms: run.modified_ms,
                        scenarios_total: run.scenarios_total,
                        summary: run.summary,
                        provider: run
                            .config
                            .as_ref()
                            .map(|cfg| provider_label(cfg.target.provider).to_string()),
                        model: run.config.as_ref().map(|cfg| cfg.target.model.clone()),
                    })
                    .collect(),
            };
            if json {
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else if out.runs.is_empty() {
                println!("no runs found in {}", out.artifacts_dir);
            } else {
                println!("artifacts_dir={}", out.artifacts_dir);
                for run in &out.runs {
                    let gate = run
                        .summary
                        .as_ref()
                        .map(|summary| if summary.gate_pass { "pass" } else { "fail" })
                        .unwrap_or("—");
                    let risk = run
                        .summary
                        .as_ref()
                        .map(|summary| format!("{:.1}", summary.total_risk))
                        .unwrap_or_else(|| "—".to_string());
                    let model = run.model.clone().unwrap_or_else(|| "—".to_string());
                    println!(
                        "- {} gate={} risk={} model={}",
                        run.run_id, gate, risk, model
                    );
                }
            }
        }
        Commands::Compare {
            run_ids,
            artifacts_dir,
            json,
        } => {
            let artifacts_dir = resolve_artifacts_dir(artifacts_dir.as_deref())?;
            let out = compare_runs(&run_ids, &artifacts_dir)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else {
                println!(
                    "base={} risk={:.1} leaks={} gate={}",
                    out.base.run_id,
                    out.base.summary.total_risk,
                    out.base.summary.canary_leaks,
                    if out.base.summary.gate_pass {
                        "pass"
                    } else {
                        "fail"
                    }
                );
                for item in &out.items {
                    println!(
                        "- {} Δrisk={:.1} Δleaks={} Δfindings={} gateChanged={}",
                        item.run_id,
                        item.delta_total_risk,
                        item.delta_canary_leaks,
                        item.delta_findings_total,
                        if item.gate_changed { "yes" } else { "no" }
                    );
                }
            }
        }
    }

    Ok(())
}
