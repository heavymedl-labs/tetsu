#!/usr/bin/env bash
# Runs benchmark suites once and writes JSON records, one per line, to $OUT.
# Options in each suite run in a shuffled order so no option always goes first.
#
# Environment:
#   SUITES        comma-separated: db,tpl,http,rt,e2e (default: all)
#   BENCH_DB_URL  app role URL (default postgres://app@localhost:5432/tetsu)
#   ADMIN_URL     superuser URL; when set, the database is created and seeded
#   SCALE         multiplies database operation counts (default 1)
#   SECONDS_PER   seconds of load per HTTP measurement (default 8)
#   OUT           output file (default results/run.jsonl)
set -euo pipefail

BIN="${BIN:-tetsu-bench}"
SUITES="${SUITES:-db,tpl,http,rt,e2e}"
SCALE="${SCALE:-1}"
SECONDS_PER="${SECONDS_PER:-8}"
OUT="${OUT:-results/run.jsonl}"
mkdir -p "$(dirname "$OUT")"
: >"$OUT"

# Split cores: servers on the first half, the load generator on the rest.
cores=$(nproc)
half=$((cores / 2 > 0 ? cores / 2 : 1))
server_cpus="0-$((half - 1))"
load_cpus="$half-$((cores - 1))"
[[ $cores -lt 2 ]] && load_cpus=0

log() { echo "[$(date +%T)] $*" >&2; }
has() { [[ ",$SUITES," == *",$1,"* ]]; }

wait_port() {
  for _ in $(seq 100); do
    (echo >"/dev/tcp/127.0.0.1/$1") 2>/dev/null && return 0
    sleep 0.1
  done
  return 1
}

# CPU ticks used so far by the running server.
server_ticks() {
  local f
  read -ra f <"/proc/$server_pid/stat"
  echo $((f[13] + f[14]))
}

# Loads a URL with oha and prints one normalized record. Extra arguments go to
# oha, such as --rand-regex-url. Server CPU shows whether the server or the load
# generator set the throughput, and stays comparable when the generator did.
load() {
  local suite=$1 case=$2 variant=$3 url=$4 conns=$5
  shift 5
  taskset -c "$load_cpus" oha -c "$conns" -z 2s --no-tui "$@" "$url" >/dev/null 2>&1 || true
  local before result
  before=$(server_ticks)
  result=$(taskset -c "$load_cpus" oha -c "$conns" -z "${SECONDS_PER}s" --no-tui --output-format json "$@" "$url")
  jq -c --arg s "$suite" --arg c "$case" --arg v "$variant" \
    --argjson ticks $(($(server_ticks) - before)) --argjson hz "$(getconf CLK_TCK)" --argjson secs "$SECONDS_PER" '{
      suite: $s, case: $c, variant: $v,
      metrics: {
        ops_per_s: .summary.requestsPerSec,
        cpu_us_per_op: ($ticks / $hz * 1e6 / (.summary.requestsPerSec * $secs)),
        server_cores_busy: ($ticks / $hz / $secs),
        p50_us: (.latencyPercentiles.p50 * 1e6),
        p99_us: (.latencyPercentiles.p99 * 1e6),
        p999_us: (.latencyPercentiles["p99.9"] * 1e6),
        errors: ([.errorDistribution // {} | to_entries[] | select(.key != "aborted due to deadline") | .value] | add // 0)
      }
    }' <<<"$result" >>"$OUT"
}

# Starts a server pinned to the server cores, runs a function, then stops it.
with_server() {
  local port=$1
  shift
  local args=()
  while [[ $1 != -- ]]; do
    args+=("$1")
    shift
  done
  shift
  taskset -c "$server_cpus" "$BIN" "${args[@]}" >/dev/null 2>&1 &
  server_pid=$!
  if wait_port "$port"; then "$@"; else log "server ${args[*]} did not start"; fi
  kill "$server_pid" 2>/dev/null || true
  wait "$server_pid" 2>/dev/null || true
  sleep 1
}

if [[ -n "${ADMIN_URL:-}" ]] && { has db || has e2e; }; then
  log "seeding database"
  "$BIN" setup "$ADMIN_URL"
fi

if has db; then
  for work in lookup list book; do
    ops=$((20000 * SCALE))
    [[ $work == book ]] && ops=$((10000 * SCALE))
    for v in $(shuf -e sqlx tokio-pg tokio-pg-pipe cornucopia cornucopia-prepared cornucopia-prepared-pipe diesel); do
      log "db $work $v"
      "$BIN" db "$v" "$work" "$ops" 64 16 >>"$OUT"
    done
  done
fi

if has tpl; then
  log "tpl"
  "$BIN" tpl 100000 >>"$OUT"
fi

if has http; then
  for v in $(shuf -e axum axum-mw axum-vec-json actix salvo hyper); do
    log "http $v"
    run_http() {
      load http plain "$v" http://127.0.0.1:38080/plain 128
      load http json "$v" http://127.0.0.1:38080/json 128
    }
    with_server 38080 http "$v" "$half" 38080 -- run_http
  done
fi

if has rt; then
  for v in $(shuf -e tokio-ct tokio-mt monoio compio); do
    log "rt $v"
    run_rt() { load rt keepalive "$v" http://127.0.0.1:38081/ 64; }
    # Pin to one core to compare per-core efficiency.
    server_cpus=0 with_server 38081 rt "$v" 38081 -- run_rt
  done
fi

if has e2e; then
  for v in $(shuf -e sqlx tokio-pg tokio-pg-pipe); do
    log "e2e $v"
    # Random tenant and start time per request. The host has no dots because
    # oha treats the whole URL as a regular expression.
    run_e2e() { load e2e schedule "$v" 'http://localhost:38082/schedule\?t=[1-9][0-9]?&h=[1-9][0-9]?' 64 --rand-regex-url; }
    with_server 38082 e2e "$v" "$half" 38082 -- run_e2e
  done
fi

log "wrote $(wc -l <"$OUT") records to $OUT"
