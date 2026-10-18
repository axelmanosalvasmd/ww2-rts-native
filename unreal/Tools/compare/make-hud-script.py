#!/usr/bin/env python3
"""Prepare an engine-input integration script from symbolic actions and explicit observed IDs.
This produces text only. It does not launch a client or send any command.
"""
import argparse
import json
import re
from pathlib import Path

p=argparse.ArgumentParser()
p.add_argument('recipe',type=Path)
p.add_argument('--header',type=Path,required=True)
p.add_argument('--ids',type=Path,help='Private JSON object of actual observed producer, job and plane IDs')
a=p.parse_args()
text=a.header.read_text()
body=re.search(r'enum class ETCUiAction\s*:\s*uint8\s*\{(.*?)\};',text,re.S).group(1)
body=re.sub(r'//[^\n]*','',body)
names=[v.strip() for v in body.split(',') if v.strip()]
assert all(re.fullmatch(r'[A-Za-z][A-Za-z0-9_]*',n) for n in names),'enum has changed; review action mapping'
values={name:i for i,name in enumerate(names)}
ids=json.loads(a.ids.read_text()) if a.ids else {}
steps=json.loads(a.recipe.read_text())
def value(v):
    if isinstance(v,str) and v.startswith('$'):
        assert v[1:] in ids,'missing observed ID '+v
        return ids[v[1:]]
    return v
out=[]
for s in steps:
    arg=s.get('arg','')
    if 'target' in s:
        t=s['target']
        arg='|'.join(map(str,[values[t['action']],value(t.get('unit','')),value(t.get('building',0)),value(t.get('job',0)),value(t.get('group',-1))]))
    assert not any(c in str(arg) for c in [';', '\n', '\r']), 'unsafe script separator'
    out.append(str(s.get('delay',.3))+('@'+s['condition'] if 'condition' in s else '')+':'+s['action']+(':'+str(arg) if arg!='' else ''))
print(';'.join(out))
