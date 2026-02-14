# Architecture

## Components

```
Client
  │  gRPC: GetScoredPosts(viewer_id, seen_ids, served_ids, ...)
  ▼
Home Mixer (`home-mixer/`)
  - Orchestrates a staged CandidatePipeline
  - Combines INN + OON candidates, hydrates, filters, scores, selects
  │
  ├─ Query hydrators
  │   - User action sequence (UAS): engagement history
  │   - User features: follows, blocks, mutes, muted keywords, subscriptions
  │
  ├─ Sources
  │   - Thunder (INN): recent posts from followed accounts
  │   - Phoenix Retrieval (OON): embedding similarity retrieval from global corpus
  │
  ├─ Hydrators
  │   - Tweet core data (text, author, reply/retweet fields)
  │   - Video duration
  │   - Subscription metadata
  │   - Author metadata
  │
  ├─ Filters (pre-scoring)
  │   - drop duplicates, age, self-post, retweet dedup
  │   - eligibility filters (subscription, blocks/mutes, muted keywords)
  │   - seen/served filters
  │
  ├─ Scorers
  │   - Phoenix ranking predictions (multi-action)
  │   - Weighted score aggregation
  │   - Author diversity attenuation
  │   - OON down-weighting
  │
  ├─ Selector
  │   - Top-K by final score
  │
  ├─ Post-selection hydration + filters
  │   - Visibility filtering (VF)
  │   - Conversation branch de-dup
  │
  └─ Side effects (best-effort)
      - Cache request info (prod-only)

Thunder (`thunder/`)
  - Ingests tweet create/delete events
  - Maintains an in-memory PostStore with bounded retention
  - Serves recent posts for a viewer’s follow graph

Phoenix (`phoenix/`)
  - Reference JAX implementations for:
    - Retrieval (two-tower / embedding similarity)
    - Ranking (transformer w/ candidate isolation)
```

## External Dependencies (Production)

Home Mixer references several service clients that are **not present** in this repository snapshot (`home-mixer/clients` is excluded). In production these typically include:

- **Thunder gRPC**: in-network post retrieval.
- **Phoenix Retrieval**: returns candidate post IDs for OON.
- **Phoenix Prediction**: returns multi-action ranking probabilities.
- **UAS Store**: user action sequence fetcher (viewer engagement history).
- **Strato**: user features (followed IDs, muted keywords, blocks/mutes, subscriptions).
- **Tweet Entity Service (TES)**: tweet core data, media, and other tweet metadata.
- **Gizmoduck**: author metadata (screen name, followers count).
- **Visibility Filtering (VF)**: safety/visibility decisions per tweet.

## End-to-End Data Flow

### Request → Query

Home Mixer receives a `pb::ScoredPostsQuery` gRPC request and constructs an internal `ScoredPostsQuery` (`home-mixer/candidate_pipeline/query.rs`).

Notable query fields:
- `viewer_id` (required, rejects `0`)
- `seen_ids`: explicit IDs the client has already seen
- `served_ids`: IDs served earlier in the session (used for pagination)
- `bloom_filter_entries`: probabilistic “seen” set
- `in_network_only`: disables Phoenix retrieval
- `is_bottom_request`: enables the “served” filter

### Query Hydration

Query hydration runs **in parallel** (see `candidate-pipeline/candidate_pipeline.rs`).

- `UserActionSeqQueryHydrator` (`home-mixer/query_hydrators/user_action_seq_query_hydrator.rs`)
  - Fetches UAS, aggregates & truncates to a max sequence length.
  - Failure does **not** fail the request; it logs and leaves UAS unset.
- `UserFeaturesQueryHydrator` (`home-mixer/query_hydrators/user_features_query_hydrator.rs`)
  - Fetches user features from Strato.
  - Failure does **not** fail the request; it logs and leaves defaults.

### Candidate Retrieval

Sources run **in parallel** and their outputs are concatenated.

- `ThunderSource` (`home-mixer/sources/thunder_source.rs`)
  - Requires `query.user_features.followed_user_ids` (empty follow graph → empty results).
- `PhoenixSource` (`home-mixer/sources/phoenix_source.rs`)
  - Disabled when `in_network_only=true`.
  - Requires `query.user_action_sequence`.

### Candidate Hydration & Filtering

Hydrators run in parallel, and must return the **same length** vector; otherwise their result is skipped (warned) by the pipeline framework.

Filters run sequentially and partition candidates into kept/removed.

### Scoring & Selection

Scorers run sequentially, and must return the **same length** vector; otherwise their result is skipped.

Selection sorts by candidate score and truncates to Top-K.

### Post-Selection Visibility Filtering

VF hydration runs after selection to avoid “wasting” VF calls on candidates that will not be returned.

VF filter runs after VF hydration.

## Reliability: Fail-Open vs Fail-Closed

The generic pipeline implementation is “best-effort”: most stage failures are logged and the pipeline continues.

In production you should explicitly decide where you want:
- **Fail-open** (return something even if signals are missing), versus
- **Fail-closed** (drop content when safety/eligibility signals are missing)

See `docs/for-you-feed/edge-cases.md` for concrete recommendations.
