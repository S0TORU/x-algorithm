# SentinelPipe / Gazetent

Author: Aanu Oshakuade

SentinelPipe is a local-first LLM red-teaming toolkit.
Gazetent is the web console for running tests, batch simulations, and reproducible security reports.

## Why it is useful
- Deterministic risk gates (`canaryLeak`, `injHeuristic`, `totalRisk`, `gatePass`).
- Fast iteration: single run, batch matrix runs, and run-to-run comparison.
- Audit-ready artifacts for every run.

## Quick start
1. Build and run the web console:
   ```bash
   cargo run -p gazetent-web
   ```
2. Open: `http://127.0.0.1:8787`
3. In UI:
   - Set `Provider`, `Base URL`, and `Model`.
   - Keep default packs or choose your own.
   - Click `Run Single` or switch to `Batch`.

## Main functionality
- Single run:
  - Run one model config against selected pack(s).
  - Inspect findings and open full prompt/response details.
- Batch matrix:
  - Use `label|model|baseUrl|provider` lines.
  - Execute multiple configs in one action.
- Runs explorer:
  - Browse previous runs from disk.
  - Click any run to reload findings and summary.
- Compare runs:
  - Select 2-5 run IDs.
  - Get deterministic deltas for risk, leaks, findings, and gate flips.
- Artifact export:
  - Download summary, findings, and redacted config from the UI.

## API endpoints
- `GET /api/health`
- `GET /api/packs/list`
- `POST /api/packs/preview`
- `POST /api/run`
- `POST /api/run/batch`
- `GET /api/runs/list`
- `POST /api/runs/compare`
- `GET /api/runs/:run_id`
- `GET /api/runs/:run_id/download/:file`

## Artifacts
Stored in `gazetent/runs/<run_id>/`:
- `summary.json`
- `findings.jsonl`
- `config.redacted.json`

## Security defaults
- Binds to localhost (`127.0.0.1:8787`) by default.
- API key is never written to run artifacts.
- API key is not persisted in browser storage.
- Pack paths are restricted to files inside the workspace root.
- CSP + basic hardening headers are set on responses.
- Request body size is capped to 1 MB.

See `USER_GUIDE.md` for operator walkthrough and `SECURITY.md` for policy/disclosure.

## License
Apache-2.0 (`LICENSE`).

-Aanu
