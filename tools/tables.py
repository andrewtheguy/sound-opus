"""The tables of docs/benchmarks.md, from the log of examples/bench, a JSON
object a line (JSONL):

    cargo run --release --example bench > bench.jsonl
    python3 -I tools/tables.py bench.jsonl [--against SETTING] [--before before.jsonl]

First, for each setting in the log, the recordings taken together at every
rate: the time a block (the mean over the recordings, each by its length),
the share of one core it is, the worst recording's 99th-percentile and worst
block; the rate the packets came to, the most any second of any recording
came to and the largest 99th-percentile packet; the quality proxies, each
recording counting once; and the shares of CELT and fullband packets. Given
`--against`, every other setting over that one, rate by rate: the time and
the bytes as ratios, the proxies as differences. Given `--before`, the log
of the run before, its cells beside this run's where the setting and rate
are in both, over the recordings both have. Then the recordings one by one and the made sounds, a grid of
rates for each figure, of the first setting in the log, or of `--against`'s.
A ratio is the setting over the other, a difference the setting less the
other; green is the side of the line that is better, and a ratio of 1.00 or
a difference of 0.00 is left plain.
"""
import collections
import json
import re
import sys

KINDS = ["music", "speech", "tone", "noise", "silence"]

SEPARATOR = re.compile(r"[\\/]")

RECORDINGS = [
    ("Aria", "Bach, Goldberg Aria, piano"),
    ("Fantasy", "Chopin, Fantasy Op. 49, piano"),
    ("dso2006", "Shostakovich 5, orchestra, live"),
    ("Golf_Club", "Indie rock, 2022, mastered"),
    ("Track10", "Private Track10, mix at −13 LUFS, wide stereo"),
    ("Track_1", "Private Track_1, mix at −9 LUFS, loud master"),
    ("Limehouse", "Limehouse Blues, 1924 78 rpm"),
    ("JumpinJudy", "Prison work song, 1947 field recording"),
    ("Raven", "The Raven, read, produced"),
    ("Track11", "Private Track11, talk over soft music"),
    ("Leviticus", "Leviticus 13, read (mono)"),
    ("JFK", "JFK at Rice, 1962"),
    ("PotniskiVlak", "Train station, field recording"),
    ("PinkNoise", "Pink noise"),
    ("WhiteNoise", "White noise"),
]


def read(path):
    """setting -> kbit -> sound -> the line's fields, a sound being a made
    sound's kind or a recording's path under the recordings' folder, which
    every log names it by, so that two logs' lines meet and two recordings
    of one name in different folders do not; and the settings in the order
    the log has them."""
    data, settings = collections.defaultdict(lambda: collections.defaultdict(dict)), []
    for number, line in enumerate(open(path), 1):
        if not line.strip():
            continue
        try:
            f = json.loads(line)
        except json.JSONDecodeError as e:
            sys.exit(f"{path}:{number}: not a JSON object a line: {e}")
        sound = f["file"] if "file" in f else f["kind"]
        f["recording"] = "file" in f
        data[f["setting"]][int(f["kbit"])][sound] = f
        if f["setting"] not in settings:
            settings.append(f["setting"])
    return data, settings


def order(sounds):
    """Recordings in the order of RECORDINGS, any other after them."""
    return sorted(sounds, key=lambda name: next((i for i, (key, _) in enumerate(RECORDINGS) if key in file_name(name)), len(RECORDINGS)))


def recording_name(file):
    return next((label for key, label in RECORDINGS if key in file_name(file)), f"`{file}`")


def file_name(path):
    """A recording's file name without its folders, which RECORDINGS names it
    by: a folder's name may hold another recording's."""
    return SEPARATOR.split(path)[-1]


def colour(text, good):
    if good is None:
        return text
    return "$\\color{%s}{%s}$" % ("green" if good else "red", text)


def ratio(mine, theirs, lower_is_better=True):
    if mine is None or theirs is None or theirs == 0:
        return "—"
    text = f"{mine / theirs:.2f}"
    return colour(text, None if text == "1.00" else (mine < theirs) == lower_is_better)


def difference(mine, theirs, lower_is_better):
    if mine is None or theirs is None:
        return "—"
    text = f"{mine - theirs:+.2f}"
    return colour(text, None if text in ("+0.00", "-0.00") else (mine < theirs) == lower_is_better)


def us(v):
    return f"{v:.1f}" if v < 100 else f"{v:.0f}"


def kbit(f):
    """The rate the packets came to, in kbit/s: fifty packets a second."""
    return f["bytes"] * 8 * 50 / 1000


