# SentinelPipe: Deterministic Security Validation for LLM Release Decisions

## Abstract

Large language model deployments increasingly require operational security validation that is repeatable, explainable, and suitable for release governance. Existing evaluation practices often emphasize exploratory red-teaming or benchmark-style scoring, but these approaches do not always provide deterministic deployment signals that engineering and security organizations can rely on for go/no-go decisions. SentinelPipe is a local-first LLM security validation system designed to execute adversarial scenario packs, score outcomes deterministically, and produce structured artifacts that can be integrated into release gating processes. We describe the system architecture, threat model, artifact design, and deployment controls, and we outline governance usage patterns for security leadership. The core contribution is an operational model that shifts LLM security testing from ad hoc demonstrations to repeatable, explainable go/no-go decision support.

## 1. Introduction

Organizations increasingly deploy LLMs across internal assistants, external copilots, and automation workflows. These deployments introduce risk classes including prompt injection, policy override behavior, sensitive data leakage, and unstable responses under adversarial inputs. In practice, many teams rely on qualitative judgment or model self-attestation during release evaluation, which reduces confidence in security-critical decisions.

SentinelPipe addresses this gap by prioritizing deterministic checks and artifact-backed outputs. The central operational question is straightforward: Can a given model configuration be deployed safely for its intended use case at this time? The system answers this question through reproducible tests, deterministic scoring, and structured run artifacts that support triage, auditing, and release governance.

The system is intentionally narrow in scope. It does not attempt to serve as a comprehensive benchmark suite, nor does it claim to capture every class of model harm. Instead, it targets a recurring operational need inside engineering organizations: the need to make consistent release decisions when a model endpoint, prompt policy, or scenario mix changes. By anchoring that decision process in fixed scenario packs, bounded execution, explicit gate thresholds, and retained artifacts, SentinelPipe provides a validation loop that can be run frequently without requiring each release review to start from first principles.

A second motivation is organizational interoperability. Security reviewers, platform teams, and release owners often need different levels of detail from the same test run. SentinelPipe addresses this by producing both compact gate summaries and scenario-level findings. This enables a single execution to support multiple workflows, from quick release checks to deeper incident-style analysis.

## 2. Related Work

Recent work on LLM red-teaming and safety evaluation has established the need for adversarial testing and systematic evaluation workflows, including automated attack generation, jailbreak analysis, and dangerous capability assessment. This paper builds on that literature but focuses on practical operationalization: deterministic gates, repeatable run comparisons, and audit-oriented artifacts suitable for engineering organizations and security teams.

Much of the existing literature optimizes for capability discovery, safety benchmarking, or broad attack coverage. Those goals are important, but production release decisions often require a different unit of analysis. In deployment settings, the relevant question is less whether a model can ever fail under sufficiently creative attack and more whether a given configuration has crossed an operationally meaningful threshold relative to policy. SentinelPipe therefore emphasizes controlled execution and stable interpretation over open-ended exploration.

This orientation places the system closer to release-control infrastructure than to purely research-driven evaluation harnesses. Scenario packs are treated as inputs to a repeatable pipeline, scores are normalized into a small deterministic set, and artifact generation is built into execution rather than added as an afterthought. In that sense, the project sits at the intersection of LLM red-teaming, secure release engineering, and evidence-oriented governance.

## 3. Problem Statement

Conventional application security testing does not fully capture LLM-specific behavioral risks. Three operational gaps are common:

- Lack of deterministic deployment gates tied to adversarial outcomes.
- Limited reproducibility of findings across model or policy changes.
- Weak traceability from test execution to governance decisions.

The objective of SentinelPipe is to provide a minimal but actionable security validation loop that can be run frequently and interpreted consistently by both engineering and security operators.

In practical terms, the problem is not simply detecting undesirable outputs. The harder issue is establishing a process by which those outputs can be compared across time, tied to a configuration, and translated into a release outcome. Without that structure, even valid security findings can be difficult to operationalize. One team may treat a disclosure-like response as a release blocker, while another may regard it as anecdotal unless reproduced. SentinelPipe reduces this ambiguity by placing findings inside a fixed run model with explicit thresholds and retained evidence.

