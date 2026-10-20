#!/usr/bin/env python3
"""Export the original Controls dialog text without changing its translation keys."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--check', action='store_true')
args = parser.parse_args()
root = Path(__file__).resolve().parents[3]
reference = root / 'reference/ww2-rts'
pin = subprocess.check_output(['git', '-C', str(reference), 'rev-parse', 'HEAD'], text=True).strip()
if pin != 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d':
    raise SystemExit('Controls export requires the pinned reference e4e579a')
source = reference / 'client/index.html'
html = re.search(r'<dialog id="controlsSheet".*?</dialog>', source.read_text(), re.S).group()
sections = [(title, re.findall(r'<dt>(.*?)</dt><dd>(.*?)</dd>', body, re.S))
            for title, body in re.findall(r'<section><h3>(.*?)</h3><dl>(.*?)</dl></section>', html, re.S)]
assert len(sections) == 4 and sum(len(rows) for _, rows in sections) == 42

def quote(value):
    return 'TEXT(' + json.dumps(value, ensure_ascii=False) + ')'

lines = ['// Generated from read-only client/index.html #controlsSheet at e4e579a.',
         '// Reproduce with unreal/Tools/compare/export-controls.py. Source SHA256: ' + hashlib.sha256(source.read_bytes()).hexdigest(),
         'static const TArray<FSection> SourceSections = {']
for title, rows in sections:
    lines.append(' { ' + quote(title) + ', {')
    lines.extend('  { ' + quote(keys) + ', ' + quote(description) + ' },' for keys, description in rows)
    lines.append(' } },')
lines.append('};')
expected = '\n'.join(lines) + '\n'
target = root / 'unreal/Source/ThreeCrossroads/Client/TCHUD26ControlsData.inl'
if args.check:
    if target.read_text() != expected:
        raise SystemExit('Controls text differs; run unreal/Tools/compare/export-controls.py')
else:
    target.write_text(expected)
print('Original Controls: 4 sections, 42 rows' + (' verified' if args.check else ' exported'))
