#!/usr/bin/env sh
# Copies the files the skill ships with from their single source in this repo:
#   prelude.d.ts, the examples, and the level guide (SPEC.md §10).
# A test (skills/test/skill.test.mjs) fails when they are out of date.
set -eu
cd "$(dirname "$0")/.."
skill=skills/flowmark-review
mkdir -p "$skill/examples" "$skill/reference"
cp prelude.d.ts "$skill/prelude.d.ts"
cp examples/register-user.review.ts "$skill/examples/register-user.review.ts"
cp examples/t3code-pr-25709/head.review.ts "$skill/examples/t3code-update-server-settings.review.ts"
node scripts/level-guide.mjs > "$skill/reference/levels.md"
echo "synced $skill"
