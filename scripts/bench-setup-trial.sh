#!/usr/bin/env bash
# Sets up a fresh run of the agentic-efficiency-benchmark protocol described
# in docs/design/experiments/agentic-efficiency-benchmark.md: builds knap,
# generates a seeded vault, turns it into two git repos (baseline/,
# knap-assisted/) at an identical bench-vault-seed tag, installs the knap
# skill (+ a CLAUDE.md that names knap explicitly, so skill *discoverability*
# isn't a variable in trials that aren't specifically testing that) into
# knap-assisted/ only, and renders the 7-step task prompt with this seed's
# concrete resolved names substituted in.
#
# Usage:
#   scripts/bench-setup-trial.sh --out DIR [--seed N] [--notes N]
#     [--broken-links N] [--broken-anchors N]
#
# Requires: cargo, git, jq. Verifies (never modifies) that `knap` resolves on
# PATH to this repo's freshly built release binary before doing anything —
# every trial's whole premise depends on that being true, so a stale binary
# shadowing it earlier on PATH fails loud instead of silently benchmarking
# the wrong knap version.
#
# Output layout under DIR:
#   vault-seed/           the generated vault + BENCH_MANIFEST.json
#   index.json            `knap index --json` over vault-seed
#   baseline/              git repo, no knap skill/CLAUDE.md
#   knap-assisted/          git repo, knap skill + CLAUDE.md installed
#   task.txt                the rendered 7-step task, no directory appended
#   prompt.baseline.txt     task.txt + baseline/'s absolute path
#   prompt.knap.txt         task.txt + knap-assisted/'s absolute path

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/bench-setup-trial.sh --out DIR [options]

Options:
  --out DIR              output directory (required; cleared if it exists)
  --seed N               RNG seed for gen_bench_vault (default: 1)
  --notes N              vault size (default: 200)
  --broken-links N       seeded broken-link defects (default: 12)
  --broken-anchors N     seeded broken-anchor defects (default: 8)
  -h, --help             show this help
EOF
}

OUT=""
SEED=1
NOTES=200
BROKEN_LINKS=12
BROKEN_ANCHORS=8

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --seed) SEED="$2"; shift 2 ;;
    --notes) NOTES="$2"; shift 2 ;;
    --broken-links) BROKEN_LINKS="$2"; shift 2 ;;
    --broken-anchors) BROKEN_ANCHORS="$2"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; usage; exit 1 ;;
  esac
done

if [[ -z "$OUT" ]]; then
  echo "error: --out DIR is required" >&2
  usage
  exit 1
fi
for cmd in cargo git jq; do
  command -v "$cmd" >/dev/null || { echo "error: '$cmd' is required on PATH" >&2; exit 1; }
done

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"
OUT="$(mkdir -p "$OUT" && cd "$OUT" && pwd)"

echo "==> building knap (release)"
cargo build --release --quiet
BUILT_BIN="$REPO_ROOT/target/release/knap"

echo "==> verifying 'knap' on PATH is this build"
ON_PATH="$(command -v knap || true)"
if [[ -z "$ON_PATH" ]]; then
  echo "error: no 'knap' found on PATH." >&2
  echo "Put $BUILT_BIN on PATH ahead of anything else named knap, then re-run." >&2
  exit 1
fi
BUILT_VERSION="$("$BUILT_BIN" --version)"
ON_PATH_VERSION="$(knap --version)"
if [[ "$ON_PATH_VERSION" != "$BUILT_VERSION" ]]; then
  echo "error: 'knap' on PATH ($ON_PATH) reports '$ON_PATH_VERSION', but this repo's freshly built binary reports '$BUILT_VERSION'." >&2
  echo "A benchmark trial is only meaningful if the agent finds *this* build's behavior on PATH." >&2
  echo "Run 'which -a knap' to see what's shadowing it, update or remove that install, and re-run." >&2
  exit 1
fi
if ! cmp -s "$ON_PATH" "$BUILT_BIN"; then
  echo "    warning: '$ON_PATH' matches this build's version ($ON_PATH_VERSION) but isn't byte-identical to $BUILT_BIN" >&2
  echo "    (expected — cargo install vs. cargo build produce different bytes from the same source; version match is what matters)" >&2
