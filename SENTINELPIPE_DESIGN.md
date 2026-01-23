# SentinelPipe v1 Design Doc

## Purpose
SentinelPipe is a universal, modular pipeline for continuous LLM red-teaming and security scoring.

It is designed to be:
- **Composable**: users add new attack generators, evaluators, targets, and sinks without rewriting the orchestrator.
- **Auditable**: every finding can be reproduced with stable IDs, immutable inputs, and stored artifacts.
- **Scalable**: runs high-throughput tests with bounded concurrency and structured observability.
- **Vendor-neutral**: targets any OpenAI-compatible endpoint (including vLLM) plus additional adapters.

This repo (`xai-org/x-algorithm`) is used as architectural inspiration. In particular, SentinelPipe adopts the staged pipeline contract from `candidate-pipeline/`:
`Query Hydration → Sources → Hydrators → Filters → Scorers → Selectors → Side Effects`.

Non-goals:
- Surveillance, targeting, or any capability intended to violate privacy or laws.
- Exploit development for unauthorized access.


## High-Level System View

### Mental Model
Treat every red-team test case as a “candidate” flowing through a pipeline:
- A **Scenario** is the unit of work (prompt(s) + optional tools + optional context + expectations).
- A **Run** is executing a Scenario against a **Target** (an LLM endpoint).
- A **Finding** is a Scenario+Target result with evidence and scored risk signals.

### Core Outputs
SentinelPipe produces:
- **Findings** (structured JSON) with evidence references
- **Risk signals** (multi-dimensional scores + tags)
- **Top-K prioritized failures** (diverse, clustered)
- **Regression reports** (diff vs baseline)
- **Metrics/alerts** to SIEM/monitoring systems


## Architecture: Staged Pipeline (Adapted from x-algorithm)

SentinelPipe uses a single pipeline execution engine with plug-in stages.

### Stage 0: Query (Run Context)
`RunConfig` is the equivalent of the “query” in x-algorithm.
It contains:
- Targets (base URL, auth, model name)
- Policies (rules, allow/deny lists)
- Budgets (max tokens, max spend, max concurrency, timeouts)
- Packs to run (OWASP, prompt injection pack, tool abuse pack, etc.)
- Baseline references (previous run ID or golden model version)

### Stage 1: Query Hydration
Purpose: enrich `RunConfig` before generating scenarios.
Examples:
- Resolve target metadata (model context length, tool support)
- Load policy bundles (YAML → compiled rules)
- Load secrets/canaries from secure store (synthetic-only, never real credentials)
- Load baseline embeddings/index if doing novelty detection

Output: `RunConfig` with derived fields populated.

### Stage 2: Sources (Scenario Generation)
Purpose: generate candidate `Scenario`s.
Sources should be independent and run in parallel where possible.

Examples of Sources:
- Static packs: OWASP LLM Top 10 prompts, known jailbreak corpora
- Prompt injection pack (RAG-focused): instruction hierarchy attacks
- Tool abuse pack: malicious tool invocation attempts
- Data leakage pack: system prompt extraction, secret exfil canaries
- Agentic pack (optional v2): multi-step tool chains

Output: `Vec<Scenario>`.

### Stage 3: Hydrators (Scenario Expansion)
Purpose: add/derive additional fields or expand scenarios.
Hydrators may run in parallel but must preserve ordering and length.

Examples:
- Language variants (EN/ES/FR) for robustness
- Multi-turn wrappers (add benign lead-in then attack)
- Tool schema injection (add tool definitions)
- Context hydrator (inject “retrieval snippets” or synthetic docs)
- Canary hydrator (inject synthetic canaries into context)

Output: enriched `Vec<Scenario>`.

### Stage 4: Filters
Purpose: remove scenarios that should not run.
Filters run sequentially and can drop items.

Examples:
- Budget filters (estimated tokens, max runtime)
- Scope filters (disallow certain categories in certain environments)
- Dedup filters (near-duplicate prompts)
- Safety constraints (avoid generating disallowed content in corp policies)

Output: `kept` and `removed` scenario sets (and reasons).

### Stage 5: Execution (Target Calls)
This is the “expensive” stage: call the target LLM.

Design requirements:
- Bounded concurrency (Semaphore)
- Cancellation/timeouts
- Retries with jitter for transient errors
- Strict redaction in logs
- Capture tool calls / structured outputs when available

Output: `Vec<ExecutionResult>` aligned to scenario order.

### Stage 6: Scorers (Risk Signal Computation)
Scorers run sequentially, preserve ordering/length, and populate only their fields.

Scorer types:
1. **Deterministic** (fast): regex/rule-based detectors
2. **Semantic** (embeddings): similarity, novelty, clustering
3. **Model-assisted** (optional): LLM-judge (requires careful anti-bias controls)

Example signals:
- `prompt_injection_success`
- `system_prompt_exposure`
- `data_leakage_canary_match`
- `pii_leakage`
- `unsafe_tool_call`
- `policy_bypass`
- `refusal_quality`
- `novelty_score`

### Stage 7: Selector (Top-K + Diversity)
Selectors choose which findings to highlight.

