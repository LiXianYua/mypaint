#!/bin/bash
set -e
cd "$(dirname "$0")/../.."
mkdir -p tests/visual_baseline/output
for preset in tests/visual_baseline/presets/*.myb; do
    name=$(basename "$preset" .myb)
    for stroke in tests/visual_baseline/strokes/*.json; do
        sname=$(basename "$stroke" .json)
        cargo run --release --example render_baseline -- \
            "$preset" "$stroke" \
            "tests/visual_baseline/output/${name}_${sname}.png"
    done
done
echo "done: $(ls tests/visual_baseline/output/*.png | wc -l) PNG generated"
