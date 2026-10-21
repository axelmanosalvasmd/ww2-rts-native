#!/usr/bin/env python3
"""Sequential integration-lead runner. Default writes the matrix; --execute launches native processes.

Use --retained-islands for the retained recipient stream and original three-pose tour,
or supply a direct native command JSON. Each lane keeps the workload and scopes its output paths.
"""
import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import subprocess
import sys
import time
import unittest

SCRATCH = Path(os.environ.get('TC_WORLD31_SCRATCH', Path.home() / '.cache/threecrossroads/round31-world'))
CASES = [('baseline', None), ('water-off', '-TCWorld31NoWater'),
         ('lights-capped', '-TCWorld31LightGridCap'), ('scatter-off', '-TCWorld31NoScatter'),
         ('shadows-off', '-TCWorld31NoShadows')]


def matrix(command, out):
    if not command or Path(command[0]).name not in ('UnrealEditor', 'ThreeCrossroads'):
        raise ValueError('Use a direct UnrealEditor or ThreeCrossroads command, not a driver/server wrapper')
    if any(x.startswith('-TCWorld31') for x in command):
        raise ValueError('Baseline already contains an isolation switch')
    if any(x.lower() == '-nullrhi' for x in command):
        raise ValueError('Bisection requires the real RHI')
    # Preserve launch arguments that define the workload. Native preference writes are isolated per process.
    base = [x for x in command if not x.lower().startswith(('-abslog=', '-userdir=', '-tcgraphicsat=',
        '-ini:gameusersettings:[threecrossroads]:graphics=', '-ini:gameusersettings:[threecrossroads]:graphicsautodropped='))]
    rows = []
    for name, flag in CASES:
        lane = out / name
        scoped = []
        for arg in base:
            match = re.match(r'^-(TC.*(?:ShotFile|ShotDir|ShotPrefix|Shots)|TCTourDir|Out|ReportExportPath)=(.*)$', arg, re.I)
            if match:
                key, value = match.groups()
                destination = lane / 'captures'
                if 'file' in key.lower() or 'prefix' in key.lower():
                    destination = destination / Path(value).name
                arg = '-' + key + '=' + str(destination)
            scoped.append(arg)
        args = scoped + ['-abslog=' + str(lane / 'native.log'), '-UserDir=' + str(lane / 'user') + '/',
                       '-TCGraphicsAt=low@0', '-ini:GameUserSettings:[ThreeCrossroads]:Graphics=low',
                       '-ini:GameUserSettings:[ThreeCrossroads]:GraphicsAutoDropped=1']
        if flag:
            args.append(flag)
        rows.append({'case': name, 'command': args})
    return rows


def result(code, log):
    oom = 'VK_ERROR_OUT_OF_DEVICE_MEMORY' in log
    fatal = any(x in log for x in ('Fatal error:', 'Assertion failed:', 'Caught signal 11', 'Signal 11'))
    started = 'LogTC: start #' in log or 'recorded presentation' in log.lower()
    return {'exit': code, 'deviceOOM': oom, 'fatal': fatal, 'matchOrPresentationMarker': started,
            'outerPoseMentioned': 'integration26-islands-outer' in log,
            'controlReceipt': 'world31 isolation:' in log,
            'classification': 'crash' if code != 0 or oom or fatal else 'clean-exit-review-workload'}


class RunnerTests(unittest.TestCase):
    def test_one_factor_and_workload(self):
        cmd = ['/engine/UnrealEditor', '/game.uproject', '-game', '-TCScene=Islands', '-TCExitAfter=180', '-TCShotFile=/retained/old.png', '-TCGraphicsAt=high@0']
        rows = matrix(cmd, Path('/evidence'))
        self.assertEqual(len(rows), 5)
        self.assertEqual([x for x in rows[0]['command'] if x.startswith('-TCWorld31')], [])
        for row, (_, flag) in zip(rows, CASES):
            self.assertEqual([x for x in row['command'] if x.startswith('-TCWorld31')], [flag] if flag else [])
            self.assertIn('-TCExitAfter=180', row['command'])
            self.assertIn('-TCScene=Islands', row['command'])
            self.assertNotIn('-TCGraphicsAt=high@0', row['command'])
            self.assertNotIn('-TCShotFile=/retained/old.png', row['command'])
            self.assertIn('-TCShotFile=/evidence/' + row['case'] + '/captures/old.png', row['command'])
        with self.assertRaises(ValueError):
            matrix(['/usr/bin/python3', 'driver.py'], Path('/evidence'))
        with self.assertRaises(ValueError):
            matrix(cmd + ['-nullrhi'], Path('/evidence'))

    def test_failures_are_not_clean_exits(self):
        self.assertEqual(result(-11, 'ClearBuffer NumCulledLightsGrid')['classification'], 'crash')
        self.assertEqual(result(0, 'VK_ERROR_OUT_OF_DEVICE_MEMORY')['classification'], 'crash')
        self.assertEqual(result(0, 'Fatal error: device lost')['classification'], 'crash')
        self.assertEqual(result(0, '')['classification'], 'clean-exit-review-workload')
        self.assertFalse(result(0, '')['matchOrPresentationMarker'])


