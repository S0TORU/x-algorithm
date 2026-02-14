# Phoenix (Retrieval + Ranking Models)

`phoenix/` contains a reference JAX implementation of the Phoenix recommendation models.

This code is **model/demo code** rather than a production inference service; Home Mixer integrates with Phoenix through clients that are excluded from this repository snapshot.

## Two-Stage Model

Phoenix is structured as:

1. **Retrieval** (global candidate discovery)
   - Produces top-K candidate IDs from a large corpus.
   - Typically implemented via a two-tower model and approximate nearest neighbor search.

2. **Ranking** (final ordering)
   - Produces multi-action engagement probabilities per candidate.
   - Implemented via a transformer with **candidate isolation**.

See `phoenix/README.md` for an overview.

## Candidate Isolation

The ranking transformer uses attention masking so that candidates do **not** attend to each other (they may attend to user context/history). This makes each candidate’s score stable regardless of which other candidates are co-batched.

This property is important for production:
- scores are consistent
- caching becomes feasible
- debug/reproducibility is improved

## Model Outputs

The reference `phoenix/runners.py` defines action outputs (`ACTIONS`) that align with the Home Mixer `PhoenixScores` structure.

Notable actions include:
- positive: favorite/like, reply, repost, quote, click, share
- negative: not-interested, block-author, mute-author, report
- continuous: dwell time

## Demos

- Ranker demo: `phoenix/run_ranker.py`
  - Builds a small transformer config and ranks 8 candidates.

- Retrieval demo: `phoenix/run_retrieval.py`
  - Builds a retrieval model and retrieves top-K from a simulated corpus.

## Production Integration Points

Home Mixer expects two Phoenix-facing RPC-like operations (exact RPC protocol is in excluded clients):

- Retrieval: `retrieve(user_id, user_action_sequence, top_k)`
  - returns candidate tweet IDs and author IDs.

- Ranking: `predict(user_id, user_action_sequence, tweet_infos)`
  - returns action log-probabilities per candidate.

Home Mixer’s scorer logic:
- converts log-probabilities to probabilities via `exp(log_prob)`
- attaches per-action probabilities to the `PostCandidate`

See `home-mixer/scorers/phoenix_scorer.rs`.

## Production Guidance

- Standardize the contract between Phoenix inference service and Home Mixer:
  - versioned schemas
  - explicit timeouts
  - max batch sizes
  - partial failure behavior
- Ensure retrieval and ranking operate on consistent content IDs (especially for retweets, quoting, and deleted content).
