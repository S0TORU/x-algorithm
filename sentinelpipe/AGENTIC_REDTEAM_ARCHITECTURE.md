# Agentic Red Team Architecture

Author: Aanu Oshakuade

## Goal

Turn SentinelPipe into an agent-callable red-teaming system with one engine and multiple control surfaces:

- `sentinelpipe-cli`: core harness and evaluator
- `sentinelpipe-mcp`: MCP tool server for Codex, Pi, and other agents
- `pi-redteam`: Pi package/command layer on top of the same engine
- `sentinelpipe-redteam` skill: thin operator workflow for Codex-style agents

## Current State

Today the repo already has a working CLI and web UI.

- CLI crate: `/Users/aanuoshaks/xai/x-algorithm/sentinelpipe/sentinelpipe-cli`
- Web UI: `/Users/aanuoshaks/xai/x-algorithm/sentinelpipe/gazetent-web`

The CLI now supports machine-readable output:

```bash
cargo run -p sentinelpipe-cli -- run --config examples/run.yaml --json
cargo run -p sentinelpipe-cli -- dry-run --config examples/run.yaml --json --show-prompts
cargo run -p sentinelpipe-cli -- init --output sentinelpipe.ollama.yaml --provider ollama --base-url http://localhost:11434 --model llama3.2:1b --preset core --json
cargo run -p sentinelpipe-cli -- list-runs --limit 10 --json
cargo run -p sentinelpipe-cli -- compare --run-id <base> --run-id <candidate> --json
```

That is the first requirement for safe agent invocation.

## Target Components

### 1. `sentinelpipe-cli`

Purpose:
- deterministic evaluator
- local operator tool
- CI entrypoint
- lowest-level interface for agents

Required commands:
- `init`
- `run`
- `dry-run`
- `batch`
- `doctor`
- `list-packs`
- `list-runs`
- `compare`

Required output modes:
- human-readable default
- `--json` for agents and automation

### 2. `sentinelpipe-mcp`

Purpose:
- expose SentinelPipe as tools that coding agents can call directly

Implemented tools:
- `redteam_list_packs`
- `redteam_preview`
- `redteam_run`
- `redteam_doctor`
- `redteam_batch`
- `redteam_list_runs`
- `redteam_compare`

Design rule:
- MCP should be a thin wrapper over the CLI/core engine, not a second implementation.

### 3. `pi-redteam`

Purpose:
- Pi-native package that makes SentinelPipe feel first-class inside Pi

Target commands:
- `/redteam-agent`
- `/redteam-prompt`
- `/attack-pack`
- `/regression-check`

Design rule:
- Pi command layer should collect intent and config, then call the shared evaluator path.

### 4. `sentinelpipe-redteam` skill

Purpose:
- teach Codex-style agents how to configure and run the tool consistently

Behavior:
- ask for or infer target
- walk through quick config
- explain each config field in a few words
- preview packs first
- execute run
- summarize evidence and next actions

## Quick Config UX

When an agent runs the skill, the flow should be:

1. Confirm target type
   - `OpenAI-compatible`: generic HTTP chat endpoint
   - `Ollama`: local model server
2. Confirm `base_url`
   - impact: where requests go
3. Confirm `model`
   - impact: what gets tested
4. Confirm packs or preset
   - impact: attack coverage
5. Confirm gate thresholds
   - `max_canary_leaks`: allowed secret leaks
   - `max_total_risk`: allowed aggregate risk
6. Preview scenarios
   - impact: verify what will actually run
7. Execute
8. Report

Config language should stay terse:

- `base_url`: endpoint to test
- `model`: model name or deployment id
- `packs`: attack families to run
- `top_k`: cap on scenarios selected
- `timeout_ms`: request timeout
- `max_canary_leaks`: leak gate
- `max_total_risk`: risk gate

## Agent Invocation Modes

### Terminal-first

For any coding agent that can run shell commands:

```bash
sentinelpipe-cli dry-run --config examples/run.yaml --json
sentinelpipe-cli run --config examples/run.yaml --json
```

### MCP-first

For agents that support tool calling:

- agent calls `redteam_preview`
- agent shows the user selected packs and scenario count
- agent calls `redteam_run`
- agent summarizes gate, findings, and exported artifacts

## Recommended Build Order

1. Keep strengthening `sentinelpipe-cli`
2. Add `doctor`, `batch`, and richer `preview`
3. Add `sentinelpipe-mcp`
4. Add Codex skill
5. Add Pi package/commands

## Immediate Next Slice

The next engineering slice should implement:

- `pi-redteam` command execution, not just the command contract
- a first `redteam_init_config` MCP helper if we want agents to create configs without shelling out to `init`
- regression workflows that compare base prompt, defended prompt, and candidate model in one request

-Aanu
