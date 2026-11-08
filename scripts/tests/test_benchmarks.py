"""The benchmark harness scripts: the report aggregator and the run script."""

import importlib.util
import json
import math
import tempfile
import unittest
from pathlib import Path

from scripts.tests.support import ROOT, run, write

BENCHMARKS = ROOT / "benchmarks"
_spec = importlib.util.spec_from_file_location("aggregate", BENCHMARKS / "aggregate.py")
aggregate = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(aggregate)


def record(suite, case, variant, **metrics):
    return {"suite": suite, "case": case, "variant": variant, "metrics": metrics}


class Summarize(unittest.TestCase):
    def test_speed_is_relative_to_the_baseline_in_the_same_run(self):
        # The second runner is twice as fast overall, so raw numbers differ but the ratio does not.
        runs = [
            [record("http", "plain", "axum", ops_per_s=100), record("http", "plain", "hyper", ops_per_s=150)],
            [record("http", "plain", "axum", ops_per_s=200), record("http", "plain", "hyper", ops_per_s=300)],
        ]
        rows = {r["variant"]: r for r in aggregate.summarize(runs)}
        self.assertEqual(rows["hyper"]["speed_vs_baseline"]["mean"], 1.5)
        self.assertEqual(rows["hyper"]["speed_vs_baseline"]["stdev"], 0.0)
        self.assertEqual(rows["axum"]["speed_vs_baseline"]["mean"], 1.0)
        self.assertEqual(rows["hyper"]["metrics"]["ops_per_s"]["mean"], 225)

    def test_a_lower_is_better_metric_inverts_the_ratio(self):
        runs = [
            [
                record("tpl", "page", "askama", ns_per_render=100),
                record("tpl", "page", "slow", ns_per_render=200),
            ]
        ]
        rows = {r["variant"]: r for r in aggregate.summarize(runs)}
        self.assertEqual(rows["slow"]["speed_vs_baseline"]["mean"], 0.5)

    def test_unknown_suites_and_missing_baselines_are_handled(self):
        runs = [
            [
                record("other", "x", "a", ops_per_s=1),
                record("db", "lookup", "diesel", ops_per_s=5, note="text"),
            ]
        ]
        rows = aggregate.summarize(runs)
        self.assertEqual([r["suite"] for r in rows], ["db"])
        self.assertIsNone(rows[0]["speed_vs_baseline"])
        self.assertEqual(list(rows[0]["metrics"]), ["ops_per_s"])

    def test_one_run_has_no_confidence_interval(self):
        s = aggregate.stats([2.0])
        self.assertEqual((s["n"], s["stdev"]), (1, 0.0))
        self.assertTrue(math.isnan(s["ci95"]))
        self.assertEqual(aggregate.fmt(s["ci95"]), "n/a")

    def test_the_t_value_falls_back_to_the_nearest_smaller_table_entry(self):
        self.assertEqual(aggregate.t95(12), aggregate.T95[10])
        self.assertEqual(aggregate.t95(0), aggregate.T95[1])
        self.assertEqual(aggregate.t95(100), aggregate.T95[30])


class Report(unittest.TestCase):
    def test_the_report_lists_each_suite_with_the_fastest_option_first(self):
        runs = [
            [
                record(
                    "http", "plain", "axum", ops_per_s=100, p99_us=2000, cpu_us_per_op=10, server_cores_busy=1
                ),
                record("http", "plain", "hyper", ops_per_s=150),
            ]
        ]
        text = aggregate.report(aggregate.summarize(runs), runs, "Title")
        self.assertTrue(text.startswith("# Title\n"))
        self.assertIn("## HTTP framework", text)
        self.assertNotIn("## Postgres access", text)
        self.assertLess(text.index("| `hyper` |"), text.index("| `axum` |"))
        self.assertIn("| plain | `axum` | 1.00 ± n/a | 100 (0%) | 2.00 ms | 10 µs | 1.00 |", text)


