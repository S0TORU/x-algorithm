# SentinelPipe / Gazetent

Author: Aanu Oshakuade

I built SentinelPipe to make LLM red teaming feel operational instead of hand-wavy.

The point is simple:
- connect a model or agent
- run deterministic attack packs
- inspect the exact failures
- keep the evidence

Gazetent is the local console.
SentinelPipe CLI is the core harness.
`sentinelpipe-mcp` is the agent/tool layer so systems like Codex or Pi can invoke the same engine directly.

## What It Is

SentinelPipe is a local-first red-teaming system for LLMs and agents.

It focuses on:
- deterministic validation layers
- artifact-backed runs
- quick batch iteration
- audit and observability inside the platform

Today the stack is:
- `gazetent-web`: visual workbench
- `sentinelpipe-cli`: terminal harness
- `sentinelpipe-mcp`: MCP wrapper for agents
- `sentinelpipe-redteam` skill: quick-config workflow for Codex-style agents

## Why It Matters

I do not want a system where the model being tested is also the main thing attesting that it is safe.

So the product starts with evidence:
- canary leak detection
- injection heuristics
- weighted risk scoring
- explicit pass/fail gates
- downloadable run artifacts

That gives operators, engineers, and security teams something concrete to inspect.

## Quick Start

From `/Users/aanuoshaks/xai/x-algorithm/sentinelpipe`:

Run the web console:

```bash
cargo run -p gazetent-web
```

Open:

```text
http://127.0.0.1:8787
```

Default flow:
1. Set `Provider`, `Base URL`, and `Model`
2. Preview packs
3. Run single or batch
4. Inspect findings, heatmaps, risk map, and audit panel
5. Export artifacts

## CLI

The CLI is already real and usable.

Help:

```bash
cargo run -p sentinelpipe-cli -- --help
```

Generate a starter config with pack coverage presets:

```bash
cargo run -p sentinelpipe-cli -- init --output sentinelpipe.ollama.yaml --provider ollama --base-url http://localhost:11434 --model llama3.2:1b --preset core --json
```

Preview scenarios without execution:

```bash
cargo run -p sentinelpipe-cli -- dry-run --config examples/run.yaml --json
```

Check target connectivity and model availability:

```bash
cargo run -p sentinelpipe-cli -- doctor --config examples/run-ollama.yaml --json
```

Run a full evaluation:

```bash
cargo run -p sentinelpipe-cli -- run --config examples/run.yaml --json
```

Run a batch across multiple configs:

```bash
cargo run -p sentinelpipe-cli -- batch --config examples/run.yaml --config examples/run-ollama.yaml --json
```

List built-in packs and presets:

```bash
cargo run -p sentinelpipe-cli -- list-packs --json
```

List recent runs:

```bash
cargo run -p sentinelpipe-cli -- list-runs --limit 10 --json
```

Compare historical runs:

```bash
cargo run -p sentinelpipe-cli -- compare --run-id <base> --run-id <candidate> --json
```

CLI modes:
- default output: human-readable
- `--json`: agent-readable and automation-friendly

## MCP

`sentinelpipe-mcp` is the stdio MCP server that wraps the CLI instead of reimplementing scoring logic.

Run it:

```bash
cargo run -p sentinelpipe-mcp
```

Current MCP tools:
- `redteam_list_packs`
- `redteam_preview`
- `redteam_run`
- `redteam_doctor`
- `redteam_batch`
- `redteam_list_runs`
- `redteam_compare`

Design rule:
- one evaluator engine
- multiple control surfaces

## Skill

There is also a project-local Codex skill:

- `/Users/aanuoshaks/xai/x-algorithm/sentinelpipe/.agents/skills/sentinelpipe-redteam/SKILL.md`

The skill is meant to do this cleanly:
- walk through quick config
- explain each field in a few words
- preview first
- run second
- summarize findings and impact

Important fields explained simply:
- `base_url`: endpoint to test
- `model`: model or deployment name
- `packs`: attack coverage
- `timeout_ms`: request timeout
- `top_k`: scenario cap
- `max_canary_leaks`: leak gate
- `max_total_risk`: risk gate

## Web Console

The web console now includes:
- target setup
- pack selection and preview
- single run and batch comparison
- findings table
- findings heatmap
- risk map
- signal breakdown
- audit and observability panel
- run history and comparison

This is the intended operator loop:
- connect
- preview
- execute
- observe
- audit
- export

## Screenshots

Overview:

![Gazetent overview](docs/images/gazetent-overview.png)

Failure detail modal:

![Gazetent finding modal](docs/images/gazetent-finding-modal.png)

## Artifacts

Every run writes evidence under:

```text
gazetent/runs/<run_id>/
```

Artifacts:
- `summary.json`
- `findings.jsonl`
- `config.redacted.json`

These are the files I expect teams to use for debugging, review, and audit trails.

## Benchmark Example

I also want the repo to show a real result, not just the interface.

Current labeled benchmark:
- `/Users/aanuoshaks/xai/x-algorithm/sentinelpipe/benchmarks/2026-03-11_ollama_head_to_head_gemma4b/REPORT.md`
- `/Users/aanuoshaks/xai/x-algorithm/sentinelpipe/benchmarks/2026-03-11_ollama_head_to_head_gemma4b/PUBLIC_SUMMARY.json`

This benchmark compares:
- `llama3.2:1b`
- `jayeshpandit2480/gemma3-UNCENSORED:4b`

Published read:
- `llama3.2:1b` failed with `11` canary leaks and `1100.0` total risk
- `jayeshpandit2480/gemma3-UNCENSORED:4b` failed with `25` canary leaks and `2500.0` total risk
- the comparison was published as sequential single runs because local Ollama model switching was not stable enough for a clean cross-model batch

Notes:
- the public summary is the scrubbed version I would point people to first
- the raw artifacts stay useful for inspection, but I only want synthetic test data in anything we publish

## Security Defaults

- binds to localhost by default
- API keys are not persisted in browser storage
- API keys are stripped from run artifacts
- pack paths are restricted to the workspace in the web app
- response hardening headers are set
- request body size is capped

## Docs

Core docs:
- `/Users/aanuoshaks/xai/x-algorithm/sentinelpipe/USER_GUIDE.md`
- `/Users/aanuoshaks/xai/x-algorithm/sentinelpipe/SECURITY.md`
- `/Users/aanuoshaks/xai/x-algorithm/sentinelpipe/WHITE_PAPER.md`
- `/Users/aanuoshaks/xai/x-algorithm/sentinelpipe/AGENTIC_REDTEAM_ARCHITECTURE.md`

## License

Apache-2.0

-Aanu
