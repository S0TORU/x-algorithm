# Gazetent Context Dump

This file captures the **current state** of Gazetent (v1 prototype) in this repo, including design intent, scaffold, code layout, runtime flow, tests, and how to run everything without installing vLLM.

---

## 1) Purpose and Safety
Gazetent is a universal, modular pipeline for **continuous LLM red‑teaming and security scoring**. It is **defensive only** (no surveillance or exploit development). The design is inspired by the staged pipeline in x‑algorithm’s `candidate-pipeline` (query hydration → sources → hydrators → filters → scorers → selector → side effects).

Reference design: `SENTINELPIPE_DESIGN.md`.

---

## 2) What We Implemented
We added a self‑contained Rust workspace at `sentinelpipe/` with these crates (brand name: Gazetent):

- `sentinelpipe-core`: core data types, config schema, gating logic, errors.
- `sentinelpipe-pipeline`: staged orchestration engine + traits.
- `sentinelpipe-targets`: OpenAI‑compatible executor (vLLM/OpenAI style).
- `sentinelpipe-packs`: YAML pack loader + validation.
- `sentinelpipe-scorers`: v1 scorers (canary leak, injection heuristic, weighted risk).
- `sentinelpipe-cli`: CLI runner + artifact writer.

Example packs + config live in `sentinelpipe/examples/`.

---

## 3) Workspace Layout
```
sentinelpipe/
  Cargo.toml
  sentinelpipe-core/
  sentinelpipe-pipeline/
  sentinelpipe-targets/
  sentinelpipe-packs/
  sentinelpipe-scorers/
  sentinelpipe-cli/
  examples/
    run.yaml
    packs/
      basic_injection.yaml
      canary_leak.yaml
```

Workspace manifest: `sentinelpipe/Cargo.toml`.

---

## 4) Core Data Model (sentinelpipe-core)
**File:** `sentinelpipe/sentinelpipe-core/src/lib.rs`

### Key types
- `RunConfig`
  - `run_id`, `target`, `packs`, `concurrency`, `timeout_ms`, `max_tokens`, `gate`, `artifacts_dir`, `metadata`
- `TargetConfig`
  - `base_url`, `model`, optional `api_key`
- `Scenario`
  - `id`, `category`, `prompt`, optional `systemPrompt`, optional `canary`
- `Finding`
  - `scenario_id`, `category`, `prompt`, optional `system_prompt`, `response_text`, `signals`, `scores`, `total_risk`
- `Signal`
  - `CanaryLeak`, `InjectionHeuristicMatch`
- `RunSummary`
  - aggregated totals + gate result

### Gate logic
`RunSummary::from_findings`:
- Gate fails if `canary_leaks > max_canary_leaks` or `total_risk > max_total_risk`.

---

## 5) Pipeline Engine (sentinelpipe-pipeline)
**File:** `sentinelpipe/sentinelpipe-pipeline/src/lib.rs`

### Trait stages
- `QueryHydrator`
- `Source`
- `Hydrator` (order‑preserving)
- `Filter` (drops scenarios)
- `Executor`
- `Scorer` (order‑preserving)
- `Selector`
- `SideEffect`

### Pipeline flow (v1)
1. Hydrate config
2. Run sources in parallel
3. Run hydrators sequentially (order preserved)
4. Run filters sequentially
5. Execute scenarios with bounded concurrency
6. Run scorers sequentially
7. Select final findings
8. Run side effects (inline, best‑effort)

---

## 6) Target Executor (sentinelpipe-targets)
**File:** `sentinelpipe/sentinelpipe-targets/src/lib.rs`

### `OpenAiCompatibleExecutor`
- Calls `POST /v1/chat/completions`
- Uses `RunConfig.timeout_ms` and `RunConfig.max_tokens`
- Parses `choices[0].message.content`
- Adds `CanaryLeak` signal if a canary is echoed
- Includes an optional `system` message if `Scenario.system_prompt` is set

This adapter works with vLLM or any OpenAI‑compatible server.

---

## 7) Pack Loader (sentinelpipe-packs)
**File:** `sentinelpipe/sentinelpipe-packs/src/lib.rs`

