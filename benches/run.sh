#!/usr/bin/env bash
# Informational performance dimension for the eval gate (SPEC-20260910111449
# R1). Reads criterion's per-benchmark medians and emits the same JSON shape
# the eval baseline uses, but never fails on timing: absolute thresholds are
# machine-dependent and shared CI runners swing 10-30%, so this reports
# numbers and exits 0. A regression shows up as a slower ns/iter for a human
# (or a future same-machine baseline diff) to act on, not as a red gate.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo bench --bench core -- --quick >/dev/null 2>&1

BENCH_JSON=$(python3 - <<'PY'
import glob, json, os
out = {}
for path in sorted(glob.glob("target/criterion/*/new/estimates.json")):
    name = os.path.basename(os.path.dirname(os.path.dirname(path)))
    out[name] = round(json.load(open(path))["median"]["point_estimate"])
print(json.dumps(out))
PY
)

printf '{"performance": {"passed": true, "score": 1.0, "verdict": "INFO", "details": {"note": "informational, no absolute gating", "ns_per_iter_median": %s}}}\n' "$BENCH_JSON"