The project also addresses a common failure mode in internal model deployments: qualitative drift. As prompts, providers, model versions, and wrapper logic change, organizations may feel that behavior has changed without being able to state precisely how. Deterministic scoring and compare-run outputs provide a way to turn that vague sense of drift into measurable deltas that can be reviewed before release.

## 4. System Overview

SentinelPipe consists of:

- A Rust-based red-teaming engine.
- Gazetent, a web console for operator workflows.
- Scenario packs covering distinct adversarial categories.
- Batch and run-comparison capabilities for regression analysis.
- Artifact export for audit and incident preparation.

The current operating model is local-first and test-focused, which reduces deployment complexity while enabling controlled security validation workflows.

At the implementation level, the system is organized as a small workspace of focused crates. `sentinelpipe-core` defines shared data structures such as `RunConfig`, `Scenario`, `Finding`, `Signal`, and `RunSummary`. `sentinelpipe-packs` loads and validates YAML scenario packs. `sentinelpipe-pipeline` defines the staged execution model, including sources, hydrators, filters, executors, scorers, selectors, and side effects. `sentinelpipe-targets` provides provider-specific execution against OpenAI-compatible endpoints and Ollama. `sentinelpipe-scorers` applies deterministic scoring logic. `sentinelpipe-cli` exposes command-line run modes, and `gazetent-web` provides the operator-facing HTTP interface.

This separation is operationally useful because it keeps the release logic legible. Shared structures live in one place, provider execution is isolated from scoring, and artifact persistence is treated as a side effect of the pipeline rather than an implicit behavior hidden inside execution code. The result is a system whose behavior is easier to inspect, test, and explain to non-authors.

## 5. Design Principles

### 5.1 Deterministic Gates First

Only deterministic signals participate in release gating. Core signals include:

- `canaryLeak`
- `injHeuristic`
- `totalRisk`
- `gatePass`

In the current implementation, deterministic gates are derived from a small scoring surface. Canary leakage is treated as a binary condition when a configured marker is disclosed. Injection-style disclosure behavior is detected through a conservative heuristic scorer that searches for disclosure-oriented language patterns in model responses. These signals are then combined into a weighted aggregate risk score. The gate decision is computed from explicit thresholds in `GateConfig`, including maximum allowed canary leaks and maximum allowed total risk.

This design intentionally favors interpretability over semantic breadth. Operators can see why a run failed, which scores contributed, and how those scores relate to gate thresholds. That property is important for release adoption: a simpler scoring model is more likely to be trusted, reviewed, and used consistently than an opaque metric that is difficult to explain under time pressure.

### 5.2 Evidence Over Opinion

Each run produces artifacts that can be independently reviewed and replayed for post-hoc analysis.

This principle is reflected directly in the artifact model. Run outputs are persisted per run ID under a dedicated artifact directory, with scenario-level findings, run-level summary metrics, and a redacted configuration snapshot. Because the configuration is stored alongside results, later reviewers can recover the context of execution without depending on memory or side-channel notes. This is especially useful in organizations where release approvers are not the same individuals who initiated the run.

Evidence-backed operation also improves dispute resolution. If a release gate is challenged, the discussion can focus on specific scenarios, responses, and thresholds rather than on recollections of what the model seemed to do during an interactive demo.

### 5.3 Fast Iteration and Regression Awareness

The workflow supports single runs for quick checks, batch execution for variant testing, and run comparison for deterministic delta analysis.

The codebase supports this principle through both CLI and web flows. Operators can execute a single run, run bounded batches of multiple configurations, or compare several historical run IDs against a base run. Compare-run output includes changes in total risk, canary leaks, finding counts, and whether the gate result changed. This gives teams a compact regression view that is more actionable than manually reading multiple artifact sets.

Fast iteration is also supported by the pipeline’s bounded concurrency model. Scenario execution is parallelized behind a semaphore using the configured concurrency value, enabling faster test completion while preserving predictable resource usage. This matters in practice because release controls that are too slow are often bypassed.