class Main(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.dir = Path(tmp.name)

    def aggregate(self, *args):
        return run(["python3", BENCHMARKS / "aggregate.py", *args], self.dir)

    def test_too_few_arguments_prints_usage(self):
        result = self.aggregate("results")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("usage: aggregate.py", result.stderr)

    def test_an_empty_results_folder_fails(self):
        write(self.dir / "results" / "empty.jsonl", "\n")
        result = self.aggregate("results", "report.md", "summary.json")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("no results in results", result.stderr)

    def test_writes_the_report_and_summary_from_every_run_file(self):
        line = json.dumps(record("rt", "keepalive", "tokio-mt", ops_per_s=10))
        write(self.dir / "results" / "a" / "run.jsonl", line + "\n")
        write(self.dir / "results" / "b" / "run.jsonl", line + "\n\n")
        result = self.aggregate("results", "report.md", "summary.json", "Nightly")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue((self.dir / "report.md").read_text().startswith("# Nightly"))
        summary = json.loads((self.dir / "summary.json").read_text())
        self.assertEqual(summary["runs"], 2)
        self.assertEqual(summary["results"][0]["speed_vs_baseline"]["n"], 2)


# Stands in for the harness binary: prints one record for db and tpl, and listens on its port as a server.
HARNESS = """#!/usr/bin/env bash
echo "$*" >>"$CALLS"
case "$1" in
  setup) ;;
  db) echo '{"suite":"db","case":"'"$3"'","variant":"'"$2"'","metrics":{"ops_per_s":1}}' ;;
  tpl) echo '{"suite":"tpl","case":"page","variant":"askama","metrics":{"ns_per_render":1}}' ;;
  rt) exec python3 -c 'import socket, sys, time
s = socket.socket(); s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(("127.0.0.1", int(sys.argv[1]))); s.listen(); time.sleep(60)' "$3" ;;
esac
"""
# Stands in for the load generator: JSON output only when asked for it.
OHA = """#!/usr/bin/env bash
if [[ "$*" == *"--output-format json"* ]]; then
  echo '{"summary":{"requestsPerSec":1000},"latencyPercentiles":{"p50":0.001,"p99":0.002,"p99.9":0.003},
        "errorDistribution":{"aborted due to deadline":5,"refused":2}}'
fi
"""


class RunScript(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.dir = Path(tmp.name)
        write(self.dir / "bin" / "harness", HARNESS, 0o755)
        write(self.dir / "bin" / "oha", OHA, 0o755)
        self.calls = self.dir / "calls.log"

    def run_script(self, **env):
        path = f"{self.dir / 'bin'}:/usr/bin:/bin"
        return run(
            ["bash", BENCHMARKS / "run.sh"],
            self.dir,
            BIN=str(self.dir / "bin" / "harness"),
            CALLS=str(self.calls),
            OUT=str(self.dir / "out" / "run.jsonl"),
            PATH=path,
            **env,
        )

    def records(self):
        lines = (self.dir / "out" / "run.jsonl").read_text().splitlines()
        return [json.loads(line) for line in lines]

    def test_runs_only_the_chosen_suites_and_scales_database_work(self):
        result = self.run_script(SUITES="db,tpl", SCALE="2")
        self.assertEqual(result.returncode, 0, result.stderr)
        records = self.records()
        self.assertEqual(len([r for r in records if r["suite"] == "db"]), 21)
        self.assertEqual(len([r for r in records if r["suite"] == "tpl"]), 1)
        calls = self.calls.read_text()
        self.assertIn("db sqlx lookup 40000 64 16", calls)
        self.assertIn("db sqlx book 20000 64 16", calls)
        self.assertNotIn("setup", calls)
        self.assertIn("wrote 22 records", result.stderr)

    def test_seeds_the_database_only_with_an_admin_url_and_a_database_suite(self):
        self.run_script(SUITES="tpl", ADMIN_URL="postgres://admin@h/db")
        self.assertNotIn("setup", self.calls.read_text())
        self.run_script(SUITES="db", ADMIN_URL="postgres://admin@h/db")
        self.assertIn("setup postgres://admin@h/db", self.calls.read_text())

    def test_a_server_suite_loads_each_option_and_records_its_metrics(self):
        result = self.run_script(SUITES="rt", SECONDS_PER="1")
        self.assertEqual(result.returncode, 0, result.stderr)
        records = self.records()
        self.assertEqual(sorted(r["variant"] for r in records), ["compio", "monoio", "tokio-ct", "tokio-mt"])
        metrics = records[0]["metrics"]
        self.assertEqual(metrics["ops_per_s"], 1000)
        self.assertEqual((metrics["p50_us"], metrics["p99_us"]), (1000, 2000))
        self.assertEqual(metrics["errors"], 2)
        self.assertGreaterEqual(metrics["server_cores_busy"], 0)


if __name__ == "__main__":
    unittest.main()
