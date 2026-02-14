# Thunder

Thunder provides **in-network (INN)** candidates: recent posts from accounts the viewer follows.

It is designed for low latency by keeping recent post state in memory.

## Entry Points

- Service main: `thunder/main.rs`
- gRPC service: `thunder/thunder_service.rs`
- Storage layer: `thunder/posts/post_store.rs`
- Kafka ingestion loop: `thunder/kafka/tweet_events_listener.rs`

## Responsibilities

- Ingest tweet create/delete events from Kafka.
- Maintain a per-author set of recent posts for a bounded retention window.
- Serve recent posts for a request user’s follow graph.

## Data Model

Thunder stores `LightPost` objects (protobuf type `xai_thunder_proto::LightPost`). In memory it uses:

- `posts`: `post_id -> LightPost`
- `original_posts_by_user`: `author_id -> VecDeque<TinyPost>` for original posts
- `secondary_posts_by_user`: `author_id -> VecDeque<TinyPost>` for replies/retweets
- `video_posts_by_user`: `author_id -> VecDeque<TinyPost>` for video-eligible posts
- `deleted_posts`: tombstone map to resolve out-of-order create/delete

See `thunder/posts/post_store.rs`.

## Ingestion

The Kafka ingestion code:
- Polls Kafka in a loop, batches messages, deserializes tweet events.
- Converts tweet create events to `LightPost`.
- Converts delete events to tombstones.
- Optionally republishes “in-network events” to another Kafka topic.

See `thunder/kafka/tweet_events_listener.rs`.

### Notable ingestion rules

- Filters out “nullcast” tweets (not eligible for timeline distribution).
- Filters video eligibility:
  - Only a single media entity is supported.
  - Requires video duration >= `MIN_VIDEO_DURATION_MS` (constant lives in a missing `thunder/config` module in this snapshot).

### Delete ordering

Because event ordering can be lost in the feeder, `PostStore::finalize_init()` removes any posts that were tombstoned.

## Serving (gRPC)

Thunder implements `InNetworkPostsService` with `get_in_network_posts` (`thunder/thunder_service.rs`).

### Backpressure

Thunder uses a semaphore to cap concurrent requests:
- If the semaphore cannot be acquired immediately, the service returns `RESOURCE_EXHAUSTED`.
- Clients should retry with exponential backoff.

### Request shaping

- `following_user_ids` and `exclude_tweet_ids` are capped to `MAX_INPUT_LIST_SIZE`.
- `max_results` defaults to `MAX_POSTS_TO_RETURN` or `MAX_VIDEOS_TO_RETURN`.

**Edge case**: the code only fetches a following list from Strato when `following_user_ids` is empty **and** `debug=true`. If `debug=false` and `following_user_ids` is empty, the request will return 0 results. Production deployments usually want to fetch follows regardless of debug mode.

### Scoring

The service “scores” posts by recency only:
- Sorts by `created_at` descending and truncates to `max_results`.

See `score_recent` in `thunder/thunder_service.rs`.

## PostStore Retrieval Rules

When scanning per-user timelines, PostStore:

- Scans from newest backwards.
- Skips excluded tweet IDs.
- Only scans up to `MAX_TINY_POSTS_PER_USER_SCAN` per user (prevents scanning deep history for inactive users).
- Filters tombstoned/deleted posts.
- Removes retweets of the request user’s own content.
- Applies reply/thread heuristics to avoid irrelevant replies.
- Enforces a request timeout (configurable) across scanning.

See `PostStore::get_posts_from_map` in `thunder/posts/post_store.rs`.

## Retention / Trimming

- Inserts filter out posts older than the retention window, and posts “from the future”.
- `start_auto_trim()` periodically trims old posts.
- Trimming also shrinks internal `VecDeque` capacity to limit memory overhead.

## Production Guidance

- Provision enough RAM to hold the desired retention window.
- Alert on:
  - Kafka lag (partition lag monitor)
  - request rejections (`RESOURCE_EXHAUSTED`)
  - request timeouts during PostStore scans
- Ensure delete/create ordering is handled consistently across ingestion paths.
