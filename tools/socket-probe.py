"""How long a write of sound blocks on a link too narrow for it, and what the
socket holds when it does: the measure examples/walk-sim.rs's socket is
modelled on (docs/walk.md).

    sudo python3 -I tools/socket-probe.py RATE_KBIT RTT_MS QUEUE_MS CC WMEM BYTES PERIOD_MS SECS [LOWAT]

Run as root on a Linux machine with nothing else on it (lab2): it makes three
network namespaces, a sender, a router and a browser, joined by veths, with
the bottleneck on the router's way to the browser — RATE_KBIT, a buffer
QUEUE_MS deep at that rate, and the whole RTT_MS on the way back — so the
sender's TCP sees it only through its acks, as it would a link's. The
sender's congestion control is CC and its tcp_wmem WMEM ("4096 16384
4194304" is Linux's default), and LOWAT, if given, its TCP_NOTSENT_LOWAT. It
writes BYTES every PERIOD_MS, one write at a time as zdesk's audio socket
does, each waiting for the last, for SECS, and then removes the namespaces.

One line of JSON: the settings, then when a write first blocked 20 ms or
more, the longest any did and how many did, how late what was written
reached the browser over the last half of the run (median and worst), and
what the socket held then (median, unsent and unacknowledged); then every
write as `rows`.
"""

import bisect
import fcntl
import json
import os
import socket
import struct
import subprocess
import sys
import termios
import time

SIOCOUTQNSD = 0x894B
TCP_NOTSENT_LOWAT = 25


def sh(cmd, check=True):
    subprocess.run(cmd, shell=True, check=check)


def namespaces(rate, rtt, queue_ms, cc, wmem):
    for ns in ("snd", "mid", "rcv"):
        sh(f"ip netns del {ns} 2>/dev/null", check=False)
        sh(f"ip netns add {ns} && ip -n {ns} link set lo up")
    sh("ip link add vs netns snd type veth peer name ms netns mid")
    sh("ip link add vr netns rcv type veth peer name mr netns mid")
    sh("ip -n snd addr add 10.9.1.1/24 dev vs && ip -n snd link set vs up && ip -n snd route add default via 10.9.1.2")
    sh("ip -n mid addr add 10.9.1.2/24 dev ms && ip -n mid link set ms up")
    sh("ip -n mid addr add 10.9.2.2/24 dev mr && ip -n mid link set mr up")
    sh("ip -n rcv addr add 10.9.2.1/24 dev vr && ip -n rcv link set vr up && ip -n rcv route add default via 10.9.2.2")
    sh("ip netns exec mid sh -c 'echo 1 > /proc/sys/net/ipv4/ip_forward'")
    for ns in ("snd", "rcv"):
        sh(f"ip netns exec {ns} sh -c 'echo {cc} > /proc/sys/net/ipv4/tcp_congestion_control; "
           f"echo \"{wmem}\" > /proc/sys/net/ipv4/tcp_wmem; echo \"4096 131072 6291456\" > /proc/sys/net/ipv4/tcp_rmem'")
    # Segments as the wire carries them, not offloaded into larger ones the
    # shaper would pass whole.
    for ns, dev in (("snd", "vs"), ("mid", "ms"), ("mid", "mr"), ("rcv", "vr")):
        sh(f"ip netns exec {ns} ethtool -K {dev} tso off gso off gro off >/dev/null 2>&1", check=False)
    sh(f"ip netns exec mid tc qdisc add dev mr root tbf rate {rate}kbit burst 1600 latency {queue_ms}ms")
    sh(f"ip netns exec mid tc qdisc add dev ms root netem delay {rtt}ms limit 10000")


def enter(ns):
    fd = os.open(f"/run/netns/{ns}", os.O_RDONLY)
    os.setns(fd, os.CLONE_NEWNET)
    os.close(fd)


def ioctl_int(sock, request):
    return struct.unpack("i", fcntl.ioctl(sock, request, b"\0\0\0\0"))[0]


def main():
    rate, rtt, queue_ms, cc, wmem, size, period, secs = sys.argv[1:9]
    lowat = int(sys.argv[9]) if len(sys.argv) > 9 else None
    rate, rtt, queue_ms = int(rate), float(rtt), float(queue_ms)
    size, period, secs = int(size), float(period) / 1e3, float(secs)
    namespaces(rate, rtt, queue_ms, cc, wmem)
    read, written = os.pipe()
    if os.fork() == 0:
        # The browser: reads everything as it comes, and stamps the count.
        os.close(read)
        enter("rcv")
        server = socket.socket()
        server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        server.bind(("10.9.2.1", 5000))
        server.listen(1)
        os.write(written, b"r")
        conn, _ = server.accept()
        got, stamps = 0, []
        while chunk := conn.recv(65536):
            got += len(chunk)
            stamps.append((time.monotonic(), got))
        os.write(written, json.dumps(stamps).encode())
        os.close(written)
        os._exit(0)
    os.close(written)
    os.read(read, 1)
    enter("snd")
    sender = socket.socket()
    sender.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
    if lowat is not None:
        sender.setsockopt(socket.IPPROTO_TCP, TCP_NOTSENT_LOWAT, lowat)
    sender.connect(("10.9.2.1", 5000))
    payload = b"\x55" * size
    rows, sent = [], 0
    start = time.monotonic()
    due = start
    while due - start < secs:
        now = time.monotonic()
        if now < due:
            time.sleep(due - now)
        before = time.monotonic()
        sender.sendall(payload)
        after = time.monotonic()
        sent += size
        rows.append({
            "t": round(before - start, 4), "due": round(due - start, 4), "blocked": round(after - before, 4), "sent": sent,
            "outq": ioctl_int(sender, termios.TIOCOUTQ), "notsent": ioctl_int(sender, SIOCOUTQNSD),
            "sndbuf": sender.getsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF),
        })
        # The next write is due a period on, or at once if this one ran past
        # it: the source does not wait, and what it would have queued
        # meanwhile is the gateway's to drop, not this probe's.
        due = due + period if after - due < period else after
    sender.close()
    stamps = json.loads(b"".join(iter(lambda: os.read(read, 1 << 16), b"")))
    os.wait()
    sh("ip netns del snd; ip netns del mid; ip netns del rcv", check=False)
    counts = [got for _, got in stamps]
    for row in rows:
        i = bisect.bisect_left(counts, row["sent"])
        row["arrived"] = round(stamps[i][0] - start, 4) if i < len(stamps) else None
    late = [r for r in rows if r["t"] >= secs / 2 and r["arrived"] is not None]
    delays = sorted((r["arrived"] - r["due"]) * 1e3 for r in late)
    blocks = [r["blocked"] * 1e3 for r in rows]
    first = next((r["t"] for r in rows if r["blocked"] >= 0.02), None)
    print(json.dumps({
        "rate": rate, "rtt": rtt, "queue_ms": queue_ms, "cc": cc, "wmem": wmem, "lowat": lowat,
        "bytes": size, "period_ms": period * 1e3, "secs": secs,
        "first_block": first, "longest_block_ms": round(max(blocks), 1), "blocks": sum(b >= 20 for b in blocks), "writes": len(rows),
        "late_p50_ms": round(delays[len(delays) // 2]), "late_max_ms": round(delays[-1]),
        "outq_p50": sorted(r["outq"] for r in late)[len(late) // 2],
        "notsent_p50": sorted(r["notsent"] for r in late)[len(late) // 2],
        "rows": rows,
    }))


if __name__ == "__main__":
    main()
