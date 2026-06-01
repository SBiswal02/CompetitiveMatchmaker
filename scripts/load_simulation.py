#!/usr/bin/env python3
"""
Load simulation for the 5v5 matchmaker service.

Injects thousands of concurrent enqueue requests against a running instance,
then polls /health to measure throughput, latency, and matches formed.

Usage:
  # Terminal 1: start the service
  cargo run -p matchmaker-service

  # Terminal 2: run simulation
  python3 scripts/load_simulation.py --players 5000 --concurrency 250

Requires: Python 3.9+ (stdlib only).
"""

from __future__ import annotations

import argparse
import json
import statistics
import sys
import time
import uuid
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
from typing import Any
from urllib import error, request

DEFAULT_BASE_URL = "http://127.0.0.1:8080"
REGIONS = ("us-east", "us-west", "eu-west")


@dataclass
class EnqueueResult:
    ok: bool
    latency_ms: float
    status: int | None = None
    error: str | None = None


def http_json(
    method: str,
    url: str,
    body: dict[str, Any] | None = None,
    timeout: float = 10.0,
) -> tuple[int, dict[str, Any] | None, float]:
    """Returns (status_code, parsed_json_or_none, latency_ms)."""
    data = None
    headers = {"Content-Type": "application/json", "Accept": "application/json"}
    if body is not None:
        data = json.dumps(body).encode("utf-8")
    req = request.Request(url, data=data, headers=headers, method=method)
    start = time.perf_counter()
    try:
        with request.urlopen(req, timeout=timeout) as resp:
            raw = resp.read().decode("utf-8")
            elapsed_ms = (time.perf_counter() - start) * 1000.0
            parsed = json.loads(raw) if raw else None
            return resp.status, parsed, elapsed_ms
    except error.HTTPError as e:
        elapsed_ms = (time.perf_counter() - start) * 1000.0
        try:
            raw = e.read().decode("utf-8")
            parsed = json.loads(raw) if raw else None
        except Exception:
            parsed = None
        return e.code, parsed, elapsed_ms
    except Exception:
        elapsed_ms = (time.perf_counter() - start) * 1000.0
        raise


def fetch_health(base_url: str) -> dict[str, Any]:
    _, data, _ = http_json("GET", f"{base_url}/health")
    if data is None:
        raise RuntimeError("empty health response")
    return data


def enqueue_player(
    base_url: str,
    player_id: str,
    skill: float,
    region: str,
    timeout: float,
) -> EnqueueResult:
    body = {
        "player_id": player_id,
        "skill": skill,
        "region": region,
        "role": "flex",
    }
    start = time.perf_counter()
    try:
        status, _, _ = http_json(
            "POST",
            f"{base_url}/queue",
            body=body,
            timeout=timeout,
        )
        latency_ms = (time.perf_counter() - start) * 1000.0
        ok = 200 <= status < 300
        return EnqueueResult(ok=ok, latency_ms=latency_ms, status=status)
    except Exception as exc:
        latency_ms = (time.perf_counter() - start) * 1000.0
        return EnqueueResult(ok=False, latency_ms=latency_ms, error=str(exc))


def percentile(values: list[float], p: float) -> float:
    if not values:
        return 0.0
    sorted_v = sorted(values)
    idx = int(round((p / 100.0) * (len(sorted_v) - 1)))
    return sorted_v[idx]


def wait_for_service(base_url: str, retries: int = 30) -> None:
    for i in range(retries):
        try:
            fetch_health(base_url)
            return
        except Exception:
            time.sleep(0.5)
    print(f"ERROR: service not reachable at {base_url}", file=sys.stderr)
    sys.exit(1)


