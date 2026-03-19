#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")"

# Run the benchmark and extract metrics
OUTPUT=$(python3 benchmarks/autoresearch_bench.py 2>/dev/null)

# Parse JSON output
NEG_RECALL=$(echo "$OUTPUT" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['primary'])")
OVERALL=$(echo "$OUTPUT" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['overall_recall_at_20'])")
BUGS_HIT=$(echo "$OUTPUT" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['total_bugs_hit'])")
CAL=$(echo "$OUTPUT" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['folds']['cal.com']['recall_at_20'])")
DISC=$(echo "$OUTPUT" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['folds']['discourse']['recall_at_20'])")
GRAF=$(echo "$OUTPUT" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['folds']['grafana']['recall_at_20'])")
KEYC=$(echo "$OUTPUT" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['folds']['keycloak']['recall_at_20'])")
SENT=$(echo "$OUTPUT" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['folds']['sentry']['recall_at_20'])")

echo "METRIC neg_recall=$NEG_RECALL"
echo "METRIC overall_recall=$OVERALL"
echo "METRIC bugs_hit=$BUGS_HIT"
echo "METRIC cal_recall=$CAL"
echo "METRIC discourse_recall=$DISC"
echo "METRIC grafana_recall=$GRAF"
echo "METRIC keycloak_recall=$KEYC"
echo "METRIC sentry_recall=$SENT"
