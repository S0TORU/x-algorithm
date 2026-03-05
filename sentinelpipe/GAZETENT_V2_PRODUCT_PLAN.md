# Gazetent v2 Product Plan

Author: Aanu Oshakuade

## Objective
Build a production-grade LLM red-teaming console that gives deterministic ship/no-ship evidence, supports rapid batch iteration, and is usable by mildly technical operators.

Core promise:
- Run realistic attack simulations against one or many model endpoints.
- Produce deterministic risk gates and reproducible artifacts.
- Make triage and comparisons fast.

## User Personas
- Security engineer: wants deterministic gates, evidence exports, and reproducibility.
- ML engineer: wants rapid batch testing across model/config variants.
- Platform owner: wants clear pass/fail summary and audit-ready run packages.

## Product Requirements
- Time-to-first-run under 3 minutes.
- End-to-end workflow in one page: Connect -> Select tests -> Simulate -> Triage -> Export.
- Batch mode for model/config matrices.
- Deterministic validation must be first-class; model-judge is optional annotation only.
- Reproducible artifacts per run, including redacted config and summary.

## Validation Contract
Primary deterministic signals:
- `canaryLeak`: exact/regex canary detection.
- `injHeuristic`: injection/disclosure heuristics.
- `totalRisk`: weighted numeric score.
- `gatePass`: strict thresholds (`max_canary_leaks`, `max_total_risk`).

Secondary non-gating signals:
- Optional semantic/LLM-assist notes for analyst context.

Rule: only deterministic signals can fail a gate in v2.

## UX Direction (Liquid Glass Web)
Visual goals:
- Light-neutral base with soft translucent layers.
- Dense but calm cards; clear type hierarchy.
- Minimal controls visible by default; advanced fields collapsed.
- Fast readouts at top: health, active mode, current gate status.

Layout model:
- Left control rail: target, attack suites, batch settings, run actions.
- Center workspace: scenario preview and results explorer.
- Optional right visual pane: risk point cloud (interactive fail clusters).

Interaction model:
- Every major action logs to an Activity stream.
- Every run has one-click exports.
- Click any run to restore full context and findings instantly.

## Simulation Engine (v2)
- Single-run endpoint remains available for simple workflows.
- Add batch-run endpoint that executes a list of run specs sequentially (safe) with per-spec summaries.
- Add run-compare endpoint for 2-5 selected runs with metric deltas.
- Add attacker profile presets:
  - `baseline_pack`
  - `pliny_style_prompting`
  - `pair_iterative_mutation` (simulated mutation loop)
  - `leakage_focus`

## Artifact and Audit Package
Per run directory:
- `summary.json`
- `findings.jsonl`
- `config.redacted.json`

Batch directory:
- `batch_summary.json`
- links to child runs

Export surfaces:
- Download summary/findings/config from UI.
- Compare export as JSON for external dashboards.

## Incremental Delivery Slices
Slice 1: Product doc and alignment
- Add v2 plan and commit.

Slice 2: Backend workflow primitives
- Add `/api/run/batch`.
- Add `/api/runs/compare`.
- Add batch and compare response models.

Slice 3: Frontend operator workflow
- Add explicit workflow mode switch (single vs batch).
- Add batch spec editor and execution.
- Add compare UI and delta summary.

Slice 4: Advanced interaction layer
- Add optional risk point-cloud panel for findings.
- Add richer filtering and keyboard-driven triage.

## Metrics for v2 Success
- Operator can run first single test in under 3 minutes.
- Operator can run 5-model batch in under 10 minutes.
- 100% of run exports contain deterministic evidence fields.
- Compare view makes regressions obvious in under 30 seconds.

## Open Risks
- Batch execution can be slow without queueing/cancellation controls.
- Semantic or LLM-judge features can confuse users if presented as gating.
- Complex attack profiles may increase false positives if not clearly scoped.

## Non-Goals for This Iteration
- Multi-tenant auth and user management.
- Distributed workers or remote queue orchestration.
- Full policy DSL editor in UI.

-Aanu
