#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/../.."
mkdir -p tests/visual_baseline/output
for preset in tests/visual_baseline/presets/*.myb; do
    name=$(basename "$preset" .myb)
    for stroke in tests/visual_baseline/strokes/*.json; do
        sname=$(basename "$stroke" .json)
        echo "render ${name} × ${sname}"
        cargo run --release --example render_baseline -- \
            "$preset" "$stroke" \
            "tests/visual_baseline/output/${name}_${sname}.png"
    done
done
echo "done: $(ls tests/visual_baseline/output/*.png | wc -l | tr -d ' ') PNG generated"
