use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse},
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
        .route("/", get(index))
        .nest_service("/static", ServeDir::new(static_dir))
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
        let path = resolve_path(&state.workspace_root, &path);
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

#[derive(Debug, Deserialize)]
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
    summary: sentinelpipe_core::RunSummary,
    findings: Vec<sentinelpipe_core::Finding>,
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

        Ok(())
    }
}

async fn run_job(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RunRequest>,
) -> Result<Json<RunResponse>, (StatusCode, String)> {
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
        .map(|p| resolve_path(&state.workspace_root, p).to_string_lossy().to_string())
        .collect::<Vec<_>>();

    let cfg = sentinelpipe_core::RunConfig {
        run_id: uuid::Uuid::new_v4(),
        target: sentinelpipe_core::TargetConfig {
            provider: req.provider,
            base_url: req.base_url,
            model: req.model,
            api_key: req.api_key.filter(|s| !s.trim().is_empty()),
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
        metadata: Default::default(),
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
    Ok(Json(RunResponse {
        run_id: cfg.run_id.to_string(),
        summary,
        findings,
    }))
}

const INDEX_HTML: &str = r#"<!doctype html>
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
            <input id="apiKey" class="input" type="password" placeholder="sk-..." />

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
              <button class="btn btnPrimary" id="runBtn">Run</button>
              <button class="btn" id="previewBtn">Preview Packs</button>
            </div>
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
            <div class="cardTitle"><h2>Results</h2></div>
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
            <div class="footer">Tip: click a finding to inspect system prompt, prompt, and response.</div>
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
</html>"#;
