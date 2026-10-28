#!/bin/sh
# The walk's cases, as docs/walk.md reads them: every case with an RDP host's
# buffers and with wlshare's units, over a minute of music and a minute of
# speech paused four seconds in every ten, one line of JSON a run on stdout.
# Extra arguments go to every run, so a constant's alternative is a second
# run of the same cases:
#
#     cargo build --release --example walk-sim
#     tools/walk-cases.sh > walk.jsonl
#
# The recordings are sound-flac's, under tmp/samples or SAMPLES. The
# simulation is deterministic: one architecture gives the same lines on any
# machine, but libopus codes a little differently on ARM and on x86
# (docs/walk.md).
set -eu
samples=${SAMPLES:-tmp/samples}
sim=${SIM:-target/release/examples/walk-sim}
music="$samples/1.-golf-club-stay-awhile/1. Golf Club - Stay Awhile.wav"
speech="$samples/LeviticusKJV/03013_Leviticus_13.wav"

# A link that swings between two rates every few seconds.
swing() {
    low=$1 high=$2 every=$3 out=$high t=$every
    while [ "$t" -lt 60 ]; do
        out="$out,$low@$t,$high@$((t + every))"
        t=$((t + 2 * every))
    done
    echo "$out"
}

# The sound's share of the link taken for half a second every ten, as a
# picture's keyframe on the same bottleneck takes it.
dips() {
    out=1000 t=10
    while [ "$t" -lt 60 ]; do
        out="$out,60@$t,1000@$t.5"
        t=$((t + 10))
    done
    echo "$out"
}

# A Wi-Fi hop holding everything every five seconds, for 80 to 500 ms, as a
# 2.4 GHz link retrying, scanning or yielding the air does, with room to
# spare between.
blips() {
    out= t=5
    for length in 100 250 150 400 120 300 200 500 80 180 350; do
        out="$out${out:+,}$length@$t"
        t=$((t + 5))
    done
    echo "$out"
}

run() {
    case=$1
    shift
    for source in rdp wlshare; do
        "$sim" "$music" --secs 60 --source $source --case "$case" "$@"
        "$sim" "$speech" --secs 60 --pause 4,10 --source $source --case "$case" "$@"
    done
}

run room --link 1000 "$@"
# Up to 60 ms more on any message or pong, as a Wi-Fi hop adds.
run room-jitter --link 1000 --jitter 60 "$@"
run room-jitter100 --link 1000 --jitter 100 "$@"
run narrow-jitter --link 1000,200@10 --jitter 60 "$@"
# Blips on a link with room, which no rate drains or needs to; and on one
# that narrows as well, which the walk must still step down for.
run room-blips --link 1000 --blip "$(blips)" "$@"
run narrow-blips --link 1000,200@10 --blip "$(blips)" "$@"
run narrow --link 1000,200@10 "$@"
run narrower --link 1000,100@10 "$@"
run recover --link 1000,150@10,1000@25 "$@"
run wobble --link "$(swing 250 450 3)" "$@"
run dips --link "$(dips)" "$@"
run start-low --link 150 "$@"
# Under the floor of 32 kbit/s and over one of 24, and under both.
run between-floors --link 1000,28@10 "$@"
run under-floor --link 1000,16@10 "$@"
# Another stream's queue at the bottleneck, 300 ms of it for half a minute,
# which no rate or skip of the sound's drains.
run path-delay --link 1000 --delay 0,300@10,0@40 "$@"
# A route 150 ms longer from ten seconds on, under the lag that skips: told
# apart from the sound's own queue only once the walk is at the floor.
run path-longer --link 1000 --delay 0,150@10 "$@"
# A keyframe's worth of pixels on wlshare's own connection to the gateway,
# every ten seconds, which the RDP sound does not share.
for recording in "$music" "$speech"; do
    "$sim" "$recording" --secs 60 --source wlshare --hop-link 20 --burst 600@10,600@20,600@30,600@40,600@50 --case hop-burst "$@"
done
