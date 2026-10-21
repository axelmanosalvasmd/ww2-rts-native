#!/usr/bin/env python3
"""Lead-run validation-free Vulkan diagnostics. Preparation is the default.

No game signals, builds, server changes or live debugger attachment. Core extraction
and GDB run after a fatal exit. Temporary core files are private and removed.
"""
import argparse
import fcntl
import json
import os
from pathlib import Path
import re
import resource
import shutil
import subprocess
import tempfile
import time

SCRATCH = Path('/home/loiko/.hermes/cache/scratch/round35-renderer')
CASES = {
    'baseline': [],
    'transient-off': ['r.RDG.TransientAllocator 0'],
    'parallel-off': ['r.RDG.ParallelExecute 0'],
    'barriers-explicit': ['r.Vulkan.UseMemoryBarrierOpt 0'],
    'staging-locks': ['r.Vulkan.ForceStagingBufferOnLock 1'],
    'pool-400': ['r.Streaming.PoolSize 400'],
}


def environment(parent, scratch):
    # --production in the previous runner still inherited validation environment.
    env = {k: v for k, v in parent.items() if not k.startswith((
        'VK_LAYER', 'VK_INSTANCE_LAYER', 'VK_LOADER_LAYER', 'ANV_DEBUG', 'INTEL_DEBUG',
        'MESA_VK_', 'MESA_VULKAN_'))}
    env.update(TMPDIR=str(scratch), VK_LOADER_LAYERS_DISABLE='*',
               VK_DRIVER_FILES='/usr/share/vulkan/icd.d/intel_icd.json')
    return env


def command(base, case, out, production=False):
    if not isinstance(base, list) or not base or not all(isinstance(x, str) for x in base):
        raise ValueError('Expected a direct native argument array')
    if Path(base[0]).name not in ('UnrealEditor', 'ThreeCrossroads'):
        raise ValueError('Supply the direct native executable, not a wrapper')
    for arg in base:
        low = arg.lower()
        if low == '-nullrhi' or low.startswith(('-vulkandebug', '-vulkanvalidation', '-tcrender', '-execcmds=')):
            raise ValueError('Baseline must have no validation, instrumentation or ExecCmds: ' + arg)
    args = [x for x in base if not x.lower().startswith((
        '-abslog=', '-userdir=', '-tcgraphicsat=',
        '-ini:gameusersettings:[threecrossroads]:graphics=',
        '-ini:gameusersettings:[threecrossroads]:graphicsautodropped='))]
    args = ['-TCTourDir=' + str(out / 'captures') if x.lower().startswith('-tctourdir=') else x for x in args]
    args += ['-abslog=' + str(out / 'native.log'), '-UserDir=' + str(out / 'user') + '/',
             '-TCGraphicsAt=low@0', '-ini:GameUserSettings:[ThreeCrossroads]:Graphics=low',
             '-ini:GameUserSettings:[ThreeCrossroads]:GraphicsAutoDropped=1']
    if not production:
        args.append('-TCRenderer35Memory')
    if CASES[case]:
        args.append('-ExecCmds=' + ','.join(CASES[case]))
    return args


def parse_drm(rows):
    clients = {}
    for fields in rows:
        if 'drm-client-id' in fields:
            clients[fields.get('drm-pdev', '') + ':' + fields['drm-client-id']] = fields
    totals = {}
    for fields in clients.values():
        for key, value in fields.items():
            if not key.startswith(('drm-memory-', 'drm-total-')):
                continue
            match = re.fullmatch(r'(\d+)\s*(B|KiB|MiB|GiB)?', value)
            if match:
                scale = {None: 1, 'B': 1, 'KiB': 1024, 'MiB': 1024**2, 'GiB': 1024**3}[match[2]]
                totals[key] = totals.get(key, 0) + int(match[1]) * scale
    # Keep resident/allocated counters separate. They describe overlapping bytes.
    return {'clients': clients, 'bytes': totals or None}


def drm_sample(pid):
    rows = []
    for file in (Path('/proc') / str(pid) / 'fdinfo').glob('*'):
        try:
            rows.append(dict(re.findall(r'^(drm-[\w-]+):\s*(.*?)\s*$', file.read_text(), re.M)))
        except OSError:
            pass
    return parse_drm(rows)


def process_start(pid):
    try:
        return (Path('/proc') / str(pid) / 'stat').read_text().rsplit(')', 1)[1].split()[19]
    except OSError:
        return None


