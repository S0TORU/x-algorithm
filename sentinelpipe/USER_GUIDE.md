# Gazetent User Guide (Simple)

## 1) Connect your model
- Open Gazetent at `http://127.0.0.1:8787`.
- Set:
  - `Provider` (`OpenAI-compatible` or `Ollama`)
  - `Base URL` (for example `http://localhost:8000`)
  - `Model` (for example `meta-llama/Meta-Llama-3.1-8B-Instruct`)
- Optional: paste API key for this session only.

## 2) Choose scenarios
- In `Packs`, check pack files.
- Or paste pack paths in `Pack paths` (one line each).
- Click `Preview Packs` to confirm loaded scenarios.

## 3) Run single test
- Keep mode on `Single`.
- Click `Run Single`.
- Read top summary:
  - `gate` pass/fail
  - total risk, leaks, findings
- Click any finding row to inspect full system prompt, prompt, and model response.

## 4) Run batch matrix
- Switch mode to `Batch`.
- In `Batch specs`, use lines in this format:
  - `label|model|baseUrl|provider`
- Example:
  - `baseline|meta-llama/Meta-Llama-3.1-8B-Instruct|http://localhost:8000|openAi`
  - `candidate|meta-llama/Meta-Llama-3.1-70B-Instruct|http://localhost:8000|openAi`
- Click `Run Batch`.

## 5) Compare runs
- In `Runs`, click rows to add run IDs.
- In compare box, keep 2-5 run IDs (first ID is base).
- Click `Compare`.
- Output shows deterministic deltas:
  - `Δrisk`
  - `Δleaks`
  - `Δfind`
  - `gateChanged`

## 6) Export evidence
From `Results`:
- `Summary` -> JSON summary
- `Findings` -> JSONL finding records
- `Config` -> redacted run config

Use these artifacts for reports, audits, and regression tracking.

## 7) Troubleshooting
- `server=down`: web server is not running.
- `target returned 404`: wrong base URL or endpoint path.
- `at least one pack is required`: select or paste pack paths.
- `invalid pack path`: path is outside workspace or missing.
