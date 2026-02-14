# Home Mixer

Home Mixer is the feed orchestration service. It exposes a gRPC endpoint that returns a ranked set of posts for a viewer.

## Entry Points

- gRPC server main: `home-mixer/main.rs`
- Service implementation: `home-mixer/server.rs`
- Pipeline assembly: `home-mixer/candidate_pipeline/phoenix_candidate_pipeline.rs`

## API

Home Mixer implements `pb::ScoredPostsService` and exposes `get_scored_posts` (`home-mixer/server.rs`).

Request validation:
- Rejects requests where `viewer_id == 0` with `INVALID_ARGUMENT`.

Transport:
- gRPC compression enabled (gzip + zstd).
- Message size limits reference `home-mixer/params` (module excluded from this snapshot).

## Internal Data Model

### Query

`ScoredPostsQuery` (`home-mixer/candidate_pipeline/query.rs`)

Fields include:
- viewer identifiers: `user_id`, `client_app_id`, `country_code`, `language_code`
- request-level controls: `in_network_only`, `is_bottom_request`
- seen/served control sets: `seen_ids`, `served_ids`, `bloom_filter_entries`
- hydrated fields: `user_action_sequence`, `user_features`

`request_id` is generated at query construction and is used in logs/metrics.

### Candidate

`PostCandidate` (`home-mixer/candidate_pipeline/candidate.rs`)

Key fields:
- Identity: `tweet_id`, `author_id`
- Threading/relationships: `in_reply_to_tweet_id`, `retweeted_tweet_id`, `retweeted_user_id`, `ancestors`
- Scoring: `phoenix_scores`, `weighted_score`, `score`
- Metadata: `tweet_text`, `video_duration_ms`, `author_followers_count`, `author_screen_name`
- Safety: `visibility_reason`

## Pipeline Wiring (Production Assembly)

`PhoenixCandidatePipeline::prod()` (`home-mixer/candidate_pipeline/phoenix_candidate_pipeline.rs`) assembles the full pipeline:

### Query hydrators

- `UserActionSeqQueryHydrator` (`home-mixer/query_hydrators/user_action_seq_query_hydrator.rs`)
  - Fetches UAS, aggregates and truncates.
  - Returns an error when no actions exist; pipeline logs and continues.

- `UserFeaturesQueryHydrator` (`home-mixer/query_hydrators/user_features_query_hydrator.rs`)
  - Fetches feature blob from Strato.

### Sources

- `ThunderSource` (`home-mixer/sources/thunder_source.rs`)
  - Fetches recent INN posts for the viewer’s `followed_user_ids`.

- `PhoenixSource` (`home-mixer/sources/phoenix_source.rs`)
  - Disabled when `in_network_only=true`.
  - Requires `user_action_sequence`.

### Hydrators

- `InNetworkCandidateHydrator` (`home-mixer/candidate_hydrators/in_network_candidate_hydrator.rs`)
  - Sets `in_network` and/or served type markers.

- `CoreDataCandidateHydrator` (`home-mixer/candidate_hydrators/core_data_candidate_hydrator.rs`)
  - Fetches core tweet data (text, author id, reply/retweet fields).

- `VideoDurationCandidateHydrator` (`home-mixer/candidate_hydrators/video_duration_candidate_hydrator.rs`)
  - Fetches video duration and sets `video_duration_ms`.

- `SubscriptionHydrator` (`home-mixer/candidate_hydrators/subscription_hydrator.rs`)
  - Sets subscription-related eligibility fields.

- `GizmoduckCandidateHydrator` (`home-mixer/candidate_hydrators/gizmoduck_hydrator.rs`)
  - Fetches author profile counts/screen names.

### Filters (pre-scoring)

- `DropDuplicatesFilter` (`home-mixer/filters/drop_duplicates_filter.rs`)
- `CoreDataHydrationFilter` (`home-mixer/filters/core_data_hydration_filter.rs`)
  - Drops candidates lacking `author_id` or empty `tweet_text`.
- `AgeFilter` (`home-mixer/filters/age_filter.rs`)
  - Uses Snowflake timestamp decoding (implementation is in excluded `home-mixer/util`).
- `SelfTweetFilter` (`home-mixer/filters/self_tweet_filter.rs`)
- `RetweetDeduplicationFilter` (`home-mixer/filters/retweet_deduplication_filter.rs`)
- `IneligibleSubscriptionFilter` (`home-mixer/filters/ineligible_subscription_filter.rs`)
- `PreviouslySeenPostsFilter` (`home-mixer/filters/previously_seen_posts_filter.rs`)
  - Uses both `seen_ids` and Bloom filter entries.
- `PreviouslyServedPostsFilter` (`home-mixer/filters/previously_served_posts_filter.rs`)
  - Only enabled when `is_bottom_request=true`.
- `MutedKeywordFilter` (`home-mixer/filters/muted_keyword_filter.rs`)
- `AuthorSocialgraphFilter` (`home-mixer/filters/author_socialgraph_filter.rs`)

### Scorers

- `PhoenixScorer` (`home-mixer/scorers/phoenix_scorer.rs`)
  - Calls the Phoenix prediction service.
  - If the call fails, returns candidates unchanged (fail-open).

- `WeightedScorer` (`home-mixer/scorers/weighted_scorer.rs`)
  - Computes a weighted sum across predicted action probabilities.
  - Calls `normalize_score` from `home-mixer/util` (excluded).

- `AuthorDiversityScorer` (`home-mixer/scorers/author_diversity_scorer.rs`)
  - Applies a per-author decay multiplier to prevent one author dominating.

- `OONScorer` (`home-mixer/scorers/oon_scorer.rs`)
  - Down-weights OON candidates (`in_network == Some(false)`).

### Selector

- `TopKScoreSelector` (`home-mixer/selectors/top_k_score_selector.rs`)
  - Sorts by `candidate.score` descending.
  - Truncates to `TOP_K_CANDIDATES_TO_SELECT` (constant lives in excluded `home-mixer/params`).

### Post-selection hydrators + filters

- `VFCandidateHydrator` (`home-mixer/candidate_hydrators/vf_candidate_hydrator.rs`)
  - Calls Visibility Filtering with different safety levels for INN vs OON.

- `VFFilter` (`home-mixer/filters/vf_filter.rs`)
  - Drops candidates that VF says must be dropped.
  - Note: if VF hydration fails, `visibility_reason` may be `None` and the filter will keep candidates.

- `DedupConversationFilter` (`home-mixer/filters/dedup_conversation_filter.rs`)
  - Keeps only the highest scored candidate per conversation branch.

### Side effects

- `CacheRequestInfoSideEffect` (`home-mixer/side_effects/cache_request_info_side_effect.rs`)
  - Enabled only when `APP_ENV=prod` and `in_network_only=false`.
  - Best-effort; failures do not fail the request.

## Production Notes

- **Safety**: VF is post-selection; decide whether fail-open is acceptable.
- **Empty-feed risk**: Missing UAS disables Phoenix retrieval, and empty follow graphs disable Thunder.
- **Observability**: Prefer per-stage candidate counts and per-dependency error rates.

See `docs/for-you-feed/operations.md` and `docs/for-you-feed/edge-cases.md` for concrete runbooks.