fi
echo "    OK: $ON_PATH ($ON_PATH_VERSION)"

echo "==> generating vault (seed=$SEED notes=$NOTES broken-links=$BROKEN_LINKS broken-anchors=$BROKEN_ANCHORS)"
rm -rf "$OUT/vault-seed" "$OUT/baseline" "$OUT/knap-assisted"
cargo run --release --quiet --example gen_bench_vault -- \
  --out "$OUT/vault-seed" --seed "$SEED" --notes "$NOTES" \
  --broken-links "$BROKEN_LINKS" --broken-anchors "$BROKEN_ANCHORS"

echo "==> verifying generator/resolver agreement"
PROBLEMS=$(knap lint "$OUT/vault-seed" --json | jq '.problem_count' || true)
EXPECTED=$((BROKEN_LINKS + BROKEN_ANCHORS))
if [[ "$PROBLEMS" != "$EXPECTED" ]]; then
  echo "error: knap lint found $PROBLEMS problem(s), expected exactly $EXPECTED seeded defects" >&2
  echo "The generator's link/anchor logic and knap's resolver have drifted apart — fix before benchmarking." >&2
  exit 1
fi
echo "    OK: $PROBLEMS/$EXPECTED"

echo "==> resolving concrete task targets for this seed"
(cd "$OUT/vault-seed" && knap index . --json) > "$OUT/index.json"

HUB=$(jq -r '.notes | sort_by(-(.backlinks | length)) | .[0].path' "$OUT/index.json")
HUB_BACKLINKS=$(jq -r --arg p "$HUB" '.notes[] | select(.path==$p) | (.backlinks | length)' "$OUT/index.json")
SPLIT_TARGET=$(jq -r '.notes | sort_by(-(.backlinks | length)) | .[1].path' "$OUT/index.json")
SPLIT_BACKLINKS=$(jq -r --arg p "$SPLIT_TARGET" '.notes[] | select(.path==$p) | (.backlinks | length)' "$OUT/index.json")
HEADING=$(jq -r --arg p "$HUB" '.notes[] | select(.path==$p) | .headings[1].text' "$OUT/index.json")
TAG=$(jq -r '.tags | to_entries | sort_by(-(.value | length)) | .[0].key' "$OUT/index.json")

HUB_STEM="${HUB%.md}"
HUB_NEW="${HUB_STEM}-notes.md"
if jq -e --arg p "$HUB_NEW" '.notes[] | select(.path==$p)' "$OUT/index.json" >/dev/null; then
  HUB_NEW="${HUB_STEM}-v2.md"
fi

case "$HEADING" in
  *Overview) NEW_HEADING="${HEADING% Overview} Guide" ;;
  *Details) NEW_HEADING="${HEADING% Details} Notes" ;;
  *) NEW_HEADING="${HEADING} (Renamed)" ;;
esac

case "$TAG" in
  draft) NEW_TAG=staged ;;
  published) NEW_TAG=released ;;
  reference) NEW_TAG=resource ;;
  idea) NEW_TAG=concept ;;
  archived) NEW_TAG=deprecated ;;
  *) NEW_TAG="${TAG}-v2" ;;
esac

echo "    hub note:      $HUB ($HUB_BACKLINKS backlinks) -> $HUB_NEW"
echo "    heading:       '$HEADING' -> '$NEW_HEADING'"
echo "    tag:           $TAG -> $NEW_TAG"
echo "    split target:  $SPLIT_TARGET ($SPLIT_BACKLINKS backlinks)"

LINK_FILES=$(jq -r '.broken_links[].file | "   - " + .' "$OUT/vault-seed/BENCH_MANIFEST.json")
ANCHOR_FILES=$(jq -r '.broken_anchors[].file | "   - " + .' "$OUT/vault-seed/BENCH_MANIFEST.json")

echo "==> rendering task.txt"
cat > "$OUT/task.txt" <<EOF
You are editing a Markdown notes vault. Work entirely inside the directory
given to you below — do not touch any other directory.

Before step 1, check whether that directory has a CLAUDE.md file at its
root; if it does, read it and follow any project-specific conventions it
describes for the rest of this session, the same way you would if this
directory were your actual session root (it isn't, so nothing auto-loads
it for you — you have to check yourself).

