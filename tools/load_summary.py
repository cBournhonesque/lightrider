#!/usr/bin/env python3
"""Summarize Lightrider load-test CSV and JSONL traces."""

from __future__ import annotations

import csv
import json
import math
import statistics
import sys
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any


def parse_float(value: str | None) -> float | None:
    if value is None or value == "":
        return None
    try:
        parsed = float(value)
    except ValueError:
        return None
    if math.isnan(parsed) or math.isinf(parsed):
        return None
    return parsed


def read_csv(path: Path) -> list[dict[str, str]]:
    if not path.exists():
        return []
    with path.open(newline="") as handle:
        return list(csv.DictReader(handle))


def fmt_mb(kb: float) -> str:
    return f"{kb / 1024.0:.1f} MiB"


def fmt_mbps(bytes_per_sec: float) -> str:
    return f"{bytes_per_sec * 8.0 / 1_000_000.0:.3f} Mbps"


def fmt_kib(byte_count: float) -> str:
    return f"{byte_count / 1024.0:.1f} KiB"


def summarize_processes(rows: list[dict[str, str]]) -> list[str]:
    if not rows:
        return ["process_metrics.csv not found or empty."]

    by_process: dict[tuple[str, str, str], list[dict[str, str]]] = defaultdict(list)
    role_by_ts: dict[tuple[str, str], dict[str, float]] = defaultdict(lambda: {"cpu": 0.0, "rss": 0.0, "alive": 0.0})
    for row in rows:
        if row.get("alive") != "1":
            continue
        key = (row.get("role", ""), row.get("name", ""), row.get("pid", ""))
        by_process[key].append(row)
        ts = row.get("timestamp_ns", "")
        role = row.get("role", "")
        cpu = parse_float(row.get("cpu_percent")) or 0.0
        rss = parse_float(row.get("rss_kb")) or 0.0
        role_by_ts[(ts, role)]["cpu"] += cpu
        role_by_ts[(ts, role)]["rss"] += rss
        role_by_ts[(ts, role)]["alive"] += 1.0

    lines = ["Process CPU/memory:"]
    for (role, name, pid), proc_rows in sorted(by_process.items(), key=lambda item: (item[0][0], item[0][1])):
        cpus = [value for row in proc_rows if (value := parse_float(row.get("cpu_percent"))) is not None]
        rss_values = [value for row in proc_rows if (value := parse_float(row.get("rss_kb"))) is not None]
        hwm_values = [value for row in proc_rows if (value := parse_float(row.get("hwm_kb"))) is not None]
        avg_cpu = statistics.fmean(cpus) if cpus else 0.0
        max_cpu = max(cpus) if cpus else 0.0
        max_rss = max(rss_values) if rss_values else 0.0
        max_hwm = max(hwm_values) if hwm_values else 0.0
        lines.append(
            f"  {role}/{name} pid={pid}: avg_cpu={avg_cpu:.1f}% "
            f"max_cpu={max_cpu:.1f}% max_rss={fmt_mb(max_rss)} max_hwm={fmt_mb(max_hwm)}"
        )

    role_samples: dict[str, list[dict[str, float]]] = defaultdict(list)
    for (_ts, role), sample in role_by_ts.items():
        role_samples[role].append(sample)
    lines.append("Role aggregate peaks:")
    for role, samples in sorted(role_samples.items()):
        max_cpu = max(sample["cpu"] for sample in samples)
        max_rss = max(sample["rss"] for sample in samples)
        max_alive = max(sample["alive"] for sample in samples)
        lines.append(
            f"  {role}: peak_cpu={max_cpu:.1f}% peak_rss={fmt_mb(max_rss)} "
            f"peak_alive_processes={int(max_alive)}"
        )
    return lines