def main():
    if '--self-test' in sys.argv:
        unittest.main(argv=[sys.argv[0]])
        return 0
    p = argparse.ArgumentParser(description=__doc__)
    source = p.add_mutually_exclusive_group(required=True)
    source.add_argument('--command-json', type=Path)
    source.add_argument('--retained-islands', action='store_true')
    p.add_argument('--out', required=True, type=Path)
    p.add_argument('--execute', action='store_true')
    p.add_argument('--timeout', type=float, default=240)
    a = p.parse_args()
    a.out = a.out.resolve()
    a.out.mkdir(parents=True, exist_ok=False)
    replay = None
    if a.retained_islands:
        repo = Path(__file__).resolve().parents[4]
        retained = Path(os.environ['TC_WORLD31_RETAINED'])
        stream = retained / 'inputs/retained-streams/islands/host-recipient.jsonl'
        driver = retained / 'run/replay-clock.mjs'
        for item in (stream, driver):
            if not item.is_file():
                raise ValueError('Missing retained input: ' + str(item))
        (a.out / 'inputs.json').write_text(json.dumps([{'path': str(x),
            'sha256': hashlib.sha256(x.read_bytes()).hexdigest()} for x in (stream, driver)], indent=2))
        tour = a.out / 'islands.tour'
        tour.write_text('integration26-islands-overview p2 512 512 1024 -65\n'
                        'integration26-islands-bank p2 400 380 90 -65\n'
                        'integration26-islands-outer p2 512 0 150 -90\n')
        port = '{PORT}'
        if a.execute:
            SCRATCH.mkdir(parents=True, exist_ok=True)
            # Integration lead only. EOF closes this owned replay without process signals.
            wrapper = "process.stdin.resume(); process.stdin.on('end',()=>process.exit(0)); await import(process.env.TC_REPLAY31_URL);"
            replay = subprocess.Popen(['node', '--input-type=module', '--eval', wrapper, '--',
                '--stream', str(stream), '--port-file', str(a.out / 'port'), '--hold-tick', '999999',
                '--log', str(a.out / 'replay.log')], stdin=subprocess.PIPE,
                env=os.environ | {'TMPDIR': str(SCRATCH), 'TC_REPLAY31_URL': driver.as_uri()})
            deadline = time.monotonic() + 20
            while not (a.out / 'port').exists():
                if replay.poll() is not None or time.monotonic() > deadline:
                    replay.stdin.close()
                    raise RuntimeError('Owned recorded replay did not become ready')
                time.sleep(.1)
            port = (a.out / 'port').read_text().strip()
            if not port.isdecimal():
                replay.stdin.close()
                raise ValueError('Recorded replay returned an invalid port')
        raw = [str(Path(os.environ['UE']) / 'Engine/Binaries/Linux/UnrealEditor'),
            str(repo / 'unreal/ThreeCrossroads.uproject'), '-game', '-RenderOffscreen',
            '-ResX=1280', '-ResY=720', '-unattended', '-nosplash', '-nosound',
            '-TCServer=ws://127.0.0.1:' + port + '/ws', '-TCRoom=r31world', '-TCName=Fern',
            '-TCTourFile=' + str(tour), '-TCTourDir=' + str(a.out), '-TCExitAfter=90',
            '-TCLang=en', '-TCFogTime=0', '-TCWaterTime=37', '-TCLookTime=0',
            '-TCTourWaitShaders', '-TCTourWaitWorld']
        if a.execute:
            os.environ['VK_DRIVER_FILES'] = '/usr/share/vulkan/icd.d/intel_icd.json'
            os.environ['TC_DECODER'] = 'rust'
    else:
        raw = json.loads(a.command_json.read_text())
    rows = matrix(raw['command'] if isinstance(raw, dict) else raw, a.out)
    (a.out / 'matrix.json').write_text(json.dumps(rows, indent=2) + '\n')
    if not a.execute:
        print('Prepared 5 sequential lanes. No process launched.')
        return 0
    results = []
    for row in rows:
        lane = a.out / row['case']
        lane.mkdir()
        (lane / 'captures').mkdir()
        task_tmp = SCRATCH / row['case']
        task_tmp.mkdir(parents=True, exist_ok=True)
        env = os.environ.copy()
        env['TMPDIR'] = str(task_tmp)
        t0 = time.monotonic()
        with (lane / 'console.log').open('w') as f:
            proc = subprocess.Popen(row['command'], env=env, stdout=f, stderr=subprocess.STDOUT)
            (lane / 'pid.json').write_text(json.dumps({'pid': proc.pid, 'command': row['command']}, indent=2))
            try:
                code = proc.wait(timeout=a.timeout)
            except subprocess.TimeoutExpired:
                # Never advance while the old GPU process is alive. Lead owns any cleanup decision.
                (a.out / 'timeout.json').write_text(json.dumps({'case': row['case'], 'pid': proc.pid,
                    'remainingLanesNotStarted': True, 'action': 'Lead must stop or wait for this exact process before resuming.'}, indent=2))
                if replay:
                    replay.stdin.close()
                return 2
        log = (lane / 'native.log').read_text(errors='replace') if (lane / 'native.log').exists() else ''
        r = {'case': row['case'], 'seconds': round(time.monotonic() - t0, 2), **result(code, log)}
        results.append(r)
        (a.out / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
        print(json.dumps(r), flush=True)
    if replay:
        replay.stdin.close()
        replay.wait(timeout=10)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
