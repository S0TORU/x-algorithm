# For You Feed: Production Documentation

This directory contains the production-oriented documentation for the **For You** feed system implemented in this repository.

**Scope** (this doc set covers only these directories):
- `home-mixer/`: request orchestration + ranking pipeline assembly
- `candidate-pipeline/`: generic staged pipeline execution framework
- `thunder/`: in-network (follow-graph) candidate store + retrieval service
- `phoenix/`: reference JAX implementation of retrieval + ranking models (demo code)

**Important note about this repository snapshot**
- Some modules referenced by the Rust services are intentionally **excluded from the open-source release** (e.g. `home-mixer/params`, `home-mixer/clients`, `home-mixer/util`, and `thunder/config`).
- This documentation is written to be accurate to the code present here, and explicitly calls out places where production deployments must supply additional configuration or internal services.

## Start Here

- `docs/for-you-feed/architecture.md`: end-to-end system architecture and request data flow
- `docs/for-you-feed/home-mixer.md`: Home Mixer API, pipeline wiring, and runtime behavior
- `docs/for-you-feed/candidate-pipeline.md`: pipeline contracts, concurrency model, and extension patterns
- `docs/for-you-feed/thunder.md`: Thunder ingestion + in-network retrieval service
- `docs/for-you-feed/phoenix.md`: Phoenix retrieval/ranking model reference and interfaces
- `docs/for-you-feed/operations.md`: production checklist, SLOs, scaling, observability, and runbooks
- `docs/for-you-feed/edge-cases.md`: known edge cases, “fail-open/closed” choices, and mitigations

## High-Level Request Path

1. Client calls Home Mixer `GetScoredPosts` (gRPC).
2. Home Mixer hydrates request context:
   - user action sequence (engagement history)
   - user features (follows, blocks, mutes, subscriptions, muted keywords)
3. Home Mixer retrieves candidates:
   - in-network from Thunder
   - out-of-network from Phoenix retrieval (disabled when `in_network_only=true`)
4. Candidates are hydrated (tweet core data, author data, video metadata, etc.) and filtered.
5. Candidates are scored (Phoenix ranking → weighted score → author diversity → in-network preference).
6. Top-K candidates are selected.
7. Post-selection hydration + filtering runs (visibility filtering + conversation dedup).
8. Best-effort side effects run (e.g. cache request info).

## Glossary

- **Candidate**: a post/tweet that might appear in the feed.
- **Hydrator**: enriches candidates or the query; must preserve ordering/length.
- **Filter**: removes candidates.
- **Scorer**: produces per-candidate scores; must preserve ordering/length.
- **Selector**: sorts/truncates candidates to final size.
- **In-network (INN)**: content from accounts the viewer follows.
- **Out-of-network (OON)**: content retrieved from the global corpus.
