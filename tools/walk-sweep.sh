#!/bin/sh
# Every constant of src/walk.rs moved one way and the other: for each, a copy
# of the crate with that one line changed, built, and the cases run on it,
# every line of a log marked with its variant. The figures on the constants'
# comments are each against `base`, the walk as it is (docs/walk.md):
#
#     tools/walk-sweep.sh OUT
#     cat OUT/*.jsonl > walk-sweep.jsonl
#     python3 -I tools/walk.py --sweep walk-sweep.jsonl
#
# ONLY, a list of variants' names, runs those alone, `base` among them or not,
# and WITH, a list of NAME=VALUE pairs, is set on every variant first, so a
# round's items are measured one on top of another with the rest off:
#
#     ONLY="base noclimb climb500" WITH="HEAVY_COOLDOWN=Duration::from_secs(2)" tools/walk-sweep.sh OUT
#
# The recordings are tools/walk-cases.sh's, under tmp/samples or SAMPLES.
set -eu
only=${ONLY:-}
with=${WITH:-}
repo=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$1/bin"
out=$(cd "$1" && pwd)
# An OUT inside the repository is not copied into its own crate.
case $out in
"$repo"/*) inside="/${out#"$repo"/}" ;;
*) inside= ;;
esac
rsync -a --delete --exclude target --exclude .git ${inside:+--exclude "$inside"} "$repo/" "$out/crate/"

# NAME then NAME=VALUE pairs: the walk with each constant set so. A perl
# substitution, s/.../.../, in place of a pair changes a line of the code.
variant() {
    name=$1
    shift
    if [ -n "$only" ]; then
        case " $only " in
        *" $name "*) ;;
        *) return ;;
        esac
    fi
    cp "$repo/src/walk.rs" "$out/crate/src/walk.rs"
    for pair in $with "$@"; do
        case $pair in
        s/*) perl -pi -e "$pair" "$out/crate/src/walk.rs" ;;
        *) perl -pi -e "s/^((?:pub )?const ${pair%%=*}: [^=]*= ).*;/\${1}${pair#*=};/" "$out/crate/src/walk.rs" ;;
        esac
    done
    if [ $# -gt 0 ] && [ -z "$with" ] && cmp -s "$repo/src/walk.rs" "$out/crate/src/walk.rs"; then
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
# The floor's skipping is skipping too.
noprobe='s/self\.bitrate <= BITRATE_FLOOR && self\.skipping/false \&\& self.skipping/'
variant noskip SKIP_LAG="$(secs 3600)" "$noprobe"
variant noprobe "$noprobe"
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
# The climb: a span as long as the clear one is no climb at all.
variant noclimb CLIMB_SPAN="$(secs 3)"
variant climb500 CLIMB_SPAN="$(ms 500)"
variant climb1500 CLIMB_SPAN="$(ms 1500)"
variant idle10 LAG_IDLE="$(ms 10)"
variant idle25 LAG_IDLE="$(ms 25)"
# A fraction this large is sound that is never quiet.
variant noquiet QUIET_FRACTION=1_000_000
variant quiet4 QUIET_FRACTION=4
variant quiet8 QUIET_FRACTION=8
# A heavy cooldown as long as the cooldown is none.
variant noheavy HEAVY_COOLDOWN="$(secs 2)"
variant heavy500 HEAVY_COOLDOWN="$(ms 500)"
variant heavy1500 HEAVY_COOLDOWN="$(ms 1500)"
variant nospacing PING_SPACING="$(ms 0)"
variant spacing150 PING_SPACING="$(ms 150)"
variant spacing180 PING_SPACING="$(ms 180)"
# Every item of this round off: the walk as v0.0.5 had it.
variant v0.0.5 CLIMB_SPAN="$(secs 3)" HEAVY_COOLDOWN="$(secs 2)" PING_SPACING="$(ms 0)"

# Four at a time.
cd "$repo"
n=0
for bin in "$out"/bin/*; do
    name=$(basename "$bin")
    [ -n "$only" ] && case " $only " in *" $name "*) ;; *) continue ;; esac
    SIM=$bin tools/walk-cases.sh | sed "s/^{/{\"variant\":\"$name\",/" > "$out/$name.jsonl" &
    n=$((n + 1))
    [ $((n % 4)) -eq 0 ] && wait
done
wait
