#!/usr/bin/env python3
"""Mark parity rows ported, citing the merged test PR and its tests.

Usage: ww2-status-flip.py <parity-file> <pr>:<ROW,ROW,...> [<pr>:<ROWS> ...]
Reads each PR body with gh, finds lines "- ROW-ID: `test`; `test`", sets the
row's Status cell to ported and appends "Native intent test (#pr): `test`..."
to its Acceptance check cell. Prints one line per row; exits 1 if any row
was not found or had no tests in the PR body.
"""
import json, re, subprocess, sys

REPO = "axelmanosalvasmd/ww2-rts-native"


def tests_by_row(pr):
    body = json.loads(subprocess.check_output(
        ["gh", "pr", "view", pr, "--repo", REPO, "--json", "body"]))["body"]
    out = {}
    for line in body.splitlines():
        m = re.match(r"^\s*(?:[-*]\s*)?([A-Z]+-\d+):\s*(.*)$", line)
        if not m:
            continue
        names = re.findall(r"`([^`]+)`", m.group(2))
        if names and m.group(1) not in out:
            out[m.group(1)] = names
    return out


def main():
    path, specs = sys.argv[1], sys.argv[2:]
    lines = open(path).read().split("\n")
    bad = False
    for spec in specs:
        pr, rows = spec.split(":")
        tests = tests_by_row(pr)
        for row in rows.split(","):
            names = tests.get(row)
            idx = next((i for i, l in enumerate(lines)
                        if l.startswith(f"| {row} |")), None)
            if idx is None or not names:
                print(f"{row}: NOT FLIPPED (row found={idx is not None}, tests={bool(names)})")
                bad = True
                continue
            cells = lines[idx].split("|")
            # cells: ['', ID, Behavior, Evidence, Rust, Unreal, Depends, Acceptance, Status, '']
            cite = f" Native intent test (#{pr}): " + "; ".join(f"`{n}`" for n in names) + ". "
            cells[-3] = cells[-3].rstrip() + cite
            cells[-2] = " ported "
            lines[idx] = "|".join(cells)
            print(f"{row}: ported via #{pr} ({len(names)} tests)")
    open(path, "w").write("\n".join(lines))
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
