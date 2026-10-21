#!/usr/bin/env python3
"""Execute production presentation arithmetic against pinned main.js formulas."""
from pathlib import Path
import json
import os
import re
import subprocess

repo = Path(__file__).resolve().parents[4]
scratch = Path(os.environ.get('TC_UNITS31_SCRATCH', Path.home() / '.cache/threecrossroads/round31-units'))
scratch.mkdir(parents=True, exist_ok=True)
cpp = scratch / 'presentation.cpp'
cpp.write_text(r'''
#include "Client/TCUnits31Presentation.h"
#include <iostream>
#include <iomanip>
#include <limits>
#include <cassert>
int main() {
 std::cout << std::setprecision(17);
 TCUnits31::FSnapshotEase E;
 for (double T : {1., 1.2, 1.7, 1.71}) {
  E.Arrived(T); std::cout << E.GapMs << ' ' << E.Factor(.05) << '\n';
 }
 for (double W : {5.06, -3., std::numeric_limits<double>::quiet_NaN()})
  std::cout << TCUnits31::Waterline(-2.5, W) << '\n';
 TCUnits31::FGroundPose P;
 P.Update(10,10,0,2,true,.05,[](double X,double Z){return .5*X+.25*Z;});
 std::cout << P.Pitch << ' ' << P.Roll << '\n';
 P.Update(10,10,0,2,true,.05,[](double,double){return 0.;});
 std::cout << P.Pitch << ' ' << P.Roll << '\n';
 assert(!P.Update(10,10,0,2,true,.05,[](double,double){return std::numeric_limits<double>::quiet_NaN();}));
 assert(P.Suspended);
 P.Update(10,10,0,2,true,.05,[](double,double){return 0.;});
 assert(P.Pitch==0 && P.Roll==0 && !P.Suspended);
 P.Update(10,10,0,2,true,.05,[](double X,double){return X>10?1.:0.;});
 assert(std::isfinite(P.Lift) && P.Lift>=0);
}
''')
binary = scratch / 'presentation'
subprocess.run(['c++', '-std=c++17', '-Wall', '-Wextra', '-Werror', '-I', str(repo/'unreal/Source/ThreeCrossroads'), str(cpp), '-o', str(binary)], check=True)
actual = [float(x) for x in subprocess.check_output([str(binary)], text=True).split()]
source = (repo/'reference/ww2-rts/client/main.js').read_text()
assert 'Math.max(60, Math.min(400, arrived - snapshotAt))' in source
assert 'Math.exp(-sdt * 1000 / snapshotGap)' in source
assert 'Math.max(ground, relief?.waterAt(v.x, v.z) || ground)' in source
gap = re.search(r'snapshotGap \+= (.*?);', source).group(1)
factor = re.search(r'const k = (.*?), ranges', source).group(1)
waterline = 'Math.max(ground, relief?.waterAt(v.x, v.z) || ground)'
js = '''let snapshotAt=0,snapshotGap=100,out=[],sdt=.05;
for(const arrived of [1000,1200,1700,1710]) {
 if(snapshotAt) snapshotGap+=GAP;
 snapshotAt=arrived; out.push(snapshotGap,FACTOR);
}
const ground=-2.5,v={x:1,z:1};
for(const water of [5.06,-3,NaN]) { const relief={waterAt:()=>water}; out.push(WATERLINE); }
console.log(JSON.stringify(out));'''.replace('GAP',gap).replace('FACTOR',factor).replace('WATERLINE',waterline)
expected = json.loads(subprocess.check_output(['node','--input-type=module','-e',js], text=True))
pitch = 'Math.atan2(1.6,3.2)'
expected += json.loads(subprocess.check_output(['node','-e',f'const p={pitch},r=-Math.atan(.25*Math.cos(p)),b=1-Math.exp(-.05*12);console.log(JSON.stringify([p,r,p*(1-b),r*(1-b)]))'],text=True))
errors = [(i,a,e) for i,(a,e) in enumerate(zip(actual,expected)) if abs(a-e)>1e-12]
assert len(actual)==len(expected)
print(json.dumps({'actual':actual,'source':expected,'mismatches':errors}))
if errors:
 raise SystemExit(1)
print('PASS: delivered cadence, 60/400 ms clamps, easing, waterline and dry fallback')
