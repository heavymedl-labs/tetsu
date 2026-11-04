#!/usr/bin/env python3
"""Combines benchmark runs into a Markdown report and a JSON summary.

usage: aggregate.py <results-dir> <report.md> <summary.json> [title]

Each *.jsonl file in the results directory is one run. For every option, the
ratio to the suite's baseline is computed within each run, then averaged, so
differences between runner machines cancel out.
"""

import json
import statistics
import sys
from collections import defaultdict
from pathlib import Path

# Baseline option and primary metric per suite. A higher speed ratio is better.
SUITES = {
    "db": ("Postgres access", "sqlx", "ops_per_s", True),
    "e2e": ("End to end", "sqlx", "ops_per_s", True),
    "http": ("HTTP framework", "axum", "ops_per_s", True),
    "rt": ("Async runtime, one core", "tokio-mt", "ops_per_s", True),
    "tpl": ("HTML templating", "askama", "ns_per_render", False),
}

# Two-sided 95% Student's t critical values by degrees of freedom.
T95 = {1: 12.71, 2: 4.30, 3: 3.18, 4: 2.78, 5: 2.57, 6: 2.45, 7: 2.36, 8: 2.31,
       9: 2.26, 10: 2.23, 15: 2.13, 20: 2.09, 30: 2.04}


def t95(df):
    keys = [k for k in T95 if k <= df]
    return T95[max(keys)] if keys else T95[1]


def stats(xs):
    n = len(xs)
    mean = statistics.fmean(xs)
    sd = statistics.stdev(xs) if n > 1 else 0.0
    ci = t95(n - 1) * sd / n ** 0.5 if n > 1 else float("nan")
    return {"n": n, "mean": mean, "median": statistics.median(xs), "stdev": sd,
            "cv_pct": 100 * sd / mean if mean else 0.0, "ci95": ci}


def load(results):
    runs = []
    for path in sorted(Path(results).rglob("*.jsonl")):
        recs = [json.loads(line) for line in path.read_text().splitlines() if line.strip()]
        if recs:
            runs.append(recs)
    return runs


def summarize(runs):
    values = defaultdict(lambda: defaultdict(list))
    ratios = defaultdict(list)
    for recs in runs:
        by_key = {(r["suite"], r["case"], r["variant"]): r["metrics"] for r in recs}
        for (suite, case, variant), m in by_key.items():
            if suite not in SUITES:
                continue
            _, base, metric, higher = SUITES[suite]
            for k, v in m.items():
                if isinstance(v, (int, float)):
                    values[(suite, case, variant)][k].append(float(v))
            b = by_key.get((suite, case, base))
            if b and m.get(metric) and b.get(metric):
                ratios[(suite, case, variant)].append(
                    m[metric] / b[metric] if higher else b[metric] / m[metric])
    out = []
    for key in sorted(values):
        suite, case, variant = key
        out.append({
            "suite": suite, "case": case, "variant": variant,
            "speed_vs_baseline": stats(ratios[key]) if ratios[key] else None,
            "metrics": {k: stats(v) for k, v in values[key].items()},
        })
    return out


def fmt(x, digits=0):
    return "n/a" if x is None or x != x else f"{x:,.{digits}f}"


def report(rows, runs, title):
    lines = [f"# {title}", "",
             f"{len(runs)} run(s). Speed is each option's throughput, or render speed, relative to the baseline's "
             "in the same run, averaged across runs; above 1.00 is faster. "
             "CV is the coefficient of variation across runs.", ""]
    for suite, (name, base, metric, _) in SUITES.items():
        suite_rows = [r for r in rows if r["suite"] == suite]
        if not suite_rows:
            continue
        unit = "ns per render" if metric == "ns_per_render" else "ops/s"
        lines += [f"## {name}", "", f"Baseline: `{base}`.", "",
                  f"| Case | Option | Speed vs baseline (95% CI) | Mean {unit} (CV) | Median p99 | CPU per op | Server cores busy |",
                  "| --- | --- | --- | --- | --- | --- | --- |"]
        suite_rows.sort(key=lambda r: (r["case"], -(r["speed_vs_baseline"] or {}).get("mean", 0)))
        for r in suite_rows:
            s = r["speed_vs_baseline"]
            speed = f"{s['mean']:.2f} ± {fmt(s['ci95'], 2)}" if s else "n/a"
            m = r["metrics"]
            main = m.get(metric)
            main_s = f"{fmt(main['mean'])} ({main['cv_pct']:.0f}%)" if main else "n/a"
            p99 = m.get("p99_us")
            p99_s = f"{p99['median'] / 1000:.2f} ms" if p99 else ""
            cpu = m.get("cpu_us_per_op")
            cpu_s = f"{cpu['mean']:.0f} µs" if cpu else ""
            busy = m.get("server_cores_busy")
            busy_s = f"{busy['median']:.2f}" if busy else ""
            lines.append(f"| {r['case']} | `{r['variant']}` | {speed} | {main_s} | {p99_s} | {cpu_s} | {busy_s} |")
        lines.append("")
    return "\n".join(lines)


def main():
    if len(sys.argv) < 4:
        sys.exit(__doc__)
    results, md, js = sys.argv[1:4]
    title = sys.argv[4] if len(sys.argv) > 4 else "Benchmark results"
    runs = load(results)
    if not runs:
        sys.exit(f"no results in {results}")
    rows = summarize(runs)
    Path(md).write_text(report(rows, runs, title))
    Path(js).write_text(json.dumps({"runs": len(runs), "results": rows}, indent=2))


if __name__ == "__main__":
    main()
