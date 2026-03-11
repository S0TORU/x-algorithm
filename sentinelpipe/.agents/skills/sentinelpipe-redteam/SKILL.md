---
name: sentinelpipe-redteam
description: Use when the task is to red-team an LLM or agent with SentinelPipe from the terminal. This skill walks through quick config, explains the impact of each field in a few words, previews attack packs first, then executes the run and summarizes findings.
---

# SentinelPipe Red Team

Use this skill when the user wants to test a model, endpoint, or agent with SentinelPipe.

## What This Skill Does

1. Confirm the target
2. Explain the key config fields briefly
3. Preview packs before execution
4. Run SentinelPipe
5. Summarize gate, findings, and next actions

## Quick Config Script

Keep explanations short:

- `base_url`: endpoint to test
- `model`: model or deployment name
- `packs`: attack coverage
- `timeout_ms`: request timeout
- `top_k`: scenario cap
- `max_canary_leaks`: leak gate
- `max_total_risk`: risk gate

Preferred order:

1. Ask or infer provider:
   - `Ollama`
   - `OpenAI-compatible`
2. Confirm `base_url`
3. Confirm `model`
4. Pick packs:
   - `core`
   - `leakage`
   - `adversarial`
   - `all`
5. Preview first
6. Run

## Commands

From `/Users/aanuoshaks/xai/x-algorithm/sentinelpipe`:

Preview:

```bash
cargo run -p sentinelpipe-cli -- dry-run --config examples/run.yaml --json
```

Run:

```bash
cargo run -p sentinelpipe-cli -- run --config examples/run.yaml --json
```

Doctor:

```bash
cargo run -p sentinelpipe-cli -- doctor --config examples/run.yaml --json
```

Batch:

```bash
cargo run -p sentinelpipe-cli -- batch --config examples/run.yaml --config examples/run-ollama.yaml --json
```

Web UI:

```bash
cargo run -p gazetent-web
```

## Operating Rules

- Prefer preview before run
- Prefer `doctor` before first run against a new endpoint
- Prefer `--json` when another agent will inspect the result
- If the user is unsure about pack choice, start with `core`
- If the target is newly wired, keep gates strict and packs small first
- When the gate fails, report the exact scenario ids and categories first

## References

Read this file if you need the broader architecture:

- `/Users/aanuoshaks/xai/x-algorithm/sentinelpipe/AGENTIC_REDTEAM_ARCHITECTURE.md`