Then perform the following 7 steps, in order, as a single editing session.
When you're done, run a final check yourself to make sure nothing is left
broken, then report what you changed.

1. Rename the note \`$HUB\` to \`$HUB_NEW\`.
   Update every reference to it (links, anchors) throughout the vault.
2. In that renamed note, rename its "$HEADING" heading to
   "$NEW_HEADING". Confirm every link that anchors into that heading
   still resolves correctly afterward.
3. Rename the tag \`$TAG\` to \`$NEW_TAG\` across every note that
   carries it.
4. Add two new notes to the vault (topics you choose, consistent with the
   vault's existing subject matter) and link each of them in from at
   least one existing note. At least one of the new links must be an
   anchor link to a specific heading.
5. Split the note \`$SPLIT_TARGET\` into two separate notes (divide its
   content into two logical pieces, your call on where the split falls).
   Update every existing link that pointed at \`$SPLIT_TARGET\` (or at
   a heading inside it) to point at whichever of the two new notes now
   actually contains that content.
6. The following notes each contain one broken link or broken anchor
   link that needs to be fixed — the link should point at the file/heading
   its surrounding text clearly means, but the target got mangled at some
   point and no longer resolves. Find the correct existing target for each
   and fix it. Don't guess mechanically — read what the link is about
   before deciding where it should point.

   Broken links (wrong/missing target file):
$LINK_FILES

   Broken anchors (wrong/missing heading in an otherwise-valid target):
$ANCHOR_FILES

7. Confirm there are no broken links or anchors left anywhere in the vault.

Your working directory for this entire task is:
EOF

echo "==> setting up baseline/ and knap-assisted/ repos"
for repo in baseline knap-assisted; do
  cp -r "$OUT/vault-seed" "$OUT/$repo"
  (
    cd "$OUT/$repo"
    git init -q
    git add -A
    git -c user.email=bench@example.com -c user.name=bench commit -q -m seed
  )
done

echo "==> installing knap skill + CLAUDE.md in knap-assisted/ only"
mkdir -p "$OUT/knap-assisted/.claude/skills/knap"
cp "$REPO_ROOT/skill/knap/SKILL.md" "$OUT/knap-assisted/.claude/skills/knap/SKILL.md"
cat > "$OUT/knap-assisted/CLAUDE.md" <<'EOF'
# Working notes

This vault has `knap` — a Markdown link/anchor linter, indexer, and
refactoring CLI — installed on `PATH`, with its skill at
`.claude/skills/knap/SKILL.md`. Read that skill and use `knap`
(`lint`/`rename-*`/`apply`/`index`, per the skill's edit → verify loop) for
editing and verifying links in this vault, instead of hand-`grep`ing or
hand-tracking link targets yourself. This is a project convention for this
repo, not optional tooling — reach for it any time a task edits, moves,
renames, or fixes Markdown docs here.
EOF
(
  cd "$OUT/knap-assisted"
  git add -A
  git -c user.email=bench@example.com -c user.name=bench commit -q -m "install knap skill + CLAUDE.md"
)

for repo in baseline knap-assisted; do
  (cd "$OUT/$repo" && git tag -f bench-vault-seed >/dev/null)
done

echo "==> writing per-condition prompts"
printf '%s\n%s\n' "$(cat "$OUT/task.txt")" "$OUT/baseline" > "$OUT/prompt.baseline.txt"
printf '%s\n%s\n' "$(cat "$OUT/task.txt")" "$OUT/knap-assisted" > "$OUT/prompt.knap.txt"

cat <<EOF

Trial ready under $OUT

  baseline repo:       $OUT/baseline        (bench-vault-seed tag, no knap skill/CLAUDE.md)
  knap-assisted repo:   $OUT/knap-assisted    (bench-vault-seed tag, knap skill + CLAUDE.md installed)
  task prompt:          $OUT/task.txt
  per-condition prompts: $OUT/prompt.baseline.txt, $OUT/prompt.knap.txt
  ground truth:          $OUT/vault-seed/BENCH_MANIFEST.json
  vault index:           $OUT/index.json

To re-seed either repo before a re-run: (cd $OUT/<repo> && git reset --hard bench-vault-seed && git clean -fdx)
EOF