def summarize_network(rows: list[dict[str, str]]) -> list[str]:
    if not rows:
        return ["network_metrics.csv not found or empty."]

    by_iface: dict[str, list[dict[str, str]]] = defaultdict(list)
    for row in rows:
        by_iface[row.get("interface", "")].append(row)

    lines = ["Network bandwidth by interface:"]
    for iface, iface_rows in sorted(by_iface.items()):
        iface_rows.sort(key=lambda row: row.get("timestamp_ns", ""))
        first = iface_rows[0]
        last = iface_rows[-1]
        first_rx = parse_float(first.get("rx_bytes")) or 0.0
        first_tx = parse_float(first.get("tx_bytes")) or 0.0
        last_rx = parse_float(last.get("rx_bytes")) or first_rx
        last_tx = parse_float(last.get("tx_bytes")) or first_tx
        rx_rates = [value for row in iface_rows if (value := parse_float(row.get("rx_bytes_per_sec"))) is not None]
        tx_rates = [value for row in iface_rows if (value := parse_float(row.get("tx_bytes_per_sec"))) is not None]
        max_rx = max(rx_rates) if rx_rates else 0.0
        max_tx = max(tx_rates) if tx_rates else 0.0
        avg_rx = statistics.fmean(rx_rates) if rx_rates else 0.0
        avg_tx = statistics.fmean(tx_rates) if tx_rates else 0.0
        lines.append(
            f"  {iface}: rx_total={(last_rx - first_rx) / 1024.0:.1f} KiB "
            f"tx_total={(last_tx - first_tx) / 1024.0:.1f} KiB "
            f"avg_rx={fmt_mbps(avg_rx)} avg_tx={fmt_mbps(avg_tx)} "
            f"peak_rx={fmt_mbps(max_rx)} peak_tx={fmt_mbps(max_tx)}"
        )
    return lines


def field_number(event: dict[str, Any], name: str) -> float | None:
    value = event.get(name)
    if value is None:
        value = event.get("fields", {}).get(name)
    if isinstance(value, (int, float)):
        return float(value)
    if isinstance(value, str):
        return parse_float(value)
    return None