def summary(cells):
    """The recordings of one setting at one rate taken together."""
    files = [f for f in cells.values() if f["recording"]]
    if not files:
        return None
    seconds = sum(f["seconds"] for f in files)
    weighted = lambda key: sum(key(f) * f["seconds"] for f in files) / seconds
    mean = lambda field: (lambda values: sum(values) / len(values) if values else None)([f[field] for f in files if f[field] is not None])
    return {
        "us": weighted(lambda f: f["us"]),
        "p99": max(f["p99"] for f in files),
        "worst": max(f["worst"] for f in files),
        "kbit": weighted(kbit),
        "peak": max(f["peak_kbit"] for f in files),
        "p99_bytes": max(f["p99_bytes"] for f in files),
        "lsd": mean("lsd"),
        "lsd_side": mean("lsd_side"),
        "segsnr": mean("segsnr"),
        "celt": weighted(lambda f: f["celt"]),
        "fb": weighted(lambda f: f["fb"]),
        "seconds": seconds,
        "count": len(files),
    }


def number(v, places=2):
    return "—" if v is None else f"{v:.{places}f}"


def together(data, setting):
    rates = sorted(data[setting])
    first = summary(data[setting][rates[0]])
    if first is None:
        return
    print(f"### `{setting}`\n")
    print(f"{first['count']} recordings, {first['seconds'] / 60:.0f} minutes of sound. A block is 20 ms; *of one core* is the mean block over\n20 ms. The 99th-percentile and worst blocks are the worst recording's.\n*Peak* is the most any second of any recording came to.\n")
    print("| Rate, kbit/s | µs a block | Of one core | 99th, µs | Worst, µs | Came to, kbit/s | Peak, kbit/s | 99th packet, bytes | LSD, dB | Side LSD, dB | SegSNR, dB | CELT | Fullband |")
    print("|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|")
    for rate in rates:
        s = summary(data[setting][rate])
        print(f"| {rate} | {us(s['us'])} | {s['us'] / 20_000 * 100:.2f}% | {us(s['p99'])} | {us(s['worst'])} | {s['kbit']:.1f} | {s['peak']:.1f} | {s['p99_bytes']} | {number(s['lsd'])} | {number(s['lsd_side'])} | {number(s['segsnr'])} | {s['celt'] * 100:.0f}% | {s['fb'] * 100:.0f}% |")
    print()


def side_by_side(data, settings):
    rates = sorted({r for s in settings for r in data[s]})
    print("## The settings side by side\n")
    print("The recordings together, as in the tables of each setting below, a table\nfor each rate.\n")
    for rate in rates:
        print(f"### {rate} kbit/s\n")
        print("| Setting | µs a block | Of one core | 99th, µs | Came to, kbit/s | Peak, kbit/s | 99th packet, bytes | LSD, dB | Side LSD, dB | SegSNR, dB | CELT | Fullband |")
        print("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|")
        for setting in settings:
            s = summary(data[setting].get(rate, {}))
            if s is None:
                continue
            print(f"| `{setting}` | {us(s['us'])} | {s['us'] / 20_000 * 100:.2f}% | {us(s['p99'])} | {s['kbit']:.1f} | {s['peak']:.1f} | {s['p99_bytes']} | {number(s['lsd'])} | {number(s['lsd_side'])} | {number(s['segsnr'])} | {s['celt'] * 100:.0f}% | {s['fb'] * 100:.0f}% |")
        print()


def against(data, setting, reference):
    rates = [r for r in sorted(data[setting]) if r in data[reference]]
    print(f"### `{setting}` against `{reference}`\n")
    print("Time and bytes as this setting over the other; the proxies as this setting\nless the other.\n")
    print("| Rate, kbit/s | Time | 99th | Came to | Peak | 99th packet | LSD | Side LSD | SegSNR | CELT, this | CELT, other |")
    print("|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|")
    for rate in rates:
        a, b = summary(data[setting][rate]), summary(data[reference][rate])
        if a is None or b is None:
            continue
        print(f"| {rate} | {ratio(a['us'], b['us'])} | {ratio(a['p99'], b['p99'])} | {ratio(a['kbit'], b['kbit'])} | {ratio(a['peak'], b['peak'])} | {ratio(a['p99_bytes'], b['p99_bytes'])} | {difference(a['lsd'], b['lsd'], True)} | {difference(a['lsd_side'], b['lsd_side'], True)} | {difference(a['segsnr'], b['segsnr'], False)} | {a['celt'] * 100:.0f}% | {b['celt'] * 100:.0f}% |")
    print()


