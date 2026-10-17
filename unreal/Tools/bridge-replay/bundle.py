#!/usr/bin/env python3
"""Great Bridge tick-8400 recipient bundle (release legacy-great-bridge-8400-efbd55c7): verify and cross-check.

  bundle.py verify <dir>    check every manifest asset's byte count and SHA-256 (exit 1 on any mismatch)
  bundle.py summary <dir>   an independent count of each stream (payloads, message types, snapshots, ticks) and the
                            tick-8400 packets compared native against source per seat as parsed JSON values

<dir> is the extracted great-bridge-8400 folder (manifest.json, packets/, streams/). Standard library only. This is a
cross-check for the Unreal automation test ThreeCrossroads.Net.LegacyBridgeReplay, not a decoder.
"""
import hashlib
import json
import os
import re
import sys

STREAMS = ["native-slot0", "native-slot1", "source-slot0", "source-slot1"]


def verify(d):
    m = json.load(open(os.path.join(d, "manifest.json")))
    ok = True
    for a in m["assets"]:
        data = open(os.path.join(d, a["path"]), "rb").read()
        h = hashlib.sha256(data).hexdigest()
        good = len(data) == a["bytes"] and h == a["sha256"]
        ok &= good
        print(f"{'ok  ' if good else 'BAD '} {a['path']} {len(data)} bytes sha256 {h}")
    for meta in ("README.md", "manifest.json"):
        print(f"meta {meta} sha256 {hashlib.sha256(open(os.path.join(d, meta), 'rb').read()).hexdigest()} (not in the asset manifest)")
    print("manifest: all assets verified" if ok else "manifest: MISMATCH")
    return 0 if ok else 1


def same(a, b):
    """JSON value equality with numbers compared by value (0 == 0.0) but booleans kept apart from numbers."""
    if isinstance(a, bool) or isinstance(b, bool):
        return type(a) is type(b) and a == b
    if isinstance(a, (int, float)) and isinstance(b, (int, float)):
        return float(a) == float(b)
    if isinstance(a, list) and isinstance(b, list):
        return len(a) == len(b) and all(same(x, y) for x, y in zip(a, b))
    if isinstance(a, dict) and isinstance(b, dict):
        return a.keys() == b.keys() and all(same(a[k], b[k]) for k in a)
    return type(a) is type(b) and a == b


def summary(d):
    packets = {}
    for s in STREAMS:
        types, ticks, payloads, bad = {}, [], 0, 0
        last = None
        with open(os.path.join(d, "streams", s + ".jsonl"), encoding="utf-8") as f:
            for line in f:
                line = line.rstrip("\n")
                payloads += 1
                try:
                    msg = json.loads(line)
                except ValueError:
                    bad += 1
                    continue
                t = msg.get("t")
                types[t] = types.get(t, 0) + 1
                if t == "s":
                    ticks.append(msg.get("tick"))
                    if msg.get("tick") == 8400:
                        last = (msg, line)
        gaps = sum(1 for a, b in zip(ticks, ticks[1:]) if b - a != 2)
        print(f"{s}: payloads {payloads}, unparsable {bad}, types {dict(sorted(types.items(), key=lambda kv: str(kv[0])))}, snapshots {len(ticks)}, ticks {ticks[0]}..{ticks[-1]}, gaps {gaps}")
        packets[s] = last
        raw = open(os.path.join(d, "packets", s + "-8400.json"), "rb").read()
        print(f"  packets/{s}-8400.json equals the stream's tick-8400 line byte for byte: {raw.rstrip(b'\\n') == last[1].encode('utf-8')}")
        m = last[0]
        shots = m.get("shots") or []
        kinds = [x.get("k") for x in shots]
        cells = m.get("cells") or []
        down = sum(1 for c in cells if len(c) > 4 and isinstance(c[4], dict) and isinstance(c[4].get("section"), dict)
                   and (c[4]["section"].get("state") == "failed" or c[4]["section"].get("hp") == 0))
        print(f"  tick 8400: shots {len(shots)} (say {kinds.count('say')} at {kinds.index('say') if 'say' in kinds else -1}, collapse {kinds.count('collapse')}),"
              f" cells {len(cells)} (5812 {any(c[0] == 5812 for c in cells)}, 5946 {any(c[0] == 5946 for c in cells)}, sections down {down}),"
              f" falling {len(m.get('falling') or [])}, wx {m.get('wx')}")
    for seat in (0, 1):
        n, sm = packets[f"native-slot{seat}"], packets[f"source-slot{seat}"]
        nm, so = n[0], sm[0]
        keys = sorted(set(nm) | set(so))
        differ = [k for k in keys if k not in nm or k not in so or not same(nm[k], so[k])]
        # numeric spellings in the raw cells arrays: tokens like 0.0 / 2400.0 against 0 / 2400
        def cells_text(line):
            i = line.index('"cells":')
            depth, j = 0, line.index("[", i)
            for k in range(j, len(line)):
                depth += line[k] == "["
                depth -= line[k] == "]"
                if depth == 0:
                    return line[j:k + 1]
        nums = lambda t: re.findall(r"(?<![\w.])-?\d+(?:\.\d+)?(?:[eE][-+]?\d+)?", t)
        nt, st = nums(cells_text(n[1])), nums(cells_text(sm[1]))
        spelled = sum(1 for a in nt if a.endswith(".0")) - sum(1 for a in st if a.endswith(".0"))
        print(f"seat {seat}: tick-8400 keys differing as parsed values native vs source: {differ or 'none'};"
              f" cells equal as values {same(nm.get('cells'), so.get('cells'))}; shots equal {same(nm.get('shots'), so.get('shots'))};"
              f" falling equal {same(nm.get('falling'), so.get('falling'))}; extra '.0' number tokens in native cells {spelled}")
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 3 or sys.argv[1] not in ("verify", "summary"):
        print(__doc__)
        sys.exit(2)
    sys.exit(verify(sys.argv[2]) if sys.argv[1] == "verify" else summary(sys.argv[2]))
