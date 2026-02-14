# Operations (Production Readiness)

This document describes what you typically need to operate the For You feed system as a production service.

Because this repository snapshot intentionally omits some internal modules and service clients, this file describes *requirements* and *recommended controls* rather than exact deployment manifests.

## Service Inventory

- **Home Mixer** (`home-mixer/`)
  - gRPC entrypoint: `home-mixer/main.rs`
  - main responsibility: assemble the pipeline and return ranked posts

- **Thunder** (`thunder/`)
  - gRPC entrypoint: `thunder/main.rs`
  - main responsibility: in-memory store for recent in-network posts

- **Phoenix** (external in production)
  - retrieval service (OON candidates)
  - prediction service (ranking scores)

## Availability & SLOs

Typical SLOs to consider:

- Home Mixer
  - Availability: >= 99.9%
  - Latency (p95): single-digit to low tens of milliseconds (highly dependent on downstream RPCs)

- Thunder
  - Availability: >= 99.9%
  - Latency (p95): low milliseconds

When choosing SLOs, explicitly account for downstream services:
- UAS store, Strato, TES, VF, Phoenix.

## Backpressure & Timeouts

### Thunder

- Uses semaphore-based backpressure and returns `RESOURCE_EXHAUSTED` when at capacity.
- Uses an overall “scan timeout” while iterating over follow graph timelines.

Recommended:
- Client retry with exponential backoff.
- Alert on:
  - request rejection rate
  - PostStore scan timeout count

### Home Mixer

The generic pipeline framework is largely “best-effort” (stage failures log and continue).

Recommended:
- Apply explicit timeouts per dependency.
- Decide which dependencies are fail-open vs fail-closed.

Safety-critical dependencies typically include:
- visibility filtering
- blocks/mutes

## Observability

### Logging

- Ensure `request_id` is logged for each Home Mixer request (`home-mixer/candidate_pipeline/query.rs`).
- Emit structured logs per pipeline stage:
  - candidate counts after each stage
  - stage latency
  - stage failures

### Metrics

Minimal metrics to run production:

Home Mixer:
- total requests, request latency
- candidate counts after: sources, hydrators, filters, scorers, selector
- per-stage failure counters (query hydrators, sources, hydrators, filters, scorers)
- empty response rate

Thunder:
- request counts, request latency
- rejected requests (`RESOURCE_EXHAUSTED`)
- PostStore entity gauges (users/posts/deleted)
- Kafka lag by topic/partition
- request scan timeouts

## Deployment & Scaling

### Home Mixer

- Horizontally scalable.
- CPU-bound when downstream RPCs are local/fast; network-bound otherwise.
- Ensure gRPC max message sizes and compression match client expectations.

### Thunder

- Stateful (in-memory) service.
- Horizontal scaling options:
  - shard by author ID or partition streams
  - run multiple clusters and have Home Mixer pick random channel (as it does now)

## Data Governance / Security

- Ensure UAS and user features access is authorized and audited.
- Treat “seen_ids” / “served_ids” as user-derived; validate sizes to avoid abuse.
- VF calls should use mutual TLS / service identity (Home Mixer references S2S cert paths).

## Runbooks

### 1) Home Mixer returning empty feeds

Checklist:
- Are query hydrators failing (missing UAS/user features)?
- Is `in_network_only=true` accidentally set?
- Is the user’s follow graph empty?
- Are filters removing everything (e.g., seen/served/bloom filters too aggressive)?
- Are downstream dependencies timing out?

### 2) Thunder high rejection rate

Checklist:
- Increase `max_concurrent_requests`.
- Reduce per-request work:
  - reduce `MAX_INPUT_LIST_SIZE` caps
  - reduce `MAX_TINY_POSTS_PER_USER_SCAN`
  - increase request_timeout to avoid long scans (or tighten if tail is too heavy)
- Validate Kafka ingestion health (if store is stale, more scanning may be needed).

### 3) Visibility Filtering unavailable

Decide policy:
- Fail-open: return candidates without VF (current behavior if VF hydrator errors).
- Fail-closed: drop all candidates requiring VF if VF is down.

See `docs/for-you-feed/edge-cases.md` for recommended behavior.