### 5.4 Operator Accessibility

The interface is designed for mildly technical operators, including security analysts and platform owners who may not specialize in ML systems engineering.

Gazetent exposes this workflow over a minimal local web application. Operators can list packs, preview scenario contents, execute runs, run batches, list previous runs, compare runs, and download artifacts from a single interface. The goal is not to abstract away all implementation detail, but to reduce the amount of manual glue required to perform a disciplined evaluation. This makes the system usable by teams who understand deployment risk and governance requirements but do not want to operate a bespoke test harness from source code for every review.

## 6. Threat Model

### 6.1 Assets

Protected assets include model behavior boundaries, policy integrity, synthetic canaries used in testing, run artifacts, and organizational trust in deployment decisions.

In addition to obvious data exposure concerns, the system treats decision integrity as an asset. If operators cannot trust that a run corresponds to a specific configuration and scenario set, then even correctly detected failures lose governance value. For that reason, reproducibility artifacts and redacted configuration snapshots are part of the threat surface considered by the system.

### 6.2 Adversary Behaviors Evaluated

SentinelPipe evaluates:

- Direct prompt injection attempts.
- Multi-turn instruction override attempts.
- Exfiltration behavior against synthetic canaries.
- Adversarial phrasing and mutation patterns.

These behaviors are represented through scenario packs rather than through unconstrained live interaction. Each scenario captures a category, prompt, optional system prompt, and optional canary marker. This allows testing to be both adversarial and structured. In the current implementation, the scoring model is strongest where the expected failure mode can be detected through explicit markers or conservative heuristics, such as canary disclosure and system-prompt-style disclosure language.

### 6.3 Current Out-of-Scope Areas

Current builds do not target full public multi-tenant SaaS hardening, complete IAM product functionality, or advanced distributed queue orchestration.

The implementation is also intentionally limited in provider scope and enterprise control surface. It currently supports OpenAI-compatible targets and Ollama endpoints, but it does not claim comprehensive coverage of deployment patterns such as brokered multi-tenant evaluation, organization-wide policy delegation, or remote worker fleets. Those omissions are design choices rather than oversights: the current system is optimized for local and internally controlled validation flows.

## 7. Architecture and Pipeline

The pipeline is staged as follows:

1. Load run configuration.
2. Load and validate scenario packs.
3. Execute scenarios against target model endpoints.
4. Apply deterministic scorers to model responses.
5. Generate summary and finding records.
6. Persist reproducible artifacts.

Key modules include `sentinelpipe-core`, `sentinelpipe-packs`, `sentinelpipe-targets`, `sentinelpipe-scorers`, `sentinelpipe-pipeline`, and `gazetent-web`.

More precisely, the pipeline begins with query hydrators that can normalize or enrich run configuration, followed by one or more sources that generate scenarios. Hydrators can then transform scenarios before filtering is applied. Execution is performed concurrently but with an explicit semaphore bound, preventing unbounded fan-out. After execution, scorers run sequentially to update findings with deterministic scores and signals. A selector can then reduce or diversify the result set, after which side effects such as artifact persistence run inline on a best-effort basis.

This staged design creates clean extension points. New scenario sources, filters, or scorers can be introduced without rewriting the full execution path. At the same time, the default configuration remains intentionally simple: the current CLI and web flows use no-op query hydration, no-op scenario hydration, no-op filtering, provider-specific executors, deterministic scorers, a top-K selector, and an artifact writer. The architecture therefore supports extensibility without requiring abstraction-heavy configuration to use the system.

## 8. Operator Workflow

A standard workflow is:

1. Configure model target (provider, endpoint, model, optional session key).
2. Select scenario packs.
3. Execute single-run or batch simulation.
4. Triage findings.
5. Compare run IDs for regressions and gate changes.
6. Export artifacts for recordkeeping and review.

