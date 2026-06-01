# 5v5 Real-Time Competitive Matchmaker

High-performance, thread-safe in-memory matchmaking for 5v5 competitive games. Built in **Rust** (Tokio + Axum) with concurrent worker scans, time-based constraint relaxation, optimal team balancing, and low-overhead metrics.

---

## Deliverables (required)

| # | Deliverable | Link / location |
|---|-------------|-----------------|
| 1 | **GitHub repo — working code** | [github.com/SBiswal02/CompetitiveMatchmaker](https://github.com/SBiswal02/CompetitiveMatchmaker) |
| 2 | **Simulation script** | [`scripts/load_simulation.py`](scripts/load_simulation.py) · wrapper: [`scripts/run_load_simulation.sh`](scripts/run_load_simulation.sh) |
| 3 | **README / engineering write-up** | This document (sections below) |

### Quick verification

```bash
# 1. Build & test the service
cargo test --workspace
cargo run -p matchmaker-service

# 2. In another terminal — inject 5,000 concurrent enqueues
python3 scripts/load_simulation.py --players 5000 --concurrency 250
# or: make simulate
```

---

## Architecture

```
┌─────────────────┐     POST /queue      ┌──────────────────┐
│  Game Client    │ ───────────────────► │ matchmaker-service│
└─────────────────┘                      │  (Axum + Tokio)   │
                                         └────────┬─────────┘
                                                  │
                    ┌─────────────────────────────┼─────────────────────────────┐
                    │                             ▼                             │
                    │  ┌──────────────┐   N workers (50ms)   ┌──────────────┐  │
                    │  │ PlayerPool   │ ◄── scan_once() ───► │ Matchmaker   │  │
                    │  │ (DashMap)    │                      │ Engine       │  │
                    │  └──────────────┘                      └──────┬───────┘  │
                    │         matchmaker-core                          │         │
                    └──────────────────────────────────────────────────┼─────────┘
                                                                       ▼
                                                              10-player lobby
                                                              → 5v5 balance split
```

| Crate | Role |
|-------|------|
| `matchmaker-types` | Shared DTOs (`EnqueueRequest`, `Match`, `HealthSnapshot`) |
| `matchmaker-core` | Pool, relaxation, matcher, team balance, metrics |
| `matchmaker-service` | HTTP API + background worker tasks |

---

## Engineering challenges & how they were tackled

### 1. Latency vs. match quality

**Problem:** Tight skill matching increases wait time; loose matching forms games quickly but feels unfair.

**Approach:** A **time-based relaxation policy** tied to the *longest-waiting* player in a region (the “anchor”):

- Skill band starts at ±100 and grows by **+5 per second** of anchor wait (cap 500).
- Minimum acceptable lobby quality starts at **0.7** and relaxes toward **0.4** over ~2 minutes.
- Maximum allowed team skill gap widens with wait (+10 per 30s, cap 150).

**Trade-off:** Early matches are high quality; players who would otherwise sit in queue indefinitely (very high or very low skill) eventually become matchable. This mirrors production systems (e.g. widening MMR search range over time).

### 2. Thread-safe state & atomic eviction

**Problem:** Multiple matching workers scan the same in-memory pool concurrently. Races must not double-match players or leave the pool inconsistent.

**Approach:**

- **`DashMap<Uuid, QueuedPlayer>`** — sharded concurrent hash map; enqueue/dequeue/snapshot without a global mutex.
- **`try_remove_batch`** — verifies all 10 player IDs exist, then removes atomically per shard; if any ID is missing (another worker won the race), the whole batch aborts.
- **`generation` counter** — incremented on eviction or successful removal so future extensions can detect stale snapshots.
- **`evict_expired`** — time-based TTL (default 300s); safe to call from every worker on each scan.

**Trade-off:** `DashMap` uses more memory than a single `RwLock<HashMap>` but scales better under concurrent reads/writes from HTTP handlers and N workers.

### 3. Time-based constraint relaxation

Implemented in `crates/matchmaker-core/src/relaxation.rs` and applied in `matcher.rs` when evaluating a lobby anchored on the oldest waiter. Relaxation is **per-region** so latency-sensitive grouping stays geographically coherent.

### 4. Team balance optimization

**Problem:** Finding 10 similar players is not enough — they must split into two fair teams of 5.

**Approach:** After greedy selection of the 10 players closest to the anchor skill, **`balance_teams`** enumerates all **C(10,5) = 252** team splits and picks the partition minimizing `|avg(team_a) − avg(team_b)|`.

**Trade-off:** 252 iterations is trivial on modern CPUs (microseconds). For much larger lobby sizes, you’d switch to greedy or DP approximations; for fixed 5v5, exhaustive search is optimal and simple.

### 5. Low-latency health metrics

**Problem:** Observability must not block the matching hot path.

**Approach:**

- Hot path: **`AtomicU64`** for queue depth, scan count, matches formed, evictions.
- P99 scan latency: small in-memory rolling buffer (512 samples), updated in `record_scan` without network I/O.
- Match quality: exponential moving average (α = 0.1), updated only when a match commits.

`GET /health` and `GET /metrics` read atomics and return a `HealthSnapshot` JSON payload suitable for load tests and CloudWatch-style scraping.

---

## Algorithmic trade-offs

| Decision | Chosen | Alternative | Why |
|----------|--------|-------------|-----|
| Lobby selection | Greedy: 10 players nearest anchor skill | Full clustering / graph matching | O(n log n) per scan; good enough with relaxation |
| Anchor player | Longest waiter in region | Random / median skill | Fairness for outliers; drives relaxation |
| Team split | Exhaustive 252 subsets | Snake draft / random | Optimal balance for fixed 5v5 |
| Concurrency | N independent periodic scanners | Single-threaded event loop | Utilizes cores; races handled by `try_remove_batch` |
| Storage | In-memory only | Redis/DB in hot path | Minimum latency; Redis planned for horizontal scale |

---

## Time & space complexity

Let **n** = players in a region snapshot, **W** = number of workers, **R** = regions.

| Operation | Time | Space |
|-----------|------|-------|
| `enqueue` / `dequeue` | O(1) average (DashMap) | O(1) per player |
| `snapshot_by_region` | O(n) | O(n) temporary vec |
| `find_best_lobby` (filter + sort 10) | O(n log n) sort on candidates | O(n) |
| `balance_teams` | O(1) — fixed 252 masks | O(1) |
| `scan_once` (all regions) | O(R × (n log n + matches)) | O(n) per region snapshot |
| `evict_expired` | O(n) scan of map | O(k) for k evicted IDs |

**Per worker per second:** With 4 workers at 50ms interval, up to ~80 scans/sec. Each scan is cheap if regions are moderate size; bottleneck at scale becomes **snapshot + sort per region**, not team balance.

**Memory:** O(P) for P queued players globally, plus DashMap shard overhead. No per-match persistence in the hot path.

---

## Scaling challenges & production path

| Challenge | In this repo | Production direction |
|-----------|--------------|----------------------|
| **Single instance memory** | All queues in RAM | Shard by region; cap queue depth; Redis for overflow tickets |
| **Horizontal scale** | One process owns truth | Sticky routing by region; or Redis queue + leader-elected scanner |
| **Thundering herd** | Many workers may scan same lobby | `try_remove_batch` prevents double-commit; only one wins |
| **Cross-region parties** | Not implemented | Party ID constraints + same-region validation |
| **Skill persistence** | Skill passed on enqueue | PostgreSQL + TrueSkill/Elo updates post-game |
| **Match delivery** | Metrics only | Kafka `match.created` → game server allocator |
| **Observability at scale** | In-process atomics | CloudWatch/Prometheus sidecar; avoid logging in `scan_once` |

**AWS-shaped deployment:** ECS service behind ALB; ElastiCache for shared queue state if multi-task; RDS for history; Lambda optional for async analytics only (not in the match hot path).

---

## API

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/queue` | POST | Join queue — body: `player_id`, `skill`, `region`, optional `role`, `party_id` |
| `/queue/{player_id}` | DELETE | Leave queue |
| `/health` | GET | Queue depth, matches formed, scan/eviction counts, p99 scan µs |
| `/metrics` | GET | Same as `/health` |

### Example

```bash
curl -s -X POST http://localhost:8080/queue \
  -H 'Content-Type: application/json' \
  -d '{"player_id":"550e8400-e29b-41d4-a716-446655440000","skill":1500,"region":"us-east"}'

curl -s http://localhost:8080/health | jq
```

### Environment

| Variable | Default | Description |
|----------|---------|-------------|
| `MATCHMAKER_BIND` | `0.0.0.0:8080` | Listen address |
| `MATCHMAKER_WORKERS` | `4` | Concurrent scan workers |
| `MATCHMAKER_SCAN_INTERVAL_MS` | `50` | Scan interval per worker |
| `RUST_LOG` | `info` | Log filter |

---

## Simulation script (deliverable #2)

**File:** [`scripts/load_simulation.py`](scripts/load_simulation.py)

Stdlib-only Python script that:

1. Waits for the service to respond on `/health`
2. Fires **N concurrent POST /queue** requests (default **5,000**, concurrency **250**)
3. Reports enqueue throughput and latency percentiles (mean, p50, p95, p99)
4. Waits for workers to settle, then compares `/health` `matches_formed_total` to expected `players / 10`

```bash
# Service must be running first
cargo run -p matchmaker-service

# Default load test
python3 scripts/load_simulation.py

# Heavier run
python3 scripts/load_simulation.py --players 10000 --concurrency 400 --settle-secs 20

# Via Makefile
make simulate
make simulate-heavy
```

**Options:** `--base-url`, `--players`, `--concurrency`, `--skill-base`, `--skill-spread`, `--settle-secs`, `--timeout`

---

## Docker

```bash
docker compose up --build
# then run simulation against http://127.0.0.1:8080
python3 scripts/load_simulation.py --players 5000
```

Or:

```bash
make docker-build && make docker-run
```

---

## Development

```bash
cargo test --workspace
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```

CI: [`.github/workflows/ci.yml`](.github/workflows/ci.yml) — format, clippy, tests, release build.

---

## License

MIT
