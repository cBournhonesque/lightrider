#!/usr/bin/env bash
set -euo pipefail

run_dir="${1:?usage: load_sampler.sh <run-dir> <pids.csv> [interval-seconds]}"
pids_file="${2:?usage: load_sampler.sh <run-dir> <pids.csv> [interval-seconds]}"
interval_seconds="${3:-1}"
interface_filter_csv="${LOAD_NET_INTERFACES:-}"
interface_filter_csv="${interface_filter_csv// /}"

if [[ ! -r /proc/stat || ! -r /proc/net/dev ]]; then
  echo "load_sampler: /proc is required; this sampler currently supports Linux only" >&2
  exit 1
fi

mkdir -p "$run_dir"
process_csv="$run_dir/process_metrics.csv"
network_csv="$run_dir/network_metrics.csv"
log_csv="$run_dir/log_metrics.csv"

if [[ ! -f "$process_csv" ]]; then
  echo "timestamp_ns,role,name,pid,alive,cpu_percent,rss_kb,hwm_kb,vsz_kb,threads,fd_count" > "$process_csv"
fi
if [[ ! -f "$network_csv" ]]; then
  echo "timestamp_ns,interface,rx_bytes,tx_bytes,rx_bytes_per_sec,tx_bytes_per_sec,rx_bits_per_sec,tx_bits_per_sec" > "$network_csv"
fi
if [[ ! -f "$log_csv" ]]; then
  echo "timestamp_ns,path,bytes" > "$log_csv"
fi

num_cpus="$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 1)"
declare -A prev_proc_ticks=()
declare -A prev_proc_total_ticks=()
declare -A prev_iface_rx=()
declare -A prev_iface_tx=()
declare -A prev_iface_ts=()
sample_index=0

total_cpu_ticks() {
  awk '/^cpu / { total = 0; for (i = 2; i <= NF; i++) total += $i; print total; exit }' /proc/stat
}

proc_cpu_ticks() {
  local pid="$1"
  [[ -r "/proc/$pid/stat" ]] || return 1
  awk '{ print $14 + $15 }' "/proc/$pid/stat"
}

proc_status_values() {
  local pid="$1"
  awk '
    /^VmRSS:/ { rss = $2 }
    /^VmHWM:/ { hwm = $2 }
    /^VmSize:/ { vsz = $2 }
    /^Threads:/ { threads = $2 }
    END {
      printf "%d,%d,%d,%d", rss + 0, hwm + 0, vsz + 0, threads + 0
    }
  ' "/proc/$pid/status" 2>/dev/null || printf "0,0,0,0"
}

fd_count() {
  local pid="$1"
  if [[ -d "/proc/$pid/fd" ]]; then
    find "/proc/$pid/fd" -mindepth 1 -maxdepth 1 2>/dev/null | wc -l | tr -d ' '
  else
    echo 0
  fi
}

while true; do
  ts_ns="$(date +%s%N)"
  total_ticks="$(total_cpu_ticks)"

  if [[ -f "$pids_file" ]]; then
    while IFS=, read -r role name pid; do
      [[ -n "${pid:-}" && "$role" != "role" ]] || continue
      if [[ -d "/proc/$pid" ]]; then
        alive=1
        cpu_percent=""
        if proc_ticks="$(proc_cpu_ticks "$pid")"; then
          prev_proc="${prev_proc_ticks[$pid]:-}"
          prev_total="${prev_proc_total_ticks[$pid]:-}"
          if [[ -n "$prev_proc" && -n "$prev_total" ]]; then
            proc_delta=$((proc_ticks - prev_proc))
            total_delta=$((total_ticks - prev_total))
            if (( total_delta > 0 && proc_delta >= 0 )); then
              cpu_percent="$(awk -v pd="$proc_delta" -v td="$total_delta" -v n="$num_cpus" 'BEGIN { printf "%.3f", 100.0 * pd * n / td }')"
            fi
          fi
          prev_proc_ticks[$pid]="$proc_ticks"
          prev_proc_total_ticks[$pid]="$total_ticks"
        fi
        status_values="$(proc_status_values "$pid")"
        fds="$(fd_count "$pid")"
        echo "$ts_ns,$role,$name,$pid,$alive,$cpu_percent,$status_values,$fds" >> "$process_csv"
      else
        echo "$ts_ns,$role,$name,$pid,0,,,,,," >> "$process_csv"
      fi
  done < "$pids_file"
  fi

  while read -r iface rx_bytes tx_bytes; do
    [[ -n "$iface" ]] || continue
    if [[ -n "$interface_filter_csv" ]]; then
      case ",$interface_filter_csv," in
        *,"$iface",*) ;;
        *) continue ;;
      esac
    fi
    key="$iface"
    rx_bps=""
    tx_bps=""
    rx_bits=""
    tx_bits=""
    prev_ts="${prev_iface_ts[$key]:-}"
    if [[ -n "$prev_ts" ]]; then
      elapsed="$(awk -v now="$ts_ns" -v prev="$prev_ts" 'BEGIN { printf "%.9f", (now - prev) / 1000000000.0 }')"
      if awk -v e="$elapsed" 'BEGIN { exit !(e > 0.0) }'; then
        rx_delta=$((rx_bytes - prev_iface_rx[$key]))
        tx_delta=$((tx_bytes - prev_iface_tx[$key]))
        if (( rx_delta >= 0 && tx_delta >= 0 )); then
          rx_bps="$(awk -v d="$rx_delta" -v e="$elapsed" 'BEGIN { printf "%.3f", d / e }')"
          tx_bps="$(awk -v d="$tx_delta" -v e="$elapsed" 'BEGIN { printf "%.3f", d / e }')"
          rx_bits="$(awk -v v="$rx_bps" 'BEGIN { printf "%.3f", v * 8.0 }')"
          tx_bits="$(awk -v v="$tx_bps" 'BEGIN { printf "%.3f", v * 8.0 }')"
        fi
      fi
    fi
    prev_iface_rx[$key]="$rx_bytes"
    prev_iface_tx[$key]="$tx_bytes"
    prev_iface_ts[$key]="$ts_ns"
    echo "$ts_ns,$iface,$rx_bytes,$tx_bytes,$rx_bps,$tx_bps,$rx_bits,$tx_bits" >> "$network_csv"
  done < <(awk 'NR > 2 { gsub(":", "", $1); print $1, $2, $10 }' /proc/net/dev)

  if (( sample_index % 5 == 0 )); then
    while IFS= read -r line; do
      [[ -n "$line" ]] || continue
      echo "$ts_ns,$line" >> "$log_csv"
    done < <(find "$run_dir" -maxdepth 3 -type f \( -name '*.log' -o -name '*.ndjson' -o -name '*.csv' \) -printf '%p,%s\n' 2>/dev/null)
  fi

  sample_index=$((sample_index + 1))
  sleep "$interval_seconds"
done
