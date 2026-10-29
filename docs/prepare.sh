#!/bin/sh
# SPDX-License-Identifier: MIT
# Brings into docs/ what the documentation shows but has its one home elsewhere: the specification and its data
# (spec/), the changelog, and the example of the library guide. Run before Jekyll builds docs/ – by
# .github/workflows/docs.yml, or by hand for a local build. The files it writes are not tracked.
set -eu
cd "$(dirname "$0")"

rm -rf spec _includes/quickstart.rs changelog.md
mkdir -p spec _includes
cp -R ../spec/data spec/data

# Each part with what the navigation needs: its title from its first heading, the order from its number.
for part in ../spec/0*.md; do
  name=$(basename "$part")
  title=$(sed -n 's/^# //p' "$part" | head -n 1)
  order=$(echo "$name" | sed 's/^0*\([0-9]*\)-.*/\1/')
  printf -- '---\ntitle: "%s"\nparent: Specification\nnav_order: %s\nedit_path: spec/%s\n---\n\n' "$title" "$order" "$name" > "spec/$name"
  cat "$part" >> "spec/$name"
done
printf -- '---\ntitle: Specification\nnav_order: 2\nhas_children: true\npermalink: /spec/\nedit_path: spec/README.md\nredirect_from:\n  - /spec/overview.html\n---\n\n' > spec/README.md
cat ../spec/README.md >> spec/README.md

printf -- '---\ntitle: Changelog\nnav_order: 99\nedit_path: CHANGELOG.md\n---\n\n' > changelog.md
cat ../CHANGELOG.md >> changelog.md

# The example from its fourth line on – after its licence header and summary.
tail -n +4 ../crates/librotenado/examples/quickstart.rs > _includes/quickstart.rs
