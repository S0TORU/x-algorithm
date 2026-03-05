use axum::{
    body::Body,
    extract::DefaultBodyLimit,
    extract::State,
    extract::Path,
    http::Request,
    http::StatusCode,
    middleware::Next,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, path::PathBuf, sync::Arc};
use tower_http::services::ServeDir;

#[derive(Clone)]
struct AppState {
    workspace_root: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let static_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("static");
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf();
    let state = AppState { workspace_root };

    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/packs/list", get(list_packs))
        .route("/api/packs/preview", post(preview_packs))
        .route("/api/run", post(run_job))
        .route("/api/run/batch", post(run_batch))
        .route("/api/runs/list", get(list_runs))
        .route("/api/runs/compare", post(compare_runs))
        .route("/api/runs/:run_id", get(get_run))
        .route("/api/runs/:run_id/download/:file", get(download_run_file))
        .route("/", get(index))
        .nest_service("/static", ServeDir::new(static_dir))
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .layer(axum::middleware::from_fn(security_headers))
        .with_state(Arc::new(state));

    let addr = SocketAddr::from(([127, 0, 0, 1], 8787));
    println!("Gazetent UI: http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PacksListResponse {
    packs: Vec<String>,
}

async fn list_packs(State(state): State<Arc<AppState>>) -> Result<Json<PacksListResponse>, (StatusCode, String)> {
    let base = state.workspace_root.join("examples").join("packs");
    let mut packs = Vec::new();
    let rd = std::fs::read_dir(&base)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    for ent in rd {
        let ent = ent.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let path = ent.path();
        if path.extension().and_then(|s| s.to_str()) != Some("yaml") {
            continue;
        }
        if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
            packs.push(format!("examples/packs/{}", name));
        }
    }
    packs.sort();
    Ok(Json(PacksListResponse { packs }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackPreviewRequest {
    pack_paths: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PackPreviewResponse {
    scenarios_loaded: usize,
    scenarios: Vec<ScenarioPreview>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScenarioPreview {
    id: String,
    category: String,
    prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_prompt: Option<String>,
    canary: bool,
}

async fn preview_packs(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PackPreviewRequest>,
) -> Result<Json<PackPreviewResponse>, (StatusCode, String)> {
    let mut scenarios = Vec::new();
    for path in req.pack_paths {
        let path = resolve_pack_path(&state.workspace_root, &path)?;
        let mut pack = sentinelpipe_packs::load_pack_file(path)
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
        scenarios.append(&mut pack);
    }

    let previews = scenarios
        .into_iter()
        .map(|s| ScenarioPreview {
            id: s.id,
            category: s.category,
            prompt: s.prompt,
            system_prompt: s.system_prompt,
            canary: s.canary.is_some(),
        })
        .collect::<Vec<_>>();

    Ok(Json(PackPreviewResponse {
        scenarios_loaded: previews.len(),
        scenarios: previews,
    }))
}

fn resolve_path(workspace_root: &std::path::Path, input: &str) -> std::path::PathBuf {
    let p = std::path::Path::new(input);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        workspace_root.join(p)
    }
}

fn resolve_pack_path(
    workspace_root: &std::path::Path,
    input: &str,
) -> Result<std::path::PathBuf, (StatusCode, String)> {
    let raw = resolve_path(workspace_root, input);
    let canonical = raw
        .canonicalize()
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid pack path: {}", e)))?;
    let root = workspace_root
        .canonicalize()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if !canonical.starts_with(&root) {
        return Err((
            StatusCode::BAD_REQUEST,
            "pack path must be inside workspace root".to_string(),
        ));
    }
    Ok(canonical)
}

async fn security_headers(req: Request<Body>, next: Next) -> Response {
    let mut res = next.run(req).await;
    let headers = res.headers_mut();
    headers.insert(
        axum::http::header::X_CONTENT_TYPE_OPTIONS,
        axum::http::HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        axum::http::header::X_FRAME_OPTIONS,
        axum::http::HeaderValue::from_static("DENY"),
    );
    headers.insert(
        axum::http::header::REFERRER_POLICY,
        axum::http::HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        axum::http::header::CONTENT_SECURITY_POLICY,
        axum::http::HeaderValue::from_static(
            "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; base-uri 'self'; frame-ancestors 'none'; form-action 'self'",
        ),
    );
    headers.insert(
        axum::http::header::HeaderName::from_static("permissions-policy"),
        axum::http::HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    res
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct RunRequest {
    provider: sentinelpipe_core::TargetProvider,
    base_url: String,
    model: String,
    api_key: Option<String>,
    pack_paths: Vec<String>,
    concurrency: usize,
    timeout_ms: u64,
    max_tokens: u32,
    max_canary_leaks: u32,
    max_total_risk: f64,
    top_k: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunResponse {
    run_id: String,
    scenarios_total: usize,
    summary: sentinelpipe_core::RunSummary,
    findings: Vec<sentinelpipe_core::Finding>,
    artifacts: ArtifactsInfo,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ArtifactsInfo {
    dir: String,
    summary_url: String,
    findings_url: String,
    config_url: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchRunResponse {
    batch_id: String,
    total_runs: usize,
    passed_runs: usize,
    failed_runs: usize,
    items: Vec<BatchRunItem>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchRunItem {
    label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scenarios_total: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<sentinelpipe_core::RunSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    artifacts: Option<ArtifactsInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchRunRequest {
    runs: Vec<BatchRunSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchRunSpec {
    #[serde(default)]
    label: Option<String>,
    #[serde(flatten)]
    run: RunRequest,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompareRunsRequest {
    run_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CompareRunsResponse {
    base: CompareRunItem,
    items: Vec<CompareRunDiff>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CompareRunItem {
    run_id: String,
    scenarios_total: Option<usize>,
    summary: sentinelpipe_core::RunSummary,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CompareRunDiff {
    run_id: String,
    scenarios_total: Option<usize>,
    summary: sentinelpipe_core::RunSummary,
    delta_total_risk: f64,
    delta_canary_leaks: i64,
    delta_findings_total: i64,
    gate_changed: bool,
}

#[derive(Debug)]
struct StoredRunData {
    run_id: String,
    scenarios_total: Option<usize>,
    summary: sentinelpipe_core::RunSummary,
    findings: Vec<sentinelpipe_core::Finding>,
    artifacts: ArtifactsInfo,
}

struct NoopQueryHydrator;
#[async_trait::async_trait]
impl sentinelpipe_pipeline::QueryHydrator for NoopQueryHydrator {
    async fn hydrate(&self, cfg: &sentinelpipe_core::RunConfig) -> sentinelpipe_core::Result<sentinelpipe_core::RunConfig> {
        Ok(cfg.clone())
    }
}

struct PackSource;
#[async_trait::async_trait]
impl sentinelpipe_pipeline::Source for PackSource {
    async fn generate(&self, cfg: &sentinelpipe_core::RunConfig) -> sentinelpipe_core::Result<Vec<sentinelpipe_core::Scenario>> {
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
        _cfg: &sentinelpipe_core::RunConfig,
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
        _cfg: &sentinelpipe_core::RunConfig,
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
        cfg: Arc<sentinelpipe_core::RunConfig>,
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

        let summary = sentinelpipe_core::RunSummary::from_findings(&cfg, &findings);
        let summary_path = run_dir.join("summary.json");
        std::fs::write(summary_path, serde_json::to_vec_pretty(&summary)?)?;

        // Never write secrets into artifacts. These are useful to share for audit/debug.
        let mut redacted = (*cfg).clone();
        redacted.target.api_key = None;
        let cfg_path = run_dir.join("config.redacted.json");
        std::fs::write(cfg_path, serde_json::to_vec_pretty(&redacted)?)?;

        Ok(())
    }
}

async fn run_job(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RunRequest>,
) -> Result<Json<RunResponse>, (StatusCode, String)> {
    let out = execute_run_request(&state, &req).await?;
    Ok(Json(RunResponse {
        run_id: out.run_id,
        scenarios_total: out.scenarios_total,
        summary: out.summary,
        findings: out.findings,
        artifacts: out.artifacts,
    }))
}

async fn run_batch(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchRunRequest>,
) -> Result<Json<BatchRunResponse>, (StatusCode, String)> {
    if req.runs.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "runs must not be empty".to_string()));
    }
    if req.runs.len() > 20 {
        return Err((StatusCode::BAD_REQUEST, "runs must be <= 20".to_string()));
    }

    let batch_id = uuid::Uuid::new_v4().to_string();
    let mut items = Vec::new();
    let mut passed_runs = 0usize;
    let mut failed_runs = 0usize;

    for (idx, spec) in req.runs.iter().enumerate() {
        let label = spec
            .label
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| format!("run-{:02}", idx + 1));
        match execute_run_request(&state, &spec.run).await {
            Ok(out) => {
                if out.summary.gate_pass {
                    passed_runs += 1;
                } else {
                    failed_runs += 1;
                }
                items.push(BatchRunItem {
                    label,
                    run_id: Some(out.run_id),
                    scenarios_total: Some(out.scenarios_total),
                    summary: Some(out.summary),
                    artifacts: Some(out.artifacts),
                    error: None,
                });
            }
            Err((_, e)) => {
                failed_runs += 1;
                items.push(BatchRunItem {
                    label,
                    run_id: None,
                    scenarios_total: None,
                    summary: None,
                    artifacts: None,
                    error: Some(e),
                });
            }
        }
    }

    Ok(Json(BatchRunResponse {
        batch_id,
        total_runs: items.len(),
        passed_runs,
        failed_runs,
        items,
    }))
}

async fn compare_runs(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CompareRunsRequest>,
) -> Result<Json<CompareRunsResponse>, (StatusCode, String)> {
    if req.run_ids.len() < 2 {
        return Err((StatusCode::BAD_REQUEST, "runIds must include at least 2 run ids".to_string()));
    }
    if req.run_ids.len() > 5 {
        return Err((StatusCode::BAD_REQUEST, "runIds must be <= 5".to_string()));
    }

    let mut loaded = Vec::new();
    for run_id in &req.run_ids {
        loaded.push(load_run_data(&state, run_id)?);
    }

    let base = loaded.remove(0);
    let base_item = CompareRunItem {
        run_id: base.run_id.clone(),
        scenarios_total: base.scenarios_total,
        summary: base.summary.clone(),
    };

    let mut items = Vec::new();
    for run in loaded {
        items.push(CompareRunDiff {
            run_id: run.run_id,
            scenarios_total: run.scenarios_total,
            summary: run.summary.clone(),
            delta_total_risk: run.summary.total_risk - base.summary.total_risk,
            delta_canary_leaks: i64::from(run.summary.canary_leaks) - i64::from(base.summary.canary_leaks),
            delta_findings_total: run.summary.findings_total as i64 - base.summary.findings_total as i64,
            gate_changed: run.summary.gate_pass != base.summary.gate_pass,
        });
    }

    Ok(Json(CompareRunsResponse {
        base: base_item,
        items,
    }))
}

async fn execute_run_request(
    state: &AppState,
    req: &RunRequest,
) -> Result<RunResponse, (StatusCode, String)> {
    if req.base_url.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "baseUrl is required".to_string()));
    }
    if req.model.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "model is required".to_string()));
    }
    if req.pack_paths.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "at least one pack is required".to_string()));
    }

    let packs_abs = req
        .pack_paths
        .iter()
        .map(|p| resolve_pack_path(&state.workspace_root, p).map(|x| x.to_string_lossy().to_string()))
        .collect::<Result<Vec<_>, _>>()?;

    let mut scenarios_total = 0usize;
    for pack_path in &packs_abs {
        let loaded =
            sentinelpipe_packs::load_pack_file(pack_path).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
        scenarios_total += loaded.len();
    }

    let mut metadata = std::collections::BTreeMap::new();
    metadata.insert("scenarios_total".to_string(), scenarios_total.to_string());

    let cfg = sentinelpipe_core::RunConfig {
        run_id: uuid::Uuid::new_v4(),
        target: sentinelpipe_core::TargetConfig {
            provider: req.provider,
            base_url: req.base_url.clone(),
            model: req.model.clone(),
            api_key: req.api_key.clone().filter(|s| !s.trim().is_empty()),
        },
        packs: packs_abs,
        concurrency: req.concurrency.max(1).min(256),
        timeout_ms: req.timeout_ms.max(1000),
        max_tokens: req.max_tokens.max(1).min(8192),
        gate: sentinelpipe_core::GateConfig {
            max_canary_leaks: req.max_canary_leaks,
            max_total_risk: req.max_total_risk,
        },
        top_k: req.top_k,
        artifacts_dir: resolve_path(&state.workspace_root, "gazetent/runs")
            .to_string_lossy()
            .to_string(),
        metadata,
    };

    // Ensure we can create the artifacts directory early.
    std::fs::create_dir_all(&cfg.artifacts_dir)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let executor = match cfg.target.provider {
        sentinelpipe_core::TargetProvider::OpenAi => sentinelpipe_targets::OpenAiCompatibleExecutor::boxed(),
        sentinelpipe_core::TargetProvider::Ollama => sentinelpipe_targets::OllamaExecutor::boxed(),
    };

    let pipeline = sentinelpipe_pipeline::Pipeline {
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
        selector: Box::new(sentinelpipe_cli::DiverseTopKSelector),
        side_effects: vec![Box::new(ArtifactWriter)],
    };

    let findings = pipeline
        .run(cfg.clone())
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let summary = sentinelpipe_core::RunSummary::from_findings(&cfg, &findings);
    let artifacts_dir = std::path::Path::new(&cfg.artifacts_dir).join(cfg.run_id.to_string());
    let artifacts = ArtifactsInfo {
        dir: artifacts_dir.to_string_lossy().to_string(),
        summary_url: format!("/api/runs/{}/download/summary.json", cfg.run_id),
        findings_url: format!("/api/runs/{}/download/findings.jsonl", cfg.run_id),
        config_url: format!("/api/runs/{}/download/config.redacted.json", cfg.run_id),
    };
    Ok(RunResponse {
        run_id: cfg.run_id.to_string(),
        scenarios_total,
        summary,
        findings,
        artifacts,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunsListResponse {
    runs: Vec<RunListItem>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunListItem {
    run_id: String,
    modified_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    scenarios_total: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<sentinelpipe_core::RunSummary>,
}

async fn list_runs(State(state): State<Arc<AppState>>) -> Result<Json<RunsListResponse>, (StatusCode, String)> {
    let base = resolve_path(&state.workspace_root, "gazetent/runs");
    let mut out = Vec::new();
    let rd = match std::fs::read_dir(&base) {
        Ok(x) => x,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Json(RunsListResponse { runs: vec![] }));
        }
        Err(e) => return Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    };

    for ent in rd {
        let ent = ent.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let path = ent.path();
        if !path.is_dir() {
            continue;
        }
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if uuid::Uuid::parse_str(&name).is_err() {
            continue;
        }

        let md = ent
            .metadata()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let modified_ms = md
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let summary_path = path.join("summary.json");
        let summary = std::fs::read(&summary_path)
            .ok()
            .and_then(|b| serde_json::from_slice::<sentinelpipe_core::RunSummary>(&b).ok());

        let cfg_path = path.join("config.redacted.json");
        let scenarios_total = std::fs::read(&cfg_path)
            .ok()
            .and_then(|b| serde_json::from_slice::<sentinelpipe_core::RunConfig>(&b).ok())
            .and_then(|cfg| cfg.metadata.get("scenarios_total").and_then(|s| s.parse::<usize>().ok()));

        out.push(RunListItem {
            run_id: name,
            modified_ms,
            scenarios_total,
            summary,
        });
    }

    out.sort_by(|a, b| b.modified_ms.cmp(&a.modified_ms));
    Ok(Json(RunsListResponse { runs: out }))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GetRunResponse {
    run_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    scenarios_total: Option<usize>,
    summary: sentinelpipe_core::RunSummary,
    findings: Vec<sentinelpipe_core::Finding>,
    artifacts: ArtifactsInfo,
}

async fn get_run(
    State(state): State<Arc<AppState>>,
    Path(run_id): Path<String>,
) -> Result<Json<GetRunResponse>, (StatusCode, String)> {
    let run = load_run_data(&state, &run_id)?;

    Ok(Json(GetRunResponse {
        run_id: run.run_id,
        scenarios_total: run.scenarios_total,
        summary: run.summary,
        findings: run.findings,
        artifacts: run.artifacts,
    }))
}

fn load_run_data(state: &AppState, run_id: &str) -> Result<StoredRunData, (StatusCode, String)> {
    let run_uuid = uuid::Uuid::parse_str(run_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid run id".to_string()))?;
    let base = resolve_path(&state.workspace_root, "gazetent/runs");
    let run_dir = base.join(run_uuid.to_string());

    let summary_path = run_dir.join("summary.json");
    let summary_bytes = std::fs::read(&summary_path).map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;
    let summary: sentinelpipe_core::RunSummary =
        serde_json::from_slice(&summary_bytes).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let findings_path = run_dir.join("findings.jsonl");
    let content = std::fs::read_to_string(&findings_path).map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;
    let mut findings = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let finding: sentinelpipe_core::Finding =
            serde_json::from_str(line).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        findings.push(finding);
    }

    let cfg_path = run_dir.join("config.redacted.json");
    let scenarios_total = std::fs::read(&cfg_path)
        .ok()
        .and_then(|b| serde_json::from_slice::<sentinelpipe_core::RunConfig>(&b).ok())
        .and_then(|cfg| cfg.metadata.get("scenarios_total").and_then(|s| s.parse::<usize>().ok()));

    let artifacts = ArtifactsInfo {
        dir: run_dir.to_string_lossy().to_string(),
        summary_url: format!("/api/runs/{}/download/summary.json", run_uuid),
        findings_url: format!("/api/runs/{}/download/findings.jsonl", run_uuid),
        config_url: format!("/api/runs/{}/download/config.redacted.json", run_uuid),
    };

    Ok(StoredRunData {
        run_id: run_uuid.to_string(),
        scenarios_total,
        summary,
        findings,
        artifacts,
    })
}

async fn download_run_file(
    State(state): State<Arc<AppState>>,
    Path((run_id, file)): Path<(String, String)>,
) -> Result<Response, (StatusCode, String)> {
    let run_uuid = uuid::Uuid::parse_str(&run_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid run id".to_string()))?;
    let allowed = match file.as_str() {
        "summary.json" | "findings.jsonl" | "config.redacted.json" => file,
        _ => return Err((StatusCode::BAD_REQUEST, "invalid file".to_string())),
    };

    let base = resolve_path(&state.workspace_root, "gazetent/runs");
    let run_dir = base.join(run_uuid.to_string());
    let path = run_dir.join(&allowed);
    let bytes = std::fs::read(&path).map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;
    let ctype = if allowed.ends_with(".jsonl") {
        "application/x-ndjson"
    } else {
        "application/json"
    };

    let mut res = axum::response::Response::new(bytes.into());
    *res.status_mut() = StatusCode::OK;
    let headers = res.headers_mut();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_str(ctype).unwrap(),
    );
    headers.insert(
        axum::http::header::CONTENT_DISPOSITION,
        axum::http::HeaderValue::from_str(&format!(
            "attachment; filename=\"{}-{}\"",
            run_uuid, allowed
        ))
        .unwrap(),
    );
    Ok(res)
}

const INDEX_HTML: &str = r##"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Gazetent</title>
    <link rel="stylesheet" href="/static/app.css" />
  </head>
  <body>
    <div class="wrap">
      <div class="topbar">
        <div class="brand">
          <div class="h1">Gazetent</div>
          <div class="sub">Local console for LLM safety testing. Preview packs, run jobs, inspect failures.</div>
        </div>
        <div class="right">
          <span class="pill"><span id="healthDot" class="dot"></span><span id="health">checking…</span></span>
          <span class="pill" id="count">scenarios_loaded=0</span>
          <span class="pill" id="note">preview</span>
        </div>
      </div>

      <div class="grid">
        <div class="stack">
          <div class="card">
            <div class="cardTitle"><h2>Target</h2></div>
            <div class="formGrid">
              <div>
                <label class="label">Provider</label>
                <select id="provider" class="select">
                  <option value="openAi">OpenAI-compatible</option>
                  <option value="ollama">Ollama</option>
                </select>
              </div>
              <div>
                <label class="label">Model</label>
                <input id="model" class="input" placeholder="e.g. meta-llama/Meta-Llama-3.1-8B-Instruct" />
              </div>
            </div>
            <label class="label">Base URL</label>
            <input id="baseUrl" class="input" placeholder="http://localhost:8000" />

            <label class="label">API Key (optional)</label>
            <input id="apiKey" class="input" type="password" placeholder="sk-..." autocomplete="off" />

            <div class="formGrid">
              <div>
                <label class="label">Concurrency</label>
                <input id="concurrency" class="input" inputmode="numeric" placeholder="16" />
              </div>
              <div>
                <label class="label">Timeout (ms)</label>
                <input id="timeoutMs" class="input" inputmode="numeric" placeholder="60000" />
              </div>
            </div>
            <div class="formGrid">
              <div>
                <label class="label">Max Tokens</label>
                <input id="maxTokens" class="input" inputmode="numeric" placeholder="256" />
              </div>
              <div>
                <label class="label">Top-K</label>
                <input id="topK" class="input" inputmode="numeric" placeholder="50" />
              </div>
            </div>
            <div class="formGrid">
              <div>
                <label class="label">Gate: Max Canary Leaks</label>
                <input id="maxCanaryLeaks" class="input" inputmode="numeric" placeholder="0" />
              </div>
              <div>
                <label class="label">Gate: Max Total Risk</label>
                <input id="maxTotalRisk" class="input" inputmode="numeric" placeholder="50" />
              </div>
            </div>
            <div class="actions">
              <div class="seg" role="tablist" aria-label="Mode">
                <button class="segBtn active" id="modeSingleBtn" type="button">Single</button>
                <button class="segBtn" id="modeBatchBtn" type="button">Batch</button>
              </div>
              <button class="btn btnPrimary" id="runBtn">Run Single</button>
              <button class="btn" id="previewBtn">Preview Packs</button>
            </div>
            <div class="hint">Workflow: Connect target -> select packs -> run single or batch -> inspect findings -> export artifacts.</div>
          </div>

          <div class="card">
            <div class="cardTitle"><h2>Packs</h2></div>
            <div id="packList" class="packList"></div>
            <label class="label">Pack paths (one per line)</label>
            <textarea id="packs" class="textarea">examples/packs/basic_injection.yaml
examples/packs/canary_leak.yaml</textarea>
            <div class="actions">
              <button class="btn" id="clearBtn">Clear</button>
            </div>
            <div class="hint">Tip: select packs above or paste custom paths.</div>
          </div>

          <div class="card hidden" id="batchCard">
            <div class="cardTitle"><h2>Batch Matrix</h2></div>
            <label class="label">Batch specs (one per line)</label>
            <textarea id="batchSpecs" class="textarea">baseline|meta-llama/Meta-Llama-3.1-8B-Instruct|http://localhost:8000|openAi
candidate|meta-llama/Meta-Llama-3.1-70B-Instruct|http://localhost:8000|openAi</textarea>
            <div class="hint">Format: label|model|baseUrl|provider . Empty fields fall back to target settings above.</div>
            <div class="actions">
              <button class="btn btnPrimary" id="runBatchBtn">Run Batch</button>
            </div>
          </div>

          <div class="card">
            <div class="cardTitle">
              <h2>Activity</h2>
              <div class="toolbar">
                <button class="btn btnSmall" id="clearLogBtn">Clear</button>
              </div>
            </div>
            <div id="log" class="log"></div>
            <div class="footer">Tip: errors and run IDs show up here for quick debugging.</div>
          </div>
        </div>

        <div class="stack">
          <div class="card">
            <div class="cardTitle">
              <h2>Scenario Preview</h2>
              <div class="toolbar">
                <input id="search" class="input search" placeholder="Search id/category/prompt…" />
              </div>
            </div>
            <div class="tableWrap">
              <table>
                <thead>
                  <tr>
                    <th style="width: 160px">id</th>
                    <th style="width: 180px">category</th>
                    <th>prompt</th>
                    <th style="width: 90px">system</th>
                    <th style="width: 90px">canary</th>
                  </tr>
                </thead>
                <tbody id="rows">
                  <tr><td colspan="5" style="color: var(--muted)">Click “Preview Packs”.</td></tr>
                </tbody>
              </table>
            </div>
            <div class="footer">Tip: click a row to inspect the full prompt.</div>
          </div>

          <div class="card">
            <div class="cardTitle">
              <h2>Results</h2>
              <div class="toolbar">
                <a class="btn btnSmall" id="dlSummary" href="#" download>Summary</a>
                <a class="btn btnSmall" id="dlFindings" href="#" download>Findings</a>
                <a class="btn btnSmall" id="dlConfig" href="#" download>Config</a>
              </div>
            </div>
            <div id="summaryBar" class="summaryBar">
              <div class="summaryItem"><div class="k">run</div><div class="v" id="sumRunId">—</div></div>
              <div class="summaryItem"><div class="k">gate</div><div class="v" id="sumGate">—</div></div>
              <div class="summaryItem"><div class="k">risk</div><div class="v" id="sumRisk">—</div></div>
              <div class="summaryItem"><div class="k">leaks</div><div class="v" id="sumLeaks">—</div></div>
              <div class="summaryItem"><div class="k">scenarios</div><div class="v" id="sumScenarios">—</div></div>
              <div class="summaryItem"><div class="k">findings</div><div class="v" id="sumFindings">—</div></div>
            </div>
            <div class="tableWrap">
              <table>
                <thead>
                  <tr>
                    <th style="width: 160px">scenario</th>
                    <th style="width: 180px">category</th>
                    <th style="width: 90px">risk</th>
                    <th style="width: 90px">leak</th>
                    <th style="width: 90px">inj</th>
                    <th>response</th>
                  </tr>
                </thead>
                <tbody id="findingsRows">
                  <tr><td colspan="6" style="color: var(--muted)">Run a job to see findings.</td></tr>
                </tbody>
              </table>
            </div>
            <div class="riskCloudWrap">
              <div class="riskCloudHead">Risk Point Cloud</div>
              <canvas id="riskCloud" width="920" height="230"></canvas>
            </div>
            <div class="footer">Tip: click a finding to inspect system prompt, prompt, and response.</div>
          </div>

          <div class="card">
            <div class="cardTitle">
              <h2>Runs</h2>
              <div class="toolbar">
                <button class="btn btnSmall" id="refreshRunsBtn">Refresh</button>
                <input id="runsSearch" class="input search" placeholder="Search run id…" />
              </div>
            </div>
            <div class="compareBox">
              <label class="label">Selected run IDs (2-5, first is base)</label>
              <textarea id="compareRunIds" class="textarea compareText" placeholder="uuid-1&#10;uuid-2"></textarea>
              <div class="actions">
                <button class="btn btnSmall" id="compareBtn">Compare</button>
                <button class="btn btnSmall" id="clearCompareBtn">Clear</button>
              </div>
              <div id="compareOut" class="hint">No comparison yet.</div>
            </div>
            <div class="tableWrap">
              <table>
                <thead>
                  <tr>
                    <th style="width: 220px">run</th>
                    <th style="width: 190px">time</th>
                    <th style="width: 90px">gate</th>
                    <th style="width: 90px">risk</th>
                    <th style="width: 90px">leaks</th>
                    <th style="width: 90px">scn</th>
                    <th style="width: 90px">find</th>
                  </tr>
                </thead>
                <tbody id="runsRows">
                  <tr><td colspan="7" style="color: var(--muted)">Click “Refresh”.</td></tr>
                </tbody>
              </table>
            </div>
            <div class="footer">Tip: click a run to reload its findings and export artifacts.</div>
          </div>
        </div>
      </div>
    </div>

    <div id="modal" class="modalBackdrop">
      <div class="modal">
        <div class="modalHead">
          <div>
            <div class="modalTitle" id="modalId">—</div>
            <div class="modalMeta" id="modalMeta">—</div>
          </div>
          <div class="modalActions">
            <button class="btn btnSmall" id="copyBtn">Copy</button>
            <button class="close" id="closeBtn" aria-label="Close">×</button>
          </div>
        </div>
        <div class="codeLabel">system</div>
        <div class="code" id="modalSystem">—</div>
        <div class="codeLabel">prompt</div>
        <div class="code" id="modalPrompt"></div>
        <div class="codeLabel">response</div>
        <div class="code" id="modalResponse"></div>
      </div>
    </div>

    <script src="/static/app.js"></script>
  </body>
</html>"##;