def postmortem(pid, executable, out, scratch):
    if not shutil.which('coredumpctl') or not shutil.which('gdb'):
        (out / 'core-status.txt').write_text('Missing coredumpctl or gdb; no backtrace collected.\n')
        return
    with (out / 'core-info.txt').open('w') as log:
        subprocess.run(['coredumpctl', '--no-pager', 'info', str(pid)], stdout=log, stderr=subprocess.STDOUT)
    # systemd may still be compressing the dump after waitpid returns.
    with tempfile.TemporaryDirectory(prefix='core-', dir=scratch) as private:
        core = Path(private) / 'game.core'
        with (out / 'core-extract.txt').open('w') as log:
            for attempt in range(10):
                done = subprocess.run(['coredumpctl', '--no-pager', 'dump', str(pid), '--output=' + str(core)],
                                      stdout=log, stderr=subprocess.STDOUT)
                if done.returncode == 0 and core.exists() and core.stat().st_size:
                    break
                time.sleep(2)
            else:
                (out / 'core-status.txt').write_text('Core unavailable. See core-extract.txt; no stack invented.\n')
                return
        with (out / 'gdb-all-threads.txt').open('w') as log:
            result = subprocess.run(['gdb', '-q', '-nx', '-batch', executable, str(core),
                '-ex', 'set pagination off', '-ex', 'set debuginfod enabled off',
                '-ex', 'info sharedlibrary', '-ex', 'info threads',
                '-ex', 'thread apply all bt full'], stdout=log, stderr=subprocess.STDOUT)
        (out / 'core-status.txt').write_text(f'GDB exit {result.returncode}; extracted core removed.\n')


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--command-json', type=Path, required=True)
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--case', choices=CASES, default='baseline')
    p.add_argument('--room', required=True)
    p.add_argument('--production', action='store_true', help='DRM and postmortem only, no native inventory')
    p.add_argument('--execute', action='store_true')
    p.add_argument('--timeout', type=float, default=360)
    a = p.parse_args()
    if not re.fullmatch(r'[A-Za-z0-9]{3,12}', a.room):
        p.error('Room must be 3 to 12 alphanumeric characters')
    raw = json.loads(a.command_json.read_text())
    base = raw['command'] if isinstance(raw, dict) else raw
    base = [x for x in base if not x.lower().startswith('-tcroom=')] + ['-TCRoom=' + a.room]
    out = a.out.resolve()
    args = command(base, a.case, out, a.production)
    env = environment(os.environ, SCRATCH)
    out.mkdir(parents=True, exist_ok=False)
    (out / 'captures').mkdir()
    (out / 'launch.json').write_text(json.dumps({'command': args,
        'environment': {k: env[k] for k in ('TMPDIR', 'VK_LOADER_LAYERS_DISABLE', 'VK_DRIVER_FILES')},
        'removedEnvironment': sorted(set(os.environ) - set(env))}, indent=2) + '\n')
    if not a.execute:
        print('Prepared. No engine process launched.')
        return 0
    SCRATCH.mkdir(parents=True, exist_ok=True)
    with (SCRATCH / 'gpu.lock').open('w') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        active = SCRATCH / 'active.json'
        if active.exists():
            previous = json.loads(active.read_text())
            if previous['start'] is not None and process_start(previous['pid']) == previous['start']:
                raise RuntimeError('Previous diagnostic game is still alive; lead must resolve it before another run')
        soft, hard = resource.getrlimit(resource.RLIMIT_CORE)
        if hard == 0:
            raise RuntimeError('Core hard limit is zero. Lead must launch from a shell with cores permitted')
        resource.setrlimit(resource.RLIMIT_CORE, (hard, hard))
        (out / 'core-config.json').write_text(json.dumps({'before': [soft, hard],
            'enabled': list(resource.getrlimit(resource.RLIMIT_CORE)),
            'pattern': Path('/proc/sys/kernel/core_pattern').read_text().strip()}) + '\n')
        t0 = time.monotonic()
        # A relative kernel core filename stays under the authorized scratch root.
        with (out / 'console.log').open('x') as log, (out / 'gpu-memory.jsonl').open('x') as samples:
            proc = subprocess.Popen(args, env=env, cwd=SCRATCH, stdout=log, stderr=subprocess.STDOUT)
            active.write_text(json.dumps({'pid': proc.pid, 'start': process_start(proc.pid)}))
            while proc.poll() is None:
                samples.write(json.dumps({'seconds': round(time.monotonic() - t0, 3),
                                          'pid': proc.pid, **drm_sample(proc.pid)}) + '\n')
                samples.flush()
                if time.monotonic() - t0 > a.timeout:
                    (out / 'result.json').write_text(json.dumps({'pid': proc.pid, 'timeout': True,
                        'stillRunning': True, 'acceptance': 'FAIL, no signal sent; resolve PID before next GPU run'}) + '\n')
                    return 2
                time.sleep(1)
        active.unlink(missing_ok=True)
        native = (out / 'native.log').read_text(errors='replace') if (out / 'native.log').exists() else ''
        console = (out / 'console.log').read_text(errors='replace')
        combined = native + '\n' + console
        failures = re.findall(r'OUT_OF_DEVICE_MEMORY|Fatal error|Caught signal|Validation Error', combined)
        rows = [json.loads(x) for x in (out / 'gpu-memory.jsonl').read_text().splitlines()]
        keys = sorted({k for row in rows for k in (row['bytes'] or {})})
        memory = {k: {'first': next(row['bytes'][k] for row in rows if k in (row['bytes'] or {})),
                      'last': next(row['bytes'][k] for row in reversed(rows) if k in (row['bytes'] or {})),
                      'peak': max((row['bytes'] or {}).get(k, 0) for row in rows)} for k in keys}
        result = {'pid': proc.pid, 'exit': proc.returncode, 'seconds': time.monotonic() - t0,
                  'memory': memory, 'failureMarkers': len(failures),
                  'validationMentioned': 'VK_LAYER_KHRONOS_validation' in combined,
                  'acceptance': 'Lead must verify 180 active match seconds, native exit 0 and flat DRM after warm-up.'}
        (out / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
        if proc.returncode in (-11, -6) or 'Caught signal 11' in combined:
            postmortem(proc.pid, args[0], out, SCRATCH)
        print(json.dumps(result))
        return 0 if proc.returncode == 0 and not failures else 1


if __name__ == '__main__':
    raise SystemExit(main())