The web interface mirrors this workflow through explicit endpoints for pack listing, pack preview, run execution, batch execution, historical run listing, and run comparison. In practice, an operator first chooses pack files from the workspace, verifies what scenarios will be loaded, and submits a run request with provider, base URL, model name, concurrency, timeout, token budget, and gate thresholds. The system resolves and validates pack paths, computes the total scenarios loaded, and records that count in run metadata for later display.

After execution, the operator receives a run ID, summary metrics, scenario-level findings, and direct artifact download links. If a release is in question, the operator can compare the new run against one or more prior runs to determine whether risk increased, leakage counts changed, or a gate transition occurred. This workflow is designed to support both immediate release triage and later governance review with the same underlying records.

## 9. Artifact and Reproducibility Model

Artifacts are written under `gazetent/runs/<run_id>/` and include:

- `summary.json`: run-level metrics and gate result.
- `findings.jsonl`: scenario-level findings.
- `config.redacted.json`: reproducible config with secrets removed.

This model supports reproducibility, post-hoc triage, and governance traceability.

The artifact structure is intentionally minimal. `summary.json` captures the compact decision surface needed for release review. `findings.jsonl` preserves one serialized finding per line, making it easy to inspect, diff, or ingest into downstream tooling. `config.redacted.json` captures the run configuration, including provider, model, pack paths, thresholds, and metadata, while explicitly removing API keys before persistence. Together these files provide a self-contained record of what was tested, how it was scored, and what decision outcome resulted.

Run listing in the web console depends on these same artifacts. Historical runs are discovered from the artifact directory, summaries are loaded from `summary.json`, and scenario totals are recovered from metadata stored in the redacted configuration snapshot. This is a useful property because it means the artifact set is not merely archival; it is also the operational substrate for historical browsing and comparisons.

## 10. Security Controls and Deployment Posture

Implemented controls include localhost default bind (`127.0.0.1:8787`), API-key redaction in persisted artifacts, non-persistence of API keys in browser local storage, request body size limits, workspace-root pack path restriction, and hardened response headers (`Content-Security-Policy`, `X-Content-Type-Options`, `X-Frame-Options`, `Referrer-Policy`, `Permissions-Policy`).

The present posture is local/internal controlled deployment. Public exposure requires additional controls including authentication/authorization, TLS policy enforcement, rate limiting, network controls, and secret-manager integration.

Several of these controls are directly enforced in the current web implementation. The Axum server binds to localhost rather than a public interface by default. Incoming request bodies are bounded to reduce abuse and accidental oversized submissions. Pack paths are canonicalized and checked to ensure that loaded scenario files remain within the workspace root, which reduces the risk of arbitrary file access through path manipulation. Response headers establish a conservative browser posture for the local UI, including a restrictive content security policy and disabled high-risk browser permissions.

The artifact writer also enforces a clear secret-handling rule: API keys are removed from the persisted configuration snapshot before artifacts are written. This supports a practical sharing model in which run artifacts can be retained or transferred for audit and debugging without embedding live credentials. The system should therefore be understood as hardened for local and controlled internal use, not as a finished public SaaS surface.

## 11. Governance and Compliance Utility

SentinelPipe is not a standalone compliance framework. It provides evidence components that can be integrated into governance workflows:

- Deterministic gate outcomes.
- Repeatable run IDs and artifact trails.
- Compare-run regression outputs.
- Security control documentation.

Security leadership can use these outputs to define mandatory pre-release red-team gates, enforce artifact retention, and include deterministic comparisons in change approval processes.

A useful governance property of the system is that it generates both a point-in-time decision and the data needed to defend that decision later. For example, a release board can require that any material model change be accompanied by a recent run ID, artifact retention for a defined period, and a comparison against the last approved baseline. This does not eliminate judgment, but it constrains judgment to a documented frame.

The system is also compatible with layered governance models. Engineering teams can use it for pre-merge or pre-release checks, while security leadership can use the same outputs for sampled review, exception handling, or policy enforcement. Because the gate surface is deterministic and compact, it can be incorporated into formal change-management controls more easily than open-ended qualitative reports.

## 12. Recommended Operational Metrics

