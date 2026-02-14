# Edge Cases & Failure Modes

This file documents edge cases observed directly in the codebase and provides production recommendations.

## Pipeline-Level Semantics (Important)

The pipeline engine (`candidate-pipeline/`) is best-effort:
- Query hydrator failures: logged, request continues.
- Source failures: logged, other sources still contribute candidates.
- Hydrator/scorer failures: logged, candidates continue unmodified.
- Filter failures: logged, filter is skipped and candidates revert to previous list.

This design is great for resiliency, but production systems must carefully choose where fail-open is acceptable.

## Home Mixer Request Edge Cases

### `viewer_id == 0`

- Behavior: request rejected with `INVALID_ARGUMENT`.
- Code: `home-mixer/server.rs`.

### Missing user action sequence (UAS)

- Cause:
  - UAS fetcher fails, or
  - no user actions exist (hydrator returns Err).
- Behavior:
  - `user_action_sequence` remains `None`.
  - Phoenix retrieval (`PhoenixSource`) errors with “missing user_action_sequence” and contributes 0 OON candidates.
  - Phoenix ranking (`PhoenixScorer`) also skips scoring.
- Net effect:
  - feed is INN-only and unpersonalized.

### Missing user features

- Cause: Strato call fails.
- Behavior:
  - `followed_user_ids` may be empty → Thunder returns empty.
  - blocks/mutes/muted keywords not applied.
- Recommendation:
  - Treat user features as required for privacy controls (blocks/mutes).
  - Consider fail-closed on `blocked_user_ids`/`muted_user_ids` missing.

### `in_network_only=true`

- Behavior:
  - Phoenix retrieval is disabled.
  - Side effect `CacheRequestInfoSideEffect` is disabled.

### `is_bottom_request=false`

- Behavior:
  - `PreviouslyServedPostsFilter` is disabled.
- Implication:
  - pagination duplicates can increase if the client does not provide `seen_ids`/Bloom filters.

## Candidate Hydration Edge Cases

### Hydrator length mismatches

- Behavior: hydrator output is skipped.
- Implication: candidates may proceed missing fields.
- Recommendation:
  - For critical hydrators (core tweet data), treat mismatches as fatal or drop impacted candidates.

### Core data hydration failures

- Behavior:
  - `CoreDataCandidateHydrator` sets defaults (author_id=0, empty text) when TES lacks data.
  - `CoreDataHydrationFilter` drops those candidates.

## Filtering Edge Cases

### Age filter false negatives

- `AgeFilter` uses Snowflake timestamp decoding.
- If decoding fails, `AgeFilter` drops the candidate (fail-closed).

### Seen/Bloom false positives

- `PreviouslySeenPostsFilter` uses Bloom filters.
- Bloom filters may yield false positives → over-filtering.
- Recommendation:
  - Monitor kept/removed ratios.
  - Keep `seen_ids` small and prefer server-side “seen” stores if available.

## Scoring Edge Cases

### Phoenix prediction unavailable

- Behavior: `PhoenixScorer` returns candidates unchanged.
- Downstream scorers still run but will see missing `phoenix_scores`.
- Net effect:
  - weights may evaluate to 0.0; candidates may still be returned but ordering quality drops.

### All candidates have `score=None`

- Selector treats missing scores as `-inf`.
- Candidates will still be truncated to Top-K; ordering may be arbitrary.

## Visibility Filtering Edge Cases

### VF unavailable (network/TLS/timeout)

- Behavior today:
  - VF hydrator errors cause the hydrator to be skipped.
  - Candidates retain `visibility_reason=None`.
  - `VFFilter` keeps `None` reasons.
- This is fail-open.

Recommended production options:
1. Fail-closed: if VF cannot be called, drop all candidates (or return an error).
2. Fail-partial: drop only OON candidates when VF is down.

## Thunder Edge Cases

### Empty follow graph handling

- If follow list is empty, PostStore lookups return no posts.
- Additionally, Thunder only fetches follows from Strato if `debug=true` (likely unintended).

### Event ordering (create/delete)

- Tombstones are used to ensure deleted content is removed even when delete arrives before create.
- `finalize_init()` removes posts that were tombstoned during catch-up.

### Request scan timeout

- PostStore scans the follow graph sequentially and stops if the timeout is exceeded.
- Partial results are returned.

## Recommended “Production Hardening” Changes

These changes are not implemented in the snapshot, but are commonly required:

- Decide fail-open vs fail-closed per dependency (VF, blocks/mutes, subscription eligibility).
- Add explicit per-stage timeouts, budgets, and circuit breakers.
- Add structured per-stage metrics (counts + latencies + failures).
- Fix Thunder follow-fetch behavior for empty follow list.
- Add load-shedding and request-size caps at Home Mixer boundary.