def before_table(data, settings, before):
    print("## Against the run before\n")
    print("The recordings taken together, this run's cell beside the run before's, where\nthe setting and rate are in both, over the recordings both have: the time a\nblock as this run over that one, the proxies as this run less that one.\n")
    print("| Setting | Rate, kbit/s | Before, µs | Now, µs | Time | Before, kbit/s | Now, kbit/s | LSD | Side LSD | SegSNR |")
    print("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|")
    for setting in settings:
        for rate in sorted(data[setting]):
            if rate not in before.get(setting, {}):
                continue
            # The recordings both runs have, so a difference is never one
            # of their inputs.
            both = data[setting][rate].keys() & before[setting][rate].keys()
            a = summary({sound: data[setting][rate][sound] for sound in both})
            b = summary({sound: before[setting][rate][sound] for sound in both})
            if a is None or b is None:
                continue
            print(f"| `{setting}` | {rate} | {us(b['us'])} | {us(a['us'])} | {ratio(a['us'], b['us'])} | {b['kbit']:.1f} | {a['kbit']:.1f} | {difference(a['lsd'], b['lsd'], True)} | {difference(a['lsd_side'], b['lsd_side'], True)} | {difference(a['segsnr'], b['segsnr'], False)} |")
    print()


GRIDS = [
    ("us", "µs a block, the least of the passes' mean", us),
    ("p99", "99th-percentile block, µs", us),
    ("kbit", "The rate the packets came to, kbit/s", lambda v: f"{v:.1f}"),
    ("peak_kbit", "The most any second came to, kbit/s", lambda v: f"{v:.1f}"),
    ("lsd", "Log-spectral distance, dB", number),
    ("lsd_side", "Log-spectral distance of the side, dB", number),
    ("segsnr", "Segmental SNR, dB", number),
    ("celt", "Packets that are CELT alone", lambda v: f"{v * 100:.0f}%"),
    ("fb", "Packets that are fullband", lambda v: f"{v * 100:.0f}%"),
]


def grids(data, setting, recordings):
    rates = sorted(data[setting])
    sounds = order({s for r in rates for s, f in data[setting][r].items() if f["recording"] == recordings})
    if not recordings:
        sounds = [k for k in KINDS if k in sounds] + sorted(s for s in sounds if s not in KINDS)
    if not sounds:
        return
    name = recording_name if recordings else (lambda s: s)
    for field, title, fmt in GRIDS:
        print(f"#### {title}\n")
        print(("| Recording | Seconds | " if recordings else "| Sound | ") + " | ".join(str(r) for r in rates) + " |")
        print("|---|" + ("---:|" if recordings else "") + "---:|" * len(rates))
        for sound in sounds:
            cells = []
            for rate in rates:
                f = data[setting][rate].get(sound)
                value = None if f is None else (kbit(f) if field == "kbit" else f[field])
                cells.append("—" if value is None else fmt(value))
            seconds = next((f"{data[setting][r][sound]['seconds']:.0f} | " for r in rates if sound in data[setting][r]), "— | ") if recordings else ""
            print(f"| {name(sound)} | {seconds}" + " | ".join(cells) + " |")
        print()


def main():
    args = sys.argv[1:]
    reference = before = None
    if "--against" in args:
        at = args.index("--against")
        reference = args[at + 1]
        del args[at:at + 2]
    if "--before" in args:
        at = args.index("--before")
        before, _ = read(args[at + 1])
        del args[at:at + 2]
    if len(args) != 1:
        sys.exit("usage: tables.py bench.jsonl [--against SETTING] [--before before.jsonl]")
    data, settings = read(args[0])
    if reference is not None and reference not in data:
        sys.exit(f"--against {reference}: no such setting in {args[0]}")
    if len(settings) > 1:
        side_by_side(data, settings)
    print("## The recordings together\n")
    for setting in settings:
        together(data, setting)
    if reference is not None:
        print("## Against `%s`\n" % reference)
        for setting in settings:
            if setting != reference:
                against(data, setting, reference)
    if before is not None:
        before_table(data, settings, before)
    main_setting = reference or settings[0]
    print(f"## The recordings one by one, `{main_setting}`\n")
    grids(data, main_setting, True)
    print(f"## The made sounds, `{main_setting}`\n")
    print("Fifty blocks of each; the time is one block's, the least of five passes.\n")
    grids(data, main_setting, False)


main()
