# SentinelPipe / Gazetent White Paper

Author: Aanu Oshakuade
Date: March 5, 2026
Version: 1.0

## Executive Summary
SentinelPipe (with the Gazetent console) is a local-first LLM red-teaming system designed to answer one practical question:

Can this model configuration be deployed safely for the intended use case right now?

The system is built around deterministic validation, reproducible evidence, and simple operator workflows.

In plain terms, this means:
- You can run realistic attack scenarios against your model.
- You receive objective pass/fail gates using deterministic signals.
- You can export artifacts that support security review, compliance conversations, and incident preparation.

SentinelPipe is designed to reduce uncertainty for CISOs, security leads, and ML platform owners who need a repeatable process, not one-off demos.

## Why This Matters
Most organizations now use LLMs in one of three patterns:
- Internal assistant usage
- External customer-facing copilots/agents
- Automation workflows with tools and retrieval

All three patterns introduce risk types that traditional application testing does not fully cover:
- Prompt injection
- System prompt leakage
- Data exfiltration behavior
- Unsafe instruction following
- High-variance responses under adversarial input

A common operational mistake is relying on model self-attestation (for example, trusting model-generated confidence statements as if they were controls). SentinelPipe addresses this by prioritizing deterministic checks and evidence-backed outputs.

## Product Scope
SentinelPipe includes:
- A Rust-based red-team pipeline engine
- A web console (Gazetent) for operators
- Scenario packs for different risk categories
- Run history and comparison workflows
- Artifact generation for audits and triage

Current usage model is local-first and test-focused.

## Design Principles
### 1) Deterministic gates first
Only deterministic signals can fail deployment gates.

Core gate signals:
- `canaryLeak`
- `injHeuristic`
- `totalRisk`
- `gatePass`

### 2) Evidence over opinion
Every run produces artifacts that can be reviewed independently.

### 3) Fast iteration
Single runs for quick checks, batch runs for matrix testing, and compare mode for regression detection.

### 4) Usability for mildly technical operators
A security analyst or platform owner can run workflows without deep ML engineering knowledge.

## Threat Model (Practical)
### Assets
- Model behavior boundaries
- Prompt/system policy integrity
- Synthetic secrets/canaries used in testing
- Run artifacts and summaries
- Operational trust in go/no-go decisions

### Adversary behaviors tested
- Direct prompt injection attempts
- Multi-turn override attempts
- Exfiltration behavior against synthetic canaries
- Adversarial phrasing/mutation patterns

### Out of scope (current release)
- Full public multi-tenant SaaS hardening
- Identity and access management as a complete product suite
- Advanced distributed execution and queue orchestration

## System Architecture Overview
### Pipeline model
SentinelPipe follows staged processing:
1. Load run configuration
2. Load and validate scenario packs
3. Execute scenarios against target model
4. Score responses with deterministic scorers
5. Build summary and findings
6. Write reproducible artifacts

### Key modules
- `sentinelpipe-core`: run config, scenario, findings, summary models
- `sentinelpipe-packs`: scenario pack loading/validation
- `sentinelpipe-targets`: model target executors
- `sentinelpipe-scorers`: deterministic risk scoring
- `sentinelpipe-pipeline`: orchestration
- `gazetent-web`: operator UI and API layer

## Operator Workflow (Simple)
### Step 1: Connect target
Set provider, base URL, model, and optional session API key.

### Step 2: Select test scenarios
Choose pack files from available list or custom paths.

### Step 3: Simulate
Run either:
- Single run (quick verification)
- Batch matrix (multiple model/config variants)

### Step 4: Triage findings
Inspect risk summary and detailed finding rows.

### Step 5: Compare and decide
Compare run IDs to detect regressions and gate flips.

### Step 6: Export evidence
Download:
- `summary.json`
- `findings.jsonl`
- `config.redacted.json`

## Batch and Comparison Capability
Batch mode executes multiple run specs and returns per-run outcomes.