def summarize_traces(run_dir: Path) -> list[str]:
    ndjson_paths = sorted(run_dir.rglob("*.ndjson"))
    if not ndjson_paths:
        return ["No JSONL Lightyear debug traces found."]

    perf_by_process: dict[tuple[str, str], list[dict[str, float]]] = defaultdict(list)
    role_by_process: dict[str, str] = {}
    trace_window_by_process: dict[str, list[int]] = {}
    transport_counts: Counter[tuple[str, str]] = Counter()
    transport_bytes: Counter[tuple[str, str]] = Counter()
    transport_send_bytes: Counter[str] = Counter()
    transport_recv_bytes: Counter[str] = Counter()
    fragment_counts: Counter[tuple[str, str]] = Counter()
    fragment_frames: Counter[tuple[str, str, int]] = Counter()
    rollback_count = 0
    rollback_delta_total = 0.0
    rollback_delta_max = 0.0

    for path in ndjson_paths:
        with path.open(errors="replace") as handle:
            for line in handle:
                try:
                    event = json.loads(line)
                except json.JSONDecodeError:
                    continue
                kind = str(event.get("kind", ""))
                target = str(event.get("target", ""))
                role = str(event.get("role") or event.get("fields", {}).get("role") or "")
                process_id = str(event.get("process_id", ""))
                if role and process_id:
                    role_by_process.setdefault(process_id, role)
                if kind == "perf_frame":
                    perf_by_process[(role, process_id)].append(
                        {
                            "fps": field_number(event, "fps") or 0.0,
                            "avg_ms": field_number(event, "frame_delta_avg_ms") or 0.0,
                            "max_ms": field_number(event, "frame_delta_max_ms") or 0.0,
                            "links": field_number(event, "link_count") or 0.0,
                            "rtt": field_number(event, "link_rtt_avg_ms") or 0.0,
                            "jitter": field_number(event, "link_jitter_avg_ms") or 0.0,
                            "recv_buffered": field_number(event, "link_recv_buffered") or 0.0,
                            "send_buffered": field_number(event, "link_send_buffered") or 0.0,
                        }
                    )
                if target == "lightyear_debug::transport":
                    timestamp = event.get("timestamp")
                    if isinstance(timestamp, int):
                        window = trace_window_by_process.setdefault(process_id, [timestamp, timestamp])
                        window[0] = min(window[0], timestamp)
                        window[1] = max(window[1], timestamp)
                    transport_counts[(process_id, kind)] += 1
                    byte_count = field_number(event, "bytes") or field_number(event, "send_bytes") or 0.0
                    transport_bytes[(process_id, kind)] += int(byte_count)
                    if kind == "packet_send":
                        transport_send_bytes[process_id] += int(byte_count)
                    elif kind == "packet_recv":
                        transport_recv_bytes[process_id] += int(byte_count)
                    packet_type = str(event.get("fields", {}).get("packet_type", ""))
                    if "fragment" in kind.lower() or "DataFragment" in packet_type:
                        fragment_counts[(process_id, kind)] += 1
                        frame = int(
                            field_number(event, "frame_index")
                            or field_number(event, "local_tick")
                            or field_number(event, "remote_tick")
                            or field_number(event, "tick_id")
                            or -1
                        )
                        fragment_frames[(process_id, kind, frame)] += 1
                if target == "lightyear_debug::prediction" and kind == "rollback_requested":
                    rollback_count += 1
                    delta = field_number(event, "rollback_delta") or 0.0
                    rollback_delta_total += delta
                    rollback_delta_max = max(rollback_delta_max, delta)

    lines = ["Trace frame/link metrics:"]
    if perf_by_process:
        for (role, process_id), samples in sorted(perf_by_process.items()):
            lines.append(
                f"  {role or 'unknown'} pid={process_id}: "
                f"avg_frame={statistics.fmean(sample['avg_ms'] for sample in samples):.2f} ms "
                f"peak_frame={max(sample['max_ms'] for sample in samples):.2f} ms "
                f"avg_fps={statistics.fmean(sample['fps'] for sample in samples):.1f} "
                f"avg_links={statistics.fmean(sample['links'] for sample in samples):.1f} "
                f"avg_rtt={statistics.fmean(sample['rtt'] for sample in samples):.2f} ms "
                f"avg_jitter={statistics.fmean(sample['jitter'] for sample in samples):.2f} ms "
                f"peak_recv_buffered={max(sample['recv_buffered'] for sample in samples):.0f} "
                f"peak_send_buffered={max(sample['send_buffered'] for sample in samples):.0f}"
            )
    else:
        lines.append("  No perf_frame rows found; ensure LIGHTYEAR_DEBUG_FILE is set for at least the server.")

    lines.append("Trace transport metrics:")
    if transport_counts:
        for (process_id, kind), count in sorted(transport_counts.items()):
            lines.append(
                f"  pid={process_id} {kind}: rows={count} bytes={transport_bytes[(process_id, kind)]}"
            )
    else:
        lines.append("  No lightyear_debug::transport rows found.")

    lines.append("Trace directional packet bandwidth:")
    process_ids = sorted(set(transport_send_bytes) | set(transport_recv_bytes))
    if process_ids:
        for process_id in process_ids:
            window = trace_window_by_process.get(process_id)
            elapsed = 0.0
            if window is not None and window[1] > window[0]:
                elapsed = (window[1] - window[0]) / 1_000_000_000.0
            sent = transport_send_bytes[process_id]
            received = transport_recv_bytes[process_id]
            sent_rate = sent / elapsed if elapsed > 0.0 else 0.0
            received_rate = received / elapsed if elapsed > 0.0 else 0.0
            role = role_by_process.get(process_id, "unknown")
            lines.append(
                f"  {role} pid={process_id}: sent={fmt_kib(sent)} recv={fmt_kib(received)} "
                f"avg_send={fmt_mbps(sent_rate)} avg_recv={fmt_mbps(received_rate)}"
            )
    else:
        lines.append("  No packet_send/packet_recv rows found.")

    lines.append("Packet fragment metrics from trace rows:")
    if fragment_counts:
        for (process_id, kind), count in sorted(fragment_counts.items()):
            max_per_frame = max(
                value
                for (pid, fragment_kind, _frame), value in fragment_frames.items()
                if pid == process_id and fragment_kind == kind
            )
            lines.append(
                f"  pid={process_id} {kind}: fragments={count} max_fragments_per_frame={max_per_frame}"
            )
    else:
        lines.append("  No packet fragment rows found in traced processes.")

    if rollback_count:
        lines.append(
            f"Prediction rollbacks: count={rollback_count} "
            f"avg_delta={rollback_delta_total / rollback_count:.2f} max_delta={rollback_delta_max:.0f}"
        )
    else:
        lines.append("Prediction rollbacks: none observed in traced processes.")

    return lines


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: load_summary.py <run-dir>", file=sys.stderr)
        return 2
    run_dir = Path(sys.argv[1])
    if not run_dir.exists():
        print(f"load summary: {run_dir} does not exist", file=sys.stderr)
        return 1

    lines: list[str] = [f"Lightrider load summary: {run_dir}", ""]
    lines.extend(summarize_processes(read_csv(run_dir / "process_metrics.csv")))
    lines.append("")
    lines.extend(summarize_network(read_csv(run_dir / "network_metrics.csv")))
    lines.append("")
    lines.extend(summarize_traces(run_dir))
    output = "\n".join(lines)
    print(output)
    (run_dir / "load_summary.txt").write_text(output + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