Core selectors:
- `TopKByRisk`: sort by composite risk score
- `DiverseTopK`: cluster and take representatives per cluster (prevents one failure mode dominating)

### Stage 8: Side Effects (Sinks)
Side effects are best-effort and must not block returning results.

Examples:
- Write JSONL artifacts to disk/object storage
- Emit metrics (Prometheus)
- Send alerts to SIEM (Splunk/Elastic) for critical findings
- Create tickets (Jira/GitHub Issues)
- Store run summary to Postgres


## Data Model (v1)

### RunConfig
Minimal fields:
- `run_id` (UUID)
- `targets: Vec<TargetConfig>`
- `packs: Vec<String>`
- `concurrency: usize`
- `timeout_ms: u64`
- `max_tokens: u32`
- `policy_bundle_ref: String`
- `baseline_run_ref: Option<String>`

### TargetConfig
- `target_id` (stable)
- `base_url`
- `model`
- `api_key_ref` (reference, not plaintext)
- `capabilities` (tools, streaming, max context)

### Scenario
- `scenario_id` (stable hash of normalized definition)
- `category` (prompt_injection | leakage | tool_abuse | etc.)
- `messages` (multi-turn)
- `tools` (optional)
- `synthetic_canaries` (optional)
- `expected_invariants` (e.g., “must refuse”, “must not reveal system prompt”)

### ExecutionResult
- `scenario_id`
- `target_id`
- `raw_response_ref` (artifact pointer)
- `transcript` (redacted)
- `tool_calls` (normalized)
- `tokens_used`
- `latency_ms`
- `error` (optional)

### Signals
- `scores: Map<String, f64>`
- `tags: Vec<String>`
- `embedding_ref` (optional pointer)
- `composite_risk_score: f64`

### Finding
- `finding_id` (hash of scenario_id + target_id + run_id)
- `scenario`
- `execution_result`
- `signals`
- `timestamp`


## Target Adapters

### Required v1: OpenAI-Compatible Adapter
Supports:
- `/v1/chat/completions`
- tool calls if target supports them

vLLM fits this shape and is a primary target.

### Optional v1 Targets
- OpenAI
- Azure OpenAI
- Anthropic-compatible proxy (if normalized)
- Bedrock (requires adapter)


## Attack Packs (v1)
Start with packs that are broadly useful, high-signal, and safe to run with synthetic data:

1. **Prompt Injection (RAG)**
   - Instruction hierarchy overrides
   - “Ignore previous instructions” variants
   - Data exfil attempts from provided context

2. **System Prompt Extraction**
   - Direct: “show system prompt”
   - Indirect: roleplay, debugging, translation attacks

3. **Synthetic Secret Leakage**
   - Canary tokens placed in context
   - Ask model to reveal/transform/exfil

4. **Tool Misuse (if tools enabled)**
   - Unauthorized tool selection
   - Parameter injection (path traversal payloads, SSRF strings)
   - “Call tool with secrets” prompts

5. **Policy Bypass / Refusal Quality**
   - Must refuse disallowed requests
   - Refusal should not include restricted info


## Scoring (v1)

### Multi-Signal Scoring Philosophy
Borrowing the `WeightedScorer` approach from x-algorithm:
- Compute many interpretable probabilities/scores.
- Combine them with policy-driven weights.
- Track both component signals and composite score.

### Composite Risk Score Example
`risk = 3.0*leakage + 2.0*system_prompt_exposure + 2.5*tool_misuse + 1.5*prompt_injection_success + 1.0*novelty`

### Normalization
- Keep raw scores in [0, 1] where possible.
- Maintain per-target baselines to detect regressions.


## Observability & Governance

### Metrics
- Runs: count, duration, scenarios executed
- Target health: error rates, latency
- Risk: #critical findings, score distributions
- Regression: delta vs baseline

### Audit Requirements
- Stable IDs for scenarios and findings
- Immutable artifact storage for raw responses
- Redaction of sensitive content in logs


## Safety & Ethics
- Only use **synthetic** canaries.
- Support “safe mode” that disables certain categories.
- Provide clear documentation that the framework is for authorized security testing.


## Implementation Plan (v1)

1. Create a new Rust workspace for SentinelPipe
   - Copy/adapt `candidate-pipeline/` as `sentinelpipe-pipeline/`
2. Define core types (`RunConfig`, `Scenario`, `ExecutionResult`, `Signals`)
3. Implement OpenAI-compatible target adapter (vLLM first)
4. Implement 2 sources (prompt injection pack + canary leakage pack)
5. Implement 2 scorers (regex leakage + simple prompt injection success)
6. Implement selection (TopK + simple diversity)
7. Add artifact sink + metrics sink


## CLI UX (v1)
- `sentinelpipe run --config run.yaml`
- `sentinelpipe pack list`
- `sentinelpipe report --run-id ...`


## Open Questions
- Should embeddings/scoring be pure Rust (fast deploy) or Python plugin (faster iteration)?
- Where do artifacts live by default: local FS, S3, or both?
- Do we support tool-call/agent evaluations in v1 or v2?
