#!/bin/sh
# Every constant of src/walk.rs moved one way and the other: for each, a copy
# of the crate with that one line changed, built, and the cases run on it.
# The figures on the constants' comments are these logs against base.jsonl,
# the walk as it is (docs/walk.md):
#
#     tools/walk-sweep.sh OUT
#     python3 -I tools/walk.py OUT/base.jsonl OUT/*.jsonl
#
# The recordings are tools/walk-cases.sh's, under tmp/samples or SAMPLES.
set -eu
out=$1
repo=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$out/bin"
rsync -a --delete --exclude target --exclude .git "$repo/" "$out/crate/"

# NAME then NAME=VALUE pairs: the walk with each constant set so.
variant() {
    name=$1
    shift
    cp "$repo/src/walk.rs" "$out/crate/src/walk.rs"
    for pair in "$@"; do
        perl -pi -e "s/^((?:pub )?const ${pair%%=*}: [^=]*= ).*;/\${1}${pair#*=};/" "$out/crate/src/walk.rs"
    done
    if [ $# -gt 0 ] && cmp -s "$repo/src/walk.rs" "$out/crate/src/walk.rs"; then
        echo "$name changed nothing" >&2
        exit 1
    fi
    (cd "$out/crate" && CARGO_TARGET_DIR="$out/target" cargo build -q --release --example walk-sim)
    cp "$out/target/release/examples/walk-sim" "$out/bin/$name"
}

ms() { echo "Duration::from_millis($1)"; }
secs() { echo "Duration::from_secs($1)"; }

variant base
variant behind20 BEHIND_LAG="$(ms 20)" CLEAR_LAG="$(ms 10)"
variant behind60 BEHIND_LAG="$(ms 60)" CLEAR_LAG="$(ms 30)"
variant behind80 BEHIND_LAG="$(ms 80)" CLEAR_LAG="$(ms 40)"
variant behind150 BEHIND_LAG="$(ms 150)" CLEAR_LAG="$(ms 75)" LAG_HEAVY="$(ms 300)" LAG_SEVERE="$(ms 600)"
variant clear25 CLEAR_LAG="$(ms 25)"
variant onestep LAG_HEAVY="$(secs 1000)" LAG_SEVERE="$(secs 2000)"
variant heavy300 LAG_HEAVY="$(ms 300)" LAG_SEVERE="$(ms 800)"
variant skip150 SKIP_LAG="$(ms 150)"
variant skip300 SKIP_LAG="$(ms 300)"
variant noskip SKIP_LAG="$(secs 3600)"
variant overdue50 OVERDUE="$(ms 50)"
variant overdue200 OVERDUE="$(ms 200)"
variant nooverdue OVERDUE="$(secs 3600)"
variant hearings3 BEHIND_HEARINGS=3
variant span2 CLEAR_SPAN="$(secs 2)"
variant span5 CLEAR_SPAN="$(secs 5)"
variant cool1 ADJUST_COOLDOWN="$(secs 1)"
variant cool3 ADJUST_COOLDOWN="$(secs 3)"
variant down34 'STEP_DOWN=(3, 4)'
variant up8 STEP_UP=8
variant up32 STEP_UP=32
variant hold5 REFUSAL_HOLD="$(secs 5)"
variant hold30 REFUSAL_HOLD="$(secs 30)"
variant together ADJUST_COOLDOWN="$(secs 1)" CLEAR_SPAN="$(secs 5)" REFUSAL_HOLD="$(secs 30)"
variant floor32 BITRATE_FLOOR=32_000
variant floor16 BITRATE_FLOOR=16_000

# Four at a time.
cd "$repo"
n=0
for bin in "$out"/bin/*; do
    SIM=$bin tools/walk-cases.sh > "$out/$(basename "$bin").jsonl" &
    n=$((n + 1))
    [ $((n % 4)) -eq 0 ] && wait
done
wait
