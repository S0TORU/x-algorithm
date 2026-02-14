# Candidate Pipeline Framework

This repository’s Rust services use the reusable pipeline engine in `candidate-pipeline/`.

The framework is intentionally minimal: it provides stage ordering, concurrency, and error-handling conventions; business logic lives in per-service implementations.

## Core Concepts

- **Query (`Q`)**: request context.
- **Candidate (`C`)**: an item flowing through the pipeline.
- **Stage contracts** are enforced by convention:
  - Hydrators and scorers must preserve ordering and length.
  - Filters may drop candidates.

## Stage Model

The pipeline interface is defined by `CandidatePipeline<Q, C>` in `candidate-pipeline/candidate_pipeline.rs`.

Stages (in order):

1. Query hydrators
2. Sources
3. Hydrators
4. Filters
5. Scorers
6. Selector
7. Post-selection hydrators
8. Post-selection filters
9. Side effects

### Query Hydrators

Trait: `QueryHydrator<Q>` (`candidate-pipeline/query_hydrator.rs`).

- Run in parallel.
- Each hydrator returns a `Q` with only its fields populated.
- The pipeline calls `update(&mut query, hydrated)` to merge fields.

Error handling:
- A query hydrator failure logs an error and is skipped.
- Other hydrators can still update the query.

### Sources

Trait: `Source<Q, C>` (`candidate-pipeline/source.rs`).

- Run in parallel.
- Results are concatenated.

Error handling:
- A source failure logs an error and contributes zero candidates.
- Other sources still run.

### Hydrators

Trait: `Hydrator<Q, C>` (`candidate-pipeline/hydrator.rs`).

Contract:
- `hydrate(query, candidates)` must return a `Vec<C>` of the **same length** and **same ordering** as the input slice.
- Hydrators are not allowed to drop candidates.

Enforcement:
- The pipeline checks vector length; mismatches are warned and the hydrator result is skipped.

Error handling:
- A hydrator failure logs an error and is skipped.
- This means candidates may continue with missing fields.

### Filters

Trait: `Filter<Q, C>` (`candidate-pipeline/filter.rs`).

- Run sequentially.
- Each filter returns `FilterResult { kept, removed }`.

Error handling:
- If a filter errors, the pipeline restores the previous candidate list (best-effort) and continues.

### Scorers

Trait: `Scorer<Q, C>` (`candidate-pipeline/scorer.rs`).

Contract:
- Same as hydrators: return vector length/order must match.

Error handling:
- If a scorer errors, it is skipped and candidates proceed unmodified.

### Selector

Trait: `Selector<Q, C>` (`candidate-pipeline/selector.rs`).

Default behavior:
- Sorts by `score(&candidate)` descending.
- Optionally truncates to `size()`.

### Side Effects

Trait: `SideEffect<Q, C>` (`candidate-pipeline/side_effect.rs`).

- Executes asynchronously after a response is produced.
- Runs in parallel.
- Best-effort: failures should not impact response.

## Production Guidance

### Make “fail-open” explicit

The current engine design naturally fails open.

For stages that are safety-critical (e.g., visibility filtering), production systems typically prefer fail-closed.

Two common patterns:

1. **Hard filter**: if a dependency is unavailable, drop all candidates that require that signal.
2. **Gate the response**: if required safety data is missing, return an error upstream so the caller can fallback.

### Monitoring expectations

Because stage failures do not necessarily manifest as request failures, production deployments should:
- Emit per-stage success/failure counters.
- Track candidate counts after each stage.
- Alert on stage failure rates and “empty feed” rates.

## Extension Checklist

When adding a new stage component:

- `enable()` is the primary mechanism for conditional execution.
- Hydrators/scorers must preserve ordering and length.
- `update()`/`update_all()` must only copy fields owned by that component.
- If you need to drop candidates, implement a filter.
- If a stage is safety-critical, decide fail-open vs fail-closed and encode it in your implementation.
