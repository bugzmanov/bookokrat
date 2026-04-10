#!/bin/bash
# Regenerate golden test data for SyncTeX tests.
# Requires: pdflatex, synctex (from TeX Live / BasicTeX)
#
# Usage: cd tests/testdata/synctex && ./generate_golden.sh
#
# This script:
# 1. Compiles test_main.tex with synctex enabled
# 2. Runs forward and inverse search queries
# 3. Prints results for manual inspection (update test_main_golden.json by hand)

set -euo pipefail

echo "=== Compiling test_main.tex ==="
pdflatex --synctex=1 -interaction=nonstopmode test_main.tex > /dev/null 2>&1
rm -f test_main.aux test_main.log
echo "Generated: test_main.pdf, test_main.synctex.gz"
echo

echo "=== Forward Search Queries ==="
for line in 6 8 9 11 15 17 19 21 25 27; do
    echo "--- Line $line ---"
    synctex view -i "$line:1:test_main.tex" -o "test_main.pdf" 2>/dev/null | grep -E "^(Page|h|v|W|H):"
    echo
done

echo "=== Inverse Search Queries ==="
for query in "1:100:84" "1:150:112" "2:150:112" "2:150:125" "3:150:112"; do
    echo "--- Query: page:x:y = $query ---"
    synctex edit -o "$query:test_main.pdf" 2>/dev/null | grep -E "^(Input|Line|Column):"
    echo
done

echo "Done. Review output above and update test_main_golden.json accordingly."