def main() -> None:
    parser = argparse.ArgumentParser(description="Matchmaker load simulation")
    parser.add_argument(
        "--base-url",
        default=DEFAULT_BASE_URL,
        help=f"Matchmaker base URL (default: {DEFAULT_BASE_URL})",
    )
    parser.add_argument(
        "--players",
        type=int,
        default=5000,
        help="Total enqueue requests to send (default: 5000)",
    )
    parser.add_argument(
        "--concurrency",
        type=int,
        default=250,
        help="Max in-flight HTTP requests (default: 250)",
    )
    parser.add_argument(
        "--skill-base",
        type=float,
        default=1500.0,
        help="Center of skill distribution (default: 1500)",
    )
    parser.add_argument(
        "--skill-spread",
        type=float,
        default=80.0,
        help="Max deviation from skill-base per player (default: 80)",
    )
    parser.add_argument(
        "--settle-secs",
        type=float,
        default=15.0,
        help="Seconds to wait after enqueue for workers to match (default: 15)",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=15.0,
        help="Per-request HTTP timeout seconds (default: 15)",
    )
    parser.add_argument(
        "--skip-wait",
        action="store_true",
        help="Skip health polling settle phase",
    )
    args = parser.parse_args()

    base_url = args.base_url.rstrip("/")
    print(f"Target: {base_url}")
    wait_for_service(base_url)

    health_before = fetch_health(base_url)
    matches_before = health_before.get("matches_formed_total", 0)

    print(
        f"\n=== Phase 1: Enqueue {args.players} players "
        f"(concurrency={args.concurrency}) ==="
    )

    tasks: list[tuple[str, float, str]] = []
    for i in range(args.players):
        pid = str(uuid.uuid4())
        # Spread skills in a tight band so matches form quickly under load.
        offset = ((i * 17) % int(args.skill_spread * 2)) - args.skill_spread
        skill = args.skill_base + float(offset)
        region = REGIONS[i % len(REGIONS)]
        tasks.append((pid, skill, region))

    latencies: list[float] = []
    ok_count = 0
    err_count = 0
    status_counts: dict[int, int] = {}

    phase_start = time.perf_counter()
    with ThreadPoolExecutor(max_workers=args.concurrency) as pool:
        futures = [
            pool.submit(enqueue_player, base_url, pid, skill, region, args.timeout)
            for pid, skill, region in tasks
        ]
        done = 0
        for fut in as_completed(futures):
            result = fut.result()
            latencies.append(result.latency_ms)
            if result.ok:
                ok_count += 1
            else:
                err_count += 1
                if result.status is not None:
                    status_counts[result.status] = status_counts.get(result.status, 0) + 1
            done += 1
            if done % 500 == 0 or done == args.players:
                print(f"  progress: {done}/{args.players} requests completed")

    enqueue_elapsed = time.perf_counter() - phase_start
    enqueue_rps = args.players / enqueue_elapsed if enqueue_elapsed > 0 else 0.0

    health_mid = fetch_health(base_url)
    queue_depth_mid = health_mid.get("queue_depth", 0)

    if not args.skip_wait and args.settle_secs > 0:
        print(f"\n=== Phase 2: Settle {args.settle_secs}s (workers forming matches) ===")
        time.sleep(args.settle_secs)

    health_after = fetch_health(base_url)
    matches_after = health_after.get("matches_formed_total", 0)
    matches_formed = matches_after - matches_before
    expected_matches = args.players // 10

    print("\n=== Enqueue latency ===")
    print(f"  successful:     {ok_count}")
    print(f"  failed:         {err_count}")
    if status_counts:
        print(f"  status codes:   {status_counts}")
    print(f"  elapsed:        {enqueue_elapsed:.2f}s")
    print(f"  throughput:     {enqueue_rps:.1f} req/s")
    if latencies:
        print(f"  mean:           {statistics.mean(latencies):.2f} ms")
        print(f"  p50:            {percentile(latencies, 50):.2f} ms")
        print(f"  p95:            {percentile(latencies, 95):.2f} ms")
        print(f"  p99:            {percentile(latencies, 99):.2f} ms")
        print(f"  max:            {max(latencies):.2f} ms")

    print("\n=== Matchmaker metrics (from /health) ===")
    print(f"  queue_depth (after enqueue): {queue_depth_mid}")
    print(f"  queue_depth (final):         {health_after.get('queue_depth', 0)}")
    print(f"  matches_formed (delta):      {matches_formed}")
    print(f"  expected (~players/10):      {expected_matches}")
    print(f"  scans_total:                 {health_after.get('scans_total', 0)}")
    print(f"  evictions_total:             {health_after.get('evictions_total', 0)}")
    print(f"  avg_match_quality:           {health_after.get('avg_match_quality', 0):.3f}")
    print(f"  p99_scan_micros (engine):    {health_after.get('p99_scan_micros', 0)}")

    print("\n=== Summary ===")
    if ok_count == args.players and matches_formed >= expected_matches * 0.9:
        print("  PASS: High enqueue success rate and matches formed as expected.")
    elif ok_count >= args.players * 0.99:
        print(
            "  PARTIAL: Enqueues succeeded; match count may lag if settle time "
            "was too short — increase --settle-secs."
        )
    else:
        print("  FAIL: Significant enqueue errors — check service logs and capacity.")
        sys.exit(1)


if __name__ == "__main__":
    main()