Recommended KPIs include gate pass rate by model version, canary leak trend, risk trend by release, time-to-triage for failed runs, and regression counts detected pre-release.

These metrics are useful because they connect technical findings to operational health. Gate pass rate indicates whether a model program is stabilizing or repeatedly regressing. Canary leak trend provides a direct measure of one of the clearest failure classes in the current scoring model. Risk trend by release can reveal whether mitigations are reducing deterministic exposure over time. Time-to-triage captures process efficiency rather than model behavior alone, which is often critical for release teams. Regression counts detected before release help quantify the value of the validation system itself.

Where possible, these metrics should be anchored to retained run IDs and artifact sets rather than to manually curated spreadsheets. Doing so preserves traceability and reduces post-hoc interpretation drift.

## 13. Limitations

Deterministic heuristics reduce ambiguity but cannot capture all semantic harms. Evaluation outcomes depend on scenario-pack quality and adversarial diversity. Operator discipline remains necessary for reliable governance outcomes.

The current scoring model is intentionally small and therefore incomplete. A conservative regex-based heuristic for prompt-injection-style disclosure is explainable and stable, but it will inevitably miss some harmful behaviors and overfit to others. Likewise, weighted aggregate risk is easy to reason about, but any fixed weighting scheme reflects a design choice rather than a universal truth. These limitations are acceptable only if the system is understood as a deployment control layer rather than as a total model safety assessment.

There are also implementation-level constraints. Provider support is currently limited, filters and hydrators are mostly placeholders in the default flows, and side effects run inline on a best-effort basis. These choices keep the system minimal and auditable, but they also indicate where future scaling work would be needed for broader enterprise adoption.

## 14. Roadmap

Near-term priorities include broader scenario coverage, stronger batch templates, and improved comparison visualizations. Medium-term priorities include policy bundle management, queue/cancellation controls, and optional governance integrations. Long-term priorities include multi-tenant controls, enterprise RBAC/approval workflows, and federated distributed execution.

Near-term work should primarily deepen coverage without undermining determinism. That includes expanding pack libraries, improving scenario categorization, and refining operator-facing comparison views so that regressions are easier to interpret. Medium-term work can extend the operational layer, especially around coordinated execution management and reusable policy bundles that encode organization-specific thresholds. Long-term work would move the system from a local-first internal tool toward a more fully managed platform with stronger access control, approval routing, and distributed execution support.

The roadmap should preserve the project’s current strengths. As capabilities expand, the central design requirement should remain the same: release decisions must stay explainable, reproducible, and tied to artifacts that can be reviewed independently.

## 15. Conclusion

SentinelPipe provides a practical pathway from ad hoc LLM testing to disciplined, evidence-based deployment validation. Its primary value is not only failure detection, but also the production of repeatable, explainable, and auditable deployment decisions for engineering and security stakeholders.

By combining structured scenario packs, bounded execution, deterministic scoring, and retained artifacts, the system translates adversarial testing into a form that fits real release processes. This is the key practical contribution. The project does not claim to solve the entire problem of LLM safety evaluation. Rather, it establishes a concrete operational pattern for one important slice of that problem: deciding, with evidence, whether a model configuration is acceptable for release at a given moment.

## References

[1] Perez, E. et al. *Red Teaming Language Models with Language Models*. arXiv:2202.03286.

[2] Ganguli, D. et al. *Red Teaming Language Models to Reduce Harms*. arXiv:2209.07858.

[3] Shevlane, T. et al. *Model Evaluation for Extreme Risks*. arXiv:2305.15324.

[4] METR. *Evaluating Frontier Models for Dangerous Capabilities*. arXiv:2403.13793.

[5] Zou, A. et al. *Universal and Transferable Adversarial Attacks*. arXiv:2307.15043.

[6] Chao, P. et al. *PAIR: Jailbreaking Black Box LLMs in Twenty Queries*. arXiv:2310.08419.

[7] Bai, Y. et al. *Constitutional Classifiers*. arXiv:2501.18837.

[8] AutoRedTeamer. arXiv:2503.15754.
