#!/usr/bin/env python3
import argparse
import json
import math
import re
import sys
from collections import defaultdict
from pathlib import Path


LOG_FAILURE_PATTERNS = [
    re.compile(pattern, re.IGNORECASE)
    for pattern in [
        r"unable to apply mutate message",
        r"without a confirmed base",
        r"received diff patches",
        r"panicked at",
        r"thread '.*' panicked",
        r"Option::unwrap\(\) on a None value",
    ]
]


def parse_args():
    parser = argparse.ArgumentParser(
        description="Validate a lightrider interpolation stress trace."
    )
    parser.add_argument("run_dir", type=Path)
    parser.add_argument("--max-interpolated-step-per-tick", type=float, default=64.0)
    parser.add_argument("--sample-schedule", default="Last")
    parser.add_argument("--min-interpolated-entities", type=int, default=2)
    parser.add_argument("--min-interpolated-samples", type=int, default=60)
    parser.add_argument("--min-direction-changes", type=int, default=4)
    return parser.parse_args()


def fail(message, details=None):
    print(f"interpolation stress check failed: {message}", file=sys.stderr)
    if details:
        for detail in details[:20]:
            print(f"  {detail}", file=sys.stderr)
    return 1


def scan_logs(run_dir):
    failures = []
    for path in sorted(run_dir.glob("*.log")):
        with path.open(errors="replace") as file:
            for line_number, line in enumerate(file, 1):
                if any(pattern.search(line) for pattern in LOG_FAILURE_PATTERNS):
                    failures.append(f"{path}:{line_number}: {line.strip()}")
    return failures


def iter_ndjson(run_dir):
    for path in sorted(run_dir.glob("*.ndjson")):
        with path.open(errors="replace") as file:
            for line_number, line in enumerate(file, 1):
                line = line.strip()
                if not line:
                    continue
                try:
                    yield path, line_number, json.loads(line)
                except json.JSONDecodeError as error:
                    yield path, line_number, {
                        "kind": "__json_error__",
                        "error": str(error),
                    }


def as_float(value):
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def as_bool(value):
    if isinstance(value, bool):
        return value
    if isinstance(value, str):
        return value.lower() == "true"
    return False


def collect_trace_stats(run_dir, sample_schedule):
    bad_events = []
    interpolated_samples = defaultdict(list)
    counts = defaultdict(int)
    json_errors = []

    for path, line_number, row in iter_ndjson(run_dir):
        kind = row.get("kind")
        counts[kind] += 1
        if kind == "__json_error__":
            json_errors.append(f"{path}:{line_number}: {row['error']}")
            continue
        if kind in {"snake_invariant_violation", "remote_snake_interpolation_diagonal"}:
            bad_events.append(f"{path}:{line_number}: kind={kind} row={row}")
            continue
        if kind != "snake_head":
            continue

        fields = row.get("fields") or {}
        if row.get("role") != "client":
            continue
        if row.get("schedule") != sample_schedule:
            continue
        if not as_bool(fields.get("is_interpolated")):
            continue

        tick = row.get("tick_id")
        x = as_float(fields.get("head_x"))
        y = as_float(fields.get("head_y"))
        if tick is None or x is None or y is None:
            continue
        key = (
            path.name,
            row.get("entity"),
            fields.get("player_id_bits"),
        )
        interpolated_samples[key].append(
            {
                "tick": int(tick),
                "x": x,
                "y": y,
                "direction": fields.get("direction"),
                "speed": as_float(fields.get("speed")),
                "tail_points": fields.get("tail_points"),
            }
        )

    return counts, json_errors, bad_events, interpolated_samples


def check_interpolated_motion(samples_by_entity, max_step_per_tick):
    jumps = []
    total_samples = 0
    direction_changes = 0
    active_entities = 0

    for key, samples in samples_by_entity.items():
        samples.sort(key=lambda sample: sample["tick"])
        total_samples += len(samples)
        if len(samples) >= 2:
            active_entities += 1
        previous = None
        for sample in samples:
            if previous is None:
                previous = sample
                continue
            tick_delta = sample["tick"] - previous["tick"]
            if tick_delta <= 0:
                previous = sample
                continue
            dx = sample["x"] - previous["x"]
            dy = sample["y"] - previous["y"]
            distance = math.hypot(dx, dy)
            limit = max_step_per_tick * tick_delta
            if distance > limit:
                jumps.append(
                    f"{key}: tick {previous['tick']}->{sample['tick']} "
                    f"distance={distance:.3f} limit={limit:.3f} "
                    f"from=({previous['x']:.3f},{previous['y']:.3f},{previous['direction']}) "
                    f"to=({sample['x']:.3f},{sample['y']:.3f},{sample['direction']})"
                )
            if (
                previous.get("direction") is not None
                and sample.get("direction") is not None
                and previous["direction"] != sample["direction"]
            ):
                direction_changes += 1
            previous = sample

    return active_entities, total_samples, direction_changes, jumps


def main():
    args = parse_args()
    if not args.run_dir.exists():
        return fail(f"run directory does not exist: {args.run_dir}")

    log_failures = scan_logs(args.run_dir)
    counts, json_errors, bad_events, samples = collect_trace_stats(
        args.run_dir, args.sample_schedule
    )
    active_entities, total_samples, direction_changes, jumps = check_interpolated_motion(
        samples, args.max_interpolated_step_per_tick
    )

    if log_failures:
        return fail("runtime logs contain known networking/interpolation errors", log_failures)
    if json_errors:
        return fail("debug trace contains invalid NDJSON", json_errors)
    if bad_events:
        return fail("debug trace contains snake invariant/interpolation failures", bad_events)
    if active_entities < args.min_interpolated_entities:
        return fail(
            f"only {active_entities} interpolated remote entities had samples; "
            f"expected at least {args.min_interpolated_entities}"
        )
    if total_samples < args.min_interpolated_samples:
        return fail(
            f"only {total_samples} interpolated {args.sample_schedule} samples; "
            f"expected at least {args.min_interpolated_samples}"
        )
    if direction_changes < args.min_direction_changes:
        return fail(
            f"only {direction_changes} interpolated direction changes; "
            f"expected at least {args.min_direction_changes}"
        )
    if jumps:
        return fail("interpolated remote snakes had implausible head jumps", jumps)

    print(
        "interpolation stress check passed: "
        f"entities={active_entities} samples={total_samples} "
        f"direction_changes={direction_changes} "
        f"snake_invariant_violations={counts['snake_invariant_violation']} "
        f"diagonal_interpolation_events={counts['remote_snake_interpolation_diagonal']}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