Compare mode accepts 2-5 run IDs and reports deterministic deltas:
- Delta total risk
- Delta canary leaks
- Delta findings count
- Gate change state

This supports practical release management patterns:
- Baseline model vs candidate model
- Prompt policy v1 vs v2
- Target endpoint A vs B

## Artifact Model
Artifacts are written per run under:
`gazetent/runs/<run_id>/`

Files:
- `summary.json`: run-level metrics and gate outcome
- `findings.jsonl`: finding records for each flagged scenario
- `config.redacted.json`: reproducible config with secrets removed

These artifacts allow reproducibility and post-hoc review.

## Security Controls in Current Build
### Implemented controls
- Localhost bind default (`127.0.0.1:8787`)
- API key redaction from persisted run artifacts
- API key not persisted in browser local storage
- Body size cap on API requests (1 MB)
- Pack path restriction to workspace root
- Response hardening headers:
  - Content Security Policy
  - X-Content-Type-Options
  - X-Frame-Options
  - Referrer-Policy
  - Permissions-Policy

### Security posture statement
Current build is designed for local/internal controlled environments. Public exposure requires additional controls (authn/authz, TLS policy, rate limiting, network controls, and secret-manager integration).

## Compliance and Audit Readiness
SentinelPipe is not a compliance framework by itself. It provides evidence components that can support compliance workflows.

### Evidence this system provides
- Deterministic pass/fail gate outcomes
- Repeatable run IDs and artifact trails
- Comparative regression outputs
- Security policy documentation and hardening notes

### How a CISO can use this in governance
- Define mandatory pre-release red-team gates
- Require artifact retention for model releases
- Include compare-run output in change approvals
- Track drift and gate failure trends over time

## KPIs for Security Leadership
Recommended metrics:
- Gate pass rate by model version
- Canary leak count trend
- Total risk trend by release
- Time-to-triage for failed runs
- Regression count detected pre-release

## Deployment Guidance
### Local controlled setup (recommended current mode)
- Run Gazetent locally
- Connect to local/private model endpoint
- Store artifacts in controlled internal workspace

### Hardened internal deployment (next stage)
- Put API behind authenticated proxy
- Enforce TLS and access control
- Add run-level identity and audit logs
- Restrict allowed outbound target hosts

## Implementation Roadmap (High Level)
### Near term
- Improved scenario pack coverage
- Better batch templates
- Stronger compare visualizations

### Medium term
- Policy bundle management
- Queueing/cancellation controls for long batch jobs
- Optional governance integrations (ticketing/report pipelines)

### Long term
- Multi-tenant controls
- Enterprise RBAC and approvals
- Federated distributed execution

## Limitations
- Deterministic heuristics reduce ambiguity but do not capture all possible semantic harms.
- Results depend on test pack quality and attack diversity.
- Operator discipline is still required for reliable governance.

## Recommended Governance Policy (Simple)
For each release candidate:
1. Run baseline pack set.
2. Run at least one adversarial mutation batch.
3. Enforce deterministic gate thresholds.
4. Compare against last approved baseline.
5. Archive artifacts and approval decision.

This creates a practical minimum standard for model deployment readiness.

## References
- Red Teaming Language Models with Language Models (arXiv:2202.03286)
- Red Teaming Language Models to Reduce Harms (arXiv:2209.07858)
- Model Evaluation for Extreme Risks (arXiv:2305.15324)
- Evaluating Frontier Models for Dangerous Capabilities (arXiv:2403.13793)
- Universal and Transferable Adversarial Attacks (arXiv:2307.15043)
- PAIR: Jailbreaking Black Box LLMs in Twenty Queries (arXiv:2310.08419)
- Constitutional Classifiers (arXiv:2501.18837)
- AutoRedTeamer (arXiv:2503.15754)

## Conclusion
SentinelPipe provides a concrete path from ad hoc LLM testing to disciplined, evidence-based security validation.

For CISOs and security officers, its primary value is not just finding failures. Its value is producing repeatable, explainable deployment decisions.

-Aanu