Loads YAML pack files into `Vec<Scenario>`, validates:
- `id` must be non‑empty
- `prompt` must be non‑empty
- `systemPrompt` (if provided) must be non‑empty

---

## 8) Scorers (sentinelpipe-scorers)
**File:** `sentinelpipe/sentinelpipe-scorers/src/lib.rs`

- `CanaryLeakScorer`
  - Looks for `CanaryLeak` signals and sets `canaryLeak` score.
- `PromptInjectionHeuristicScorer`
  - Regex match on typical injection phrases; sets `injHeuristic` score and adds signal.
- `WeightedRiskScorer`
  - Composite: `total_risk = 100 * canaryLeak + 10 * injHeuristic`.

---

## 9) CLI Runner + Artifacts (sentinelpipe-cli)
**File:** `sentinelpipe/sentinelpipe-cli/src/main.rs`

Commands:
```
gazetent run --config examples/run.yaml
gazetent dry-run --config examples/run.yaml
```

Pipeline assembly:
- Noop query hydrator
- Pack source (loads `RunConfig.packs`)
- Noop hydrator + Noop filter
- OpenAI‑compatible executor
- Scorers: Canary leak → Injection heuristic → Weighted risk
- **Selector: DiverseTopK** (risk‑sorted by category, then truncated by `topK`)
- Artifact writer side effect

Artifacts written:
- `runs/<run_id>/findings.jsonl`
- `runs/<run_id>/summary.json`

If gate fails, CLI returns non‑zero.

---

## 10) Example Packs + Config
**Config:** `sentinelpipe/examples/run.yaml`
- `baseUrl: http://localhost:8000`
- `packs`: prompt injection + canary leak
- Gate: `maxCanaryLeaks: 0`, `maxTotalRisk: 50`

**Pack:** `sentinelpipe/examples/packs/basic_injection.yaml`
- 2 prompt injection scenarios

**Pack:** `sentinelpipe/examples/packs/canary_leak.yaml`
- 2 canary leak scenarios

---

## 11) Tests (No vLLM Required)
All tests pass locally using mocks. Run all tests from `sentinelpipe/`:

```
cargo test
```

### Test coverage by crate
- `sentinelpipe-cli` integration test (mock server + CLI run)
**sentinelpipe-core**
- `run_summary_gate_passes_with_no_findings`
- `run_summary_fails_on_canary_leak`

**sentinelpipe-packs**
- `load_pack_file_parses_scenarios`
- `load_pack_file_rejects_empty_id`
- `load_pack_file_rejects_empty_prompt`

**sentinelpipe-pipeline**
- `pipeline_runs_filters_and_executes_in_order`

**sentinelpipe-scorers**
- `canary_leak_scorer_sets_score`
- `injection_heuristic_sets_signal`
- `weighted_risk_combines_scores`

**sentinelpipe-targets** (HTTP mock, no real LLM)
- `executor_parses_response`
- `executor_adds_canary_signal`

**sentinelpipe-cli** (integration)
- `cli_runs_against_mock_server`

---

## 12) Build & Run Commands
From repo root:

**Build the workspace**
```
cd sentinelpipe
cargo build
```

**Run tests**
```
cargo test
```

**Dry run (no target calls)**
```
cargo run -p sentinelpipe-cli -- dry-run --config examples/run.yaml
```

**Run CLI (requires OpenAI‑compatible endpoint)**
```
cargo run -p sentinelpipe-cli -- run --config examples/run.yaml
```

If no endpoint is running, executor will error (expected). Tests do not require any LLM.

---

## 13) Current Limitations (v1)
- Single‑turn prompt only (no multi‑turn messages yet).
- No tool/function calling in v1.
- Selector is heuristic (category‑diversified Top‑K).
- No embedding‑based or LLM‑judge scorers.
- No structured artifact schema beyond JSONL + summary.

---

## 14) Strategic Next Steps (optional)
- Add `TopK` / `DiverseTopK` selector
- Add multi‑turn scenarios + tool‑use tests
- Add embedding‑based dedup / novelty scoring
- Add `--dry-run` mode for pack validation
- Add CI gate output + machine‑readable summaries
