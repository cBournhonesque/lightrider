#!/usr/bin/env python3
"""Summarize Bevy/tracing Chrome trace JSON into CSV and Markdown.

The trace is produced by Bevy's `trace_chrome` feature. Durations in Chrome
trace files are microseconds; this script reports milliseconds.
"""

from __future__ import annotations

import csv
import json
import math
import statistics
import sys
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Any


@dataclass(frozen=True)
class Span:
    name: str
    pid: str
    tid: str
    start_us: float
    dur_us: float

    @property
    def end_us(self) -> float:
        return self.start_us + self.dur_us


def as_float(value: Any) -> float | None:
    if isinstance(value, bool) or value is None:
        return None
    if isinstance(value, (int, float)):
        number = float(value)
    elif isinstance(value, str):
        try:
            number = float(value)
        except ValueError:
            return None
    else:
        return None
    if math.isnan(number) or math.isinf(number):
        return None
    return number


def percentile(values: list[float], percentile_value: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    index = min(
        len(ordered) - 1,
        max(0, math.ceil((percentile_value / 100.0) * len(ordered)) - 1),
    )
    return ordered[index]


def short_text(value: Any) -> str | None:
    if value is None:
        return None
    if isinstance(value, (str, int, float, bool)):
        text = str(value)
    else:
        return None
    text = " ".join(text.split())
    if not text or len(text) > 240:
        return None
    return text


def find_label_arg(value: Any) -> str | None:
    if not isinstance(value, dict):
        return None
    preferred_keys = (
        "system_name",
        "system",
        "name",
        "function",
        "target",
        "module_path",
    )
    for key in preferred_keys:
        if key in value and (text := short_text(value[key])):
            return text
    for key, nested in value.items():
        key_lower = str(key).lower()
        if any(fragment in key_lower for fragment in ("system", "name", "function")):
            if text := short_text(nested):
                return text
        if isinstance(nested, dict) and (text := find_label_arg(nested)):
            return text
    return None


def is_generic_name(name: str) -> bool:
    lowered = name.lower()
    return lowered in {"system", "run_system", "system_span", "schedule"} or lowered.startswith(
        ("system ", "system{", "system:")
    )


def span_name(event: dict[str, Any]) -> str:
    name = short_text(event.get("name")) or "<unnamed>"
    args = event.get("args")
    detail = find_label_arg(args) if isinstance(args, dict) else None
    if detail and detail not in name and (is_generic_name(name) or "system" in name.lower()):
        return f"{name}: {detail}"
    return name


def load_events(path: Path) -> list[dict[str, Any]]:
    with path.open("r", encoding="utf-8") as handle:
        document = json.load(handle)
    if isinstance(document, dict):
        events = document.get("traceEvents", [])
    else:
        events = document
    return [event for event in events if isinstance(event, dict)]


def read_spans(events: list[dict[str, Any]]) -> list[Span]:
    spans: list[Span] = []
    stacks: dict[tuple[str, str], list[tuple[str, float]]] = defaultdict(list)

    for event in events:
        phase = event.get("ph")
        pid = str(event.get("pid", ""))
        tid = str(event.get("tid", ""))
        timestamp = as_float(event.get("ts"))
        if timestamp is None:
            continue

        if phase == "X":
            duration = as_float(event.get("dur"))
            if duration is not None and duration > 0.0:
                spans.append(Span(span_name(event), pid, tid, timestamp, duration))
        elif phase == "B":
            stacks[(pid, tid)].append((span_name(event), timestamp))
        elif phase == "E":
            stack = stacks.get((pid, tid))
            if not stack:
                continue
            name, start = stack.pop()
            duration = timestamp - start
            if duration > 0.0:
                spans.append(Span(name, pid, tid, start, duration))

    return spans


def merge_intervals(intervals: list[tuple[float, float]]) -> list[tuple[float, float]]:
    if not intervals:
        return []
    ordered = sorted(intervals)
    merged = [ordered[0]]
    for start, end in ordered[1:]:
        current_start, current_end = merged[-1]
        if start <= current_end:
            merged[-1] = (current_start, max(current_end, end))
        else:
            merged.append((start, end))
    return merged


def peak_concurrency(thread_intervals: dict[tuple[str, str], list[tuple[float, float]]]) -> int:
    points: list[tuple[float, int]] = []
    for intervals in thread_intervals.values():
        for start, end in merge_intervals(intervals):
            points.append((start, 1))
            points.append((end, -1))
    active = 0
    peak = 0
    for _time, delta in sorted(points, key=lambda item: (item[0], item[1])):
        active += delta
        peak = max(peak, active)
    return peak


def write_system_profile(spans: list[Span], path: Path) -> None:
    durations_by_name: dict[str, list[float]] = defaultdict(list)
    threads_by_name: dict[str, set[str]] = defaultdict(set)
    for span in spans:
        durations_by_name[span.name].append(span.dur_us / 1000.0)
        threads_by_name[span.name].add(f"{span.pid}:{span.tid}")

    rows = []
    for name, durations in durations_by_name.items():
        rows.append(
            {
                "name": name,
                "count": len(durations),
                "total_ms": sum(durations),
                "avg_ms": statistics.fmean(durations),
                "p50_ms": percentile(durations, 50),
                "p95_ms": percentile(durations, 95),
                "max_ms": max(durations),
                "thread_count": len(threads_by_name[name]),
                "threads": " ".join(sorted(threads_by_name[name])),
            }
        )

    rows.sort(key=lambda row: (row["total_ms"], row["max_ms"]), reverse=True)
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(
            handle,
            fieldnames=[
                "name",
                "count",
                "total_ms",
                "avg_ms",
                "p50_ms",
                "p95_ms",
                "max_ms",
                "thread_count",
                "threads",
            ],
        )
        writer.writeheader()
        for row in rows:
            writer.writerow(row)


def write_thread_profile(spans: list[Span], path: Path) -> dict[tuple[str, str], float]:
    intervals_by_thread: dict[tuple[str, str], list[tuple[float, float]]] = defaultdict(list)
    top_span_by_thread: dict[tuple[str, str], dict[str, float]] = defaultdict(lambda: defaultdict(float))
    for span in spans:
        key = (span.pid, span.tid)
        intervals_by_thread[key].append((span.start_us, span.end_us))
        top_span_by_thread[key][span.name] += span.dur_us / 1000.0

    busy_ms_by_thread: dict[tuple[str, str], float] = {}
    rows = []
    for key, intervals in intervals_by_thread.items():
        busy_ms = sum(end - start for start, end in merge_intervals(intervals)) / 1000.0
        busy_ms_by_thread[key] = busy_ms
        top_name, top_ms = max(top_span_by_thread[key].items(), key=lambda item: item[1])
        rows.append(
            {
                "pid": key[0],
                "tid": key[1],
                "busy_ms": busy_ms,
                "span_count": len(intervals),
                "top_span": top_name,
                "top_span_total_ms": top_ms,
            }
        )

    rows.sort(key=lambda row: row["busy_ms"], reverse=True)
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(
            handle,
            fieldnames=[
                "pid",
                "tid",
                "busy_ms",
                "span_count",
                "top_span",
                "top_span_total_ms",
            ],
        )
        writer.writeheader()
        for row in rows:
            writer.writerow(row)
    return busy_ms_by_thread


def markdown_summary(
    trace_path: Path,
    out_dir: Path,
    events: list[dict[str, Any]],
    spans: list[Span],
    busy_ms_by_thread: dict[tuple[str, str], float],
) -> str:
    if not spans:
        return (
            "# Chrome Trace Summary\n\n"
            f"Trace: `{trace_path}`\n\n"
            "No complete spans were found. Check that the server was built with "
            "`--features profile-chrome` and exited gracefully.\n"
        )

    start_us = min(span.start_us for span in spans)
    end_us = max(span.end_us for span in spans)
    wall_ms = max(0.0, (end_us - start_us) / 1000.0)
    total_inclusive_ms = sum(span.dur_us for span in spans) / 1000.0
    busy_ms = sum(busy_ms_by_thread.values())
    intervals_by_thread: dict[tuple[str, str], list[tuple[float, float]]] = defaultdict(list)
    for span in spans:
        intervals_by_thread[(span.pid, span.tid)].append((span.start_us, span.end_us))

    durations_by_name: dict[str, list[float]] = defaultdict(list)
    for span in spans:
        durations_by_name[span.name].append(span.dur_us / 1000.0)

    top_total = sorted(
        durations_by_name.items(),
        key=lambda item: (sum(item[1]), max(item[1])),
        reverse=True,
    )[:15]
    top_p95 = sorted(
        durations_by_name.items(),
        key=lambda item: (percentile(item[1], 95), sum(item[1])),
        reverse=True,
    )[:15]
    top_threads = sorted(busy_ms_by_thread.items(), key=lambda item: item[1], reverse=True)[:10]

    lines = [
        "# Chrome Trace Summary",
        "",
        f"Trace: `{trace_path}`",
        f"Output: `{out_dir}`",
        "",
        "## Run",
        "",
        f"- Trace events: {len(events)}",
        f"- Complete spans: {len(spans)}",
        f"- Wall time: {wall_ms:.2f} ms",
        f"- Inclusive traced span time: {total_inclusive_ms:.2f} ms",
        f"- Merged thread busy time: {busy_ms:.2f} ms",
        f"- Estimated average parallelism: {(busy_ms / wall_ms) if wall_ms > 0 else 0.0:.2f}x",
        f"- Estimated peak busy threads: {peak_concurrency(intervals_by_thread)}",
        "",
        "Durations are inclusive tracing spans. Nested spans can double-count in the inclusive totals; use merged thread busy time for a better wall-clock parallelism estimate.",
        "",
        "## Top Spans By Total",
        "",
        "| span | count | total ms | avg ms | p95 ms | max ms |",
        "|---|---:|---:|---:|---:|---:|",
    ]
    for name, durations in top_total:
        lines.append(
            f"| `{name}` | {len(durations)} | {sum(durations):.2f} | "
            f"{statistics.fmean(durations):.3f} | {percentile(durations, 95):.3f} | {max(durations):.3f} |"
        )

    lines.extend(
        [
            "",
            "## Top Spans By P95",
            "",
            "| span | count | p95 ms | max ms | total ms |",
            "|---|---:|---:|---:|---:|",
        ]
    )
    for name, durations in top_p95:
        lines.append(
            f"| `{name}` | {len(durations)} | {percentile(durations, 95):.3f} | "
            f"{max(durations):.3f} | {sum(durations):.2f} |"
        )

    lines.extend(
        [
            "",
            "## Busiest Threads",
            "",
            "| pid:tid | busy ms |",
            "|---|---:|",
        ]
    )
    for (pid, tid), busy in top_threads:
        lines.append(f"| `{pid}:{tid}` | {busy:.2f} |")

    lines.extend(
        [
            "",
            "## Files",
            "",
            "- `system_profile.csv`: span aggregate table sorted by total inclusive time.",
            "- `thread_profile.csv`: merged busy time by process/thread.",
        ]
    )
    return "\n".join(lines) + "\n"


def main() -> int:
    if len(sys.argv) not in {2, 3}:
        print(
            "usage: chrome_trace_summary.py TRACE_JSON [OUT_DIR]",
            file=sys.stderr,
        )
        return 2

    trace_path = Path(sys.argv[1])
    out_dir = Path(sys.argv[2]) if len(sys.argv) == 3 else trace_path.parent / "chrome_profile"
    out_dir.mkdir(parents=True, exist_ok=True)

    events = load_events(trace_path)
    spans = read_spans(events)
    write_system_profile(spans, out_dir / "system_profile.csv")
    busy_ms_by_thread = write_thread_profile(spans, out_dir / "thread_profile.csv")
    summary = markdown_summary(trace_path, out_dir, events, spans, busy_ms_by_thread)
    (out_dir / "profile_summary.md").write_text(summary, encoding="utf-8")
    print(summary)
    return 0 if spans else 1


if __name__ == "__main__":
    raise SystemExit(main())
