#!/usr/bin/env python3
"""Integration lead only: one Vulkan lane, no build, server or process signals.
Prepare by default. --execute launches the supplied native command and samples its DRM clients.
"""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import time

ROOT = Path(os.environ.get('TMPDIR', str(Path.home() / '.cache/threecrossroads/renderer34')))
LAYER = Path(os.environ.get('UE', str(Path.home() / 'Applications/UnrealEngine-5.8.3'))) / 'Engine/Binaries/ThirdParty/Vulkan/Linux'
CASES = {'baseline': [], 'full-resolution': ['-TCRenderer34FullResolution'],
         'high-relief': ['-TCRenderer34HighRelief'],
         'weather-mirror-off': ['-TCRenderer34NoWeatherMirror'],
         'occlusion-off': ['-ExecCmds=r.AllowOcclusionQueries 0,r.HZBOcclusion 0,stat memory,stat VulkanMemory,stat VulkanMemoryRaw']}


def drm_sample(pid):
    clients = {}
    for file in (Path('/proc') / str(pid) / 'fdinfo').glob('*'):
        try:
            fields = dict(re.findall(r'^(drm-[\w-]+):\s*(.*?)\s*$', file.read_text(), re.M))
        except (OSError, ValueError):
            continue
        if 'drm-client-id' not in fields:
            continue
        # Multiple descriptors of the same DRM client report identical allocations.
        key = fields.get('drm-pdev', '') + ':' + fields['drm-client-id']
        clients[key] = fields
    return clients


def command(base, case, out, production=False):
    if Path(base[0]).name not in ('UnrealEditor', 'ThreeCrossroads'):
        raise ValueError('Supply a direct native command')
    if any(x.lower() == '-nullrhi' or x.startswith('-TCRenderer34') for x in base):
        raise ValueError('Supply a rendered baseline without renderer34 switches')
    if any(x.lower().startswith('-execcmds=') for x in base):
        raise ValueError('Move existing ExecCmds into the workload before preparing this diagnostic')
    args = [x for x in base if not x.lower().startswith(('-abslog=', '-userdir=', '-tcgraphicsat=',
            '-ini:gameusersettings:[threecrossroads]:graphics=',
            '-ini:gameusersettings:[threecrossroads]:graphicsautodropped='))]
    args = [('-TCTourDir=' + str(out / 'captures')) if x.lower().startswith('-tctourdir=') else x for x in args]
    args += ['-abslog=' + str(out / 'native.log'), '-UserDir=' + str(out / 'user') + '/',
             '-TCGraphicsAt=low@0', '-ini:GameUserSettings:[ThreeCrossroads]:Graphics=low',
             '-ini:GameUserSettings:[ThreeCrossroads]:GraphicsAutoDropped=1']
    if not production:
        args += ['-TCRenderer34Memory', '-vulkandebug', '-vulkandebugutils']
    if case != 'occlusion-off' and not production:
        args += ['-ExecCmds=stat memory,stat VulkanMemory,stat VulkanMemoryRaw']
    lane = CASES[case]
    if production and case == 'occlusion-off':
        lane = ['-ExecCmds=r.AllowOcclusionQueries 0,r.HZBOcclusion 0']
    return args + lane


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--command-json', type=Path, required=True)
    p.add_argument('--case', choices=CASES, default='baseline')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--execute', action='store_true')
    p.add_argument('--production', action='store_true', help='DRM sampling only, without validation, stats or RHI inventories')
    p.add_argument('--room', help='Fresh live room, 3 to 12 alphanumeric characters')
    p.add_argument('--timeout', type=float, default=300)
    a = p.parse_args()
    out = a.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    raw = json.loads(a.command_json.read_text())
    base = raw.get('command') if isinstance(raw, dict) else raw
    if a.room:
        if not re.fullmatch(r'[A-Za-z0-9]{3,12}', a.room):
            raise ValueError('Invalid room')
        base = [x for x in base if not x.lower().startswith('-tcroom=')] + ['-TCRoom=' + a.room]
    args = command(base, a.case, out, a.production)
    env_patch = {'VK_LAYER_PATH': str(LAYER), 'VK_INSTANCE_LAYERS': 'VK_LAYER_KHRONOS_validation',
                 'VK_DRIVER_FILES': '/usr/share/vulkan/icd.d/intel_icd.json', 'TMPDIR': str(ROOT / a.case)}
    if a.production:
        env_patch.pop('VK_LAYER_PATH')
        env_patch.pop('VK_INSTANCE_LAYERS')
    (out / 'captures').mkdir()
    (out / 'launch.json').write_text(json.dumps({'command': args, 'environment': env_patch}, indent=2) + '\n')
    if not a.execute:
        print('Prepared one lane. No Unreal process launched.')
        return
    if not a.production and not (LAYER / 'VkLayer_khronos_validation.json').is_file():
        raise ValueError('Engine validation manifest missing')
    Path(env_patch['TMPDIR']).mkdir(parents=True, exist_ok=True)
    t0 = time.monotonic()
    with (out / 'console.log').open('x') as log, (out / 'gpu-memory.jsonl').open('x') as samples:
        proc = subprocess.Popen(args, env=os.environ | env_patch, stdout=log, stderr=subprocess.STDOUT)
        while proc.poll() is None:
            samples.write(json.dumps({'seconds': time.monotonic()-t0, 'pid': proc.pid,
                                      'drmClients': drm_sample(proc.pid)}) + '\n')
            samples.flush()
            if time.monotonic()-t0 > a.timeout:
                (out / 'timeout.json').write_text(json.dumps({'pid': proc.pid, 'stillRunning': True,
                    'action': 'Lead must wait or stop this process before any next GPU run.'}))
                return 2
            time.sleep(1)
    text = (out / 'native.log').read_text(errors='replace') if (out / 'native.log').exists() else ''
    console = (out / 'console.log').read_text(errors='replace')
    lines = (text + '\n' + console).splitlines()
    failures = [i for i, line in enumerate(lines) if re.search(
        r'VUID-|Validation Error|OUT_OF_DEVICE_MEMORY|failed, VkResult|Caught signal|Fatal error', line)]
    excerpts = ['\n'.join(lines[max(0, i-3):i+5]) for i in failures[:20]]
    (out / 'failure-context.txt').write_text('\n\n'.join(excerpts))
    memory_rows = [tuple(map(float, row)) for row in re.findall(
        r'memory sample=\d+ monotonic=([\d.]+).*?localUsed=(\d+) localBudget=(\d+)', text)]
    memory_summary = {}
    if memory_rows:
        memory_summary = {'firstUsed': memory_rows[0][1], 'lastUsed': memory_rows[-1][1],
                          'peakUsed': max(r[1] for r in memory_rows),
                          'minBudget': min(r[2] for r in memory_rows),
                          'samples': len(memory_rows)}
    result = {'memory': memory_summary, 'exit': proc.returncode, 'seconds': time.monotonic()-t0,
              'failureMarkers': len(failures), 'nativeMemorySamples': text.count('memory sample='),
              'validationMentioned': 'VK_LAYER_KHRONOS_validation' in text + console,
              'acceptance': 'Lead must confirm 120 live match seconds and stable memory; exit alone is insufficient.'}
    (out / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result))
    return 0 if proc.returncode == 0 and not failures else 1


if __name__ == '__main__':
    raise SystemExit(main())
