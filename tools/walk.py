"""The tables of docs/walk.md, from walk-sim's JSON lines, or
socket-probe's.

    python3 -I tools/walk.py walk.jsonl [other.jsonl ...]

One log: a row a case, source and recording. More than one: each case's
figures in every log side by side, the first log being the run before, and
a last table of their sums over every case. A log of socket probes: a row a
probe. A sweep, every line with its `variant` (tools/walk-sweep.sh's logs
put together): each variant against `base`, over every case and over those
where a link narrowed, swung or dipped — every case but the jittery ones and
the floors' — with the rate it held on the jittery ones.

    python3 -I tools/walk.py --sweep walk-sweep.jsonl
"""

import json
import sys

COLUMNS = [
    ("kbit/s", lambda r: "%.0f" % r["rate_mean"]),
    ("late p50", lambda r: "%.0f" % r["late_p50"]),
    ("p95", lambda r: "%.0f" % r["late_p95"]),
    ("max", lambda r: "%.0f" % r["late_max"]),
    ("dropped", lambda r: "%.1f" % r["dropped"]),
    ("skipped", lambda r: "%.1f" % r["skipped"]),
    ("shed", lambda r: "%.1f" % r["shed"]),
    ("stalled", lambda r: "%.1f" % r["stalled"]),
    ("moves", lambda r: "%d" % r["moves"]),
]

RECORDINGS = {"1. Golf Club - Stay Awhile": "music", "03013_Leviticus_13": "speech"}


def name(r):
    return RECORDINGS.get(r["recording"], r["recording"])


def key(r):
    return (r["case"], r["source"], name(r))


def load(path):
    with open(path) as f:
        return [json.loads(line) for line in f if line.strip()]


def table(rows):
    print("| case | source | sound | " + " | ".join(c for c, _ in COLUMNS) + " |")
    print("|---|---|---|" + "---:|" * len(COLUMNS))
    for r in rows:
        print("| %s | %s | %s | " % key(r) + " | ".join(f(r) for _, f in COLUMNS) + " |")


def sums(logs):
    print("| log | kbit/s mean | late p95 sum | late max sum | lost s | stalled s | moves |")
    print("|---|---:|---:|---:|---:|---:|---:|")
    for path, rows in logs:
        print("| %s | %.0f | %.0f | %.0f | %.1f | %.1f | %d |" % (
            path.rsplit("/", 1)[-1].removesuffix(".jsonl"),
            sum(r["rate_mean"] for r in rows) / len(rows),
            sum(r["late_p95"] for r in rows),
            sum(r["late_max"] for r in rows),
            sum(r["dropped"] + r["skipped"] for r in rows),
            sum(r["stalled"] for r in rows),
            sum(r["moves"] for r in rows),
        ))


def side_by_side(logs):
    names = [p.rsplit("/", 1)[-1].removesuffix(".jsonl") for p, _ in logs]
    print("| case | source | sound | " + " | ".join(names) + " |")
    print("|---|---|---|" + "---|" * len(names))
    print("| | | | " + " | ".join("kbit/s, late p95/max ms, lost s, stalled s" for _ in names) + " |")
    first = logs[0][1]
    for r in first:
        cells = []
        for _, rows in logs:
            o = next((o for o in rows if key(o) == key(r)), None)
            cells.append("—" if o is None else "%.0f, %.0f/%.0f, %.1f, %.1f" % (
                o["rate_mean"], o["late_p95"], o["late_max"], o["dropped"] + o["skipped"], o["stalled"]))
        print("| %s | %s | %s | " % key(r) + " | ".join(cells) + " |")


def socket(rows):
    print("| Link, kbit/s | Queue, ms | Congestion control | tcp_wmem | Low-water mark | First block, s | Longest, ms | Blocks | Late p50, ms | Late max, ms | Held p50, bytes | Unsent p50, bytes |")
    print("|---:|---:|---|---|---|---:|---:|---:|---:|---:|---:|---:|")
    for r in rows:
        print("| %d | %.0f | %s | %s | %s | %s | %.0f | %d of %d | %d | %d | %d | %d |" % (
            r["rate"], r["queue_ms"], r["cc"], r["wmem"], r["lowat"] if r["lowat"] is not None else "—",
            "never" if r["first_block"] is None else "%.1f" % r["first_block"],
            r["longest_block_ms"], r["blocks"], r["writes"], r["late_p50_ms"], r["late_max_ms"], r["outq_p50"], r["notsent_p50"],
        ))


def sweep(rows):
    variants = {}
    for r in rows:
        variants.setdefault(r["variant"], []).append(r)

    def jittery(r):
        return "jitter" in r["case"]

    def moving(r):
        return not jittery(r) and "floor" not in r["case"]

    def totals(rows):
        return {
            "rate": sum(r["rate_mean"] for r in rows) / len(rows),
            "lost": sum(r["dropped"] + r["skipped"] for r in rows),
            "late": sum(r["late_p95"] for r in rows),
            "stalled": sum(r["stalled"] for r in rows),
            "moves": sum(r["moves"] for r in rows),
        }

    def change(mine, base):
        return "%+.0f%%" % ((mine / base - 1) * 100) if base else "—"

    base = variants["base"]
    every, narrowed = totals(base), totals([r for r in base if moving(r)])
    print("| Variant | Rate | Lost | Late p95 | Moves | Rate, narrowed | Lost, narrowed | Late p95, narrowed | Stalled, narrowed | Moves, narrowed | kbit/s, jittery |")
    print("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|")
    for name, rows in variants.items():
        a, n = totals(rows), totals([r for r in rows if moving(r)])
        jitter = [r["rate_mean"] for r in rows if jittery(r)]
        print("| %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %.0f–%.0f |" % (
            name, change(a["rate"], every["rate"]), change(a["lost"], every["lost"]), change(a["late"], every["late"]), change(a["moves"], every["moves"]),
            change(n["rate"], narrowed["rate"]), change(n["lost"], narrowed["lost"]), change(n["late"], narrowed["late"]),
            change(n["stalled"], narrowed["stalled"]), change(n["moves"], narrowed["moves"]), min(jitter), max(jitter),
        ))


def main():
    if sys.argv[1] == "--sweep":
        sweep(load(sys.argv[2]))
        return
    logs = [(p, load(p)) for p in sys.argv[1:]]
    if "cc" in logs[0][1][0]:
        socket(logs[0][1])
    elif len(logs) == 1:
        table(logs[0][1])
    else:
        side_by_side(logs)
        print()
        sums(logs)


if __name__ == "__main__":
    main()
