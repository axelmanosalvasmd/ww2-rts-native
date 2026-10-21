#!/usr/bin/env python3
"""Offline stock DumpGPU lineage/texels and RDG transition receipts.

No engine launch, inferred missing bytes, surface attribution or acceptance.
Sample coordinates address the exact resource, never display pixels implicitly.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import re
import struct


def receipt(path):
    path = Path(path)
    with path.open('rb') as stream:
        digest = hashlib.file_digest(stream, 'sha256').hexdigest()
    return dict(path=str(path.resolve()), bytes=path.stat().st_size, sha256=digest)


def sequence(path):
    # Stock DumpGPU appends JSON objects, rather than emitting a JSON array.
    text, decoder, offset, values = Path(path).read_text(), json.JSONDecoder(), 0, []
    while offset < len(text):
        while offset < len(text) and text[offset].isspace():
            offset += 1
        if offset == len(text):
            break
        value, offset = decoder.raw_decode(text, offset)
        if not isinstance(value, dict):
            raise ValueError('expected concatenated DumpGPU objects')
        values.append(value)
    return values


def graph(root):
    root = Path(root)
    if (root / 'Base/Status.txt').read_text().strip() != 'ok':
        raise ValueError('incomplete GPU dump')
    passes = sequence(root / 'Base/Passes.json')
    descs = sequence(root / 'Base/ResourceDescs.json')
    if not passes or not descs:
        raise ValueError('absent intended graph/resources')
    for field in ('Id', 'Pointer'):
        if len({p[field] for p in passes}) != len(passes):
            raise ValueError('ambiguous pass ' + field)
    if len({d['UniqueResourceName'] for d in descs}) != len(descs):
        raise ValueError('ambiguous resource pointer')
    return passes, descs


def index(root):
    passes, descs = graph(root)
    return dict(kind='descriptor-index', rawTexels=False, passes=passes, resources=descs,
                inputs=[receipt(Path(root) / 'Base' / n) for n in ('Status.txt', 'Passes.json', 'ResourceDescs.json')],
                binaryFiles=[dict(name=p.name, bytes=p.stat().st_size) for p in sorted((Path(root) / 'Resources').glob('*.bin'))],
                limits='Descriptors and binary presence are not sampled values, runtime success or visibility acceptance.')


def layout(desc, subresource):
    match = re.fullmatch(r'(.+?)(?:\.\[(\d+)\])?\.(mip(\d+)|stencil)', subresource)
    if not match or match[1] != desc['UniqueResourceName']:
        raise ValueError('exact resource subresource required')
    if desc.get('Desc') != 'FRDGTextureDesc' or desc.get('Type') not in ('Texture2D', 'Texture2DArray'):
        raise ValueError('unsupported texture layout; no buffer/cube/3D interpretation')
    if desc['NumSamples'] != 1 or desc['Depth'] != 1:
        raise ValueError('unsupported multisample or volume layout')
    is_array = desc['Type'] == 'Texture2DArray'
    array_slice = int(match[2]) if match[2] is not None else None
    if (is_array and (array_slice is None or not 0 <= array_slice < desc['ArraySize'])) or (not is_array and (array_slice is not None or desc['ArraySize'] != 1)):
        raise ValueError('explicit valid array slice required only for arrays')
    stencil = match[3] == 'stencil'
    mip = 0 if stencil else int(match[4])
    if not 0 <= mip < desc['NumMips']:
        raise ValueError('mip outside resource')
    if stencil and desc['Format'] != 'PF_DepthStencil':
        raise ValueError('stencil requires the actual depth-stencil resource')
    # Engine 5.8 DumpGPU.cpp TranslateSubresourceDumpDesc. Unsupported formats stay unavailable.
    original = desc['Format']
    converted = 'PF_R8_UINT' if stencil else ('PF_R32_FLOAT' if original in ('PF_DepthStencil', 'PF_ShadowDepth', 'PF_D24') else original)
    formats = {
        'PF_R32_FLOAT': ('<f', ['R']), 'PF_FloatRGBA': ('<4e', ['R', 'G', 'B', 'A']),
        'PF_A32B32G32R32F': ('<4f', ['R', 'G', 'B', 'A']),
        'PF_B8G8R8A8': ('<4B', ['B', 'G', 'R', 'A']), 'PF_R8G8B8A8': ('<4B', ['R', 'G', 'B', 'A']),
        'PF_R8_UINT': ('<B', ['R']), 'PF_R32_UINT': ('<I', ['R']),
    }
    if converted not in formats:
        raise ValueError('unsupported converted format ' + converted)
    width, height = desc['ExtentX'] >> mip, desc['ExtentY'] >> mip
    if width <= 0 or height <= 0:
        raise ValueError('invalid mip extent')
    return dict(width=width, height=height, mip=mip, arraySlice=array_slice, stencil=stencil,
                originalFormat=original, readbackFormat=converted, structFormat=formats[converted][0],
                storedChannels=formats[converted][1], conversion='DumpGPU.cpp TranslateSubresourceDumpDesc (Engine 5.8)')


def sample(root, subresource, consumer_pointer, x, y, origin):
    root = Path(root)
    passes, descs = graph(root)
    readers = [p for p in passes if p['Pointer'] == consumer_pointer]
    if len(readers) != 1 or subresource not in readers[0]['InputResources']:
        raise ValueError('exact consumer must consume the requested subresource')
    reader = readers[0]
    writers = [p for p in passes if p['Id'] < reader['Id'] and subresource in p['OutputResources']]
    if not writers:
        raise ValueError('no captured preceding writer; external initial version is unavailable')
    writer = max(writers, key=lambda p: p['Id'])
    desc = next((d for d in descs if subresource.startswith(d['UniqueResourceName'] + '.')), None)
    if desc is None:
        raise ValueError('resource descriptor absent')
    dimensions = layout(desc, subresource)
    width, height = dimensions['width'], dimensions['height']
    if origin not in ('top-left', 'bottom-left') or not (1 <= x < width - 1 and 1 <= y < height - 1):
        raise ValueError('explicit storage origin and interior resource 3x3 coordinates required')
    name = subresource + '.v' + writer['Pointer'] + '.bin'
    if '/' in name or '\\' in name:
        raise ValueError('resource version must be a filename')
    binary = root / 'Resources' / name
    if not binary.is_file():
        raise ValueError('last writer binary is absent; descriptors or earlier versions are not samples')
    stride = struct.calcsize(dimensions['structFormat'])
    if binary.stat().st_size != width * height * stride:
        raise ValueError('binary byte count disagrees with exact subresource extent/converted format')
    rows = []
    with binary.open('rb') as stream:
        for dy in (-1, 0, 1):
            for dx in (-1, 0, 1):
                resource_y = y + dy
                stored_y = resource_y if origin == 'top-left' else height - 1 - resource_y
                offset = (stored_y * width + x + dx) * stride
                stream.seek(offset)
                raw = stream.read(stride)
                values = list(struct.unpack(dimensions['structFormat'], raw))
                if not all(math.isfinite(v) for v in values):
                    raise ValueError('nonfinite selected texel')
                rows.append(dict(resourceXY=[x + dx, resource_y], storedRow=stored_y,
                                 byteOffset=offset, rawHex=raw.hex(), values=values))
    return dict(kind='raw-versioned-3x3', resource=desc, subresource=subresource,
                writer=writer, reader=reader, extent=[width, height], storageOrigin=origin,
                coordinateSpace='resource texels; display/view/light projection must be established separately',
                **{k: v for k, v in dimensions.items() if k not in ('width', 'height', 'structFormat')},
                binary=receipt(binary), samples=rows,
                graphInputs=[receipt(root / 'Base' / n) for n in ('Status.txt', 'Passes.json', 'ResourceDescs.json')],
                runtimeValidated=False, visibilityAccepted=False,
                limits='Actual stored channels only. Reverse-Z depth is not metres. No surface/caster identity or source alignment follows from texels alone.')


def role_exit(value, role):
    if isinstance(value, dict):
        if value.get('timeout') is True:
            raise ValueError('exact runtime role timed out')
        if 'exits' in value:
            return role_exit(value['exits'].get(role), role)
        if role in value:
            return role_exit(value[role], role)
        value = value.get('exit')
    if type(value) is not int:
        raise ValueError('exact runtime role exit is absent or malformed')
    return value


def require_clean_exit(value, role):
    if role_exit(value, role) != 0:
        raise ValueError('exact runtime role failed')


def runtime(manifest, dump):
    manifest = Path(manifest)
    data = json.loads(manifest.read_text())
    pinned = {}
    for field in ('log', 'command', 'process', 'exit'):
        expected = data[field]
        actual = receipt(expected['path'])
        if actual['sha256'] != expected['sha256']:
            raise ValueError('runtime ' + field + ' hash differs')
        pinned[field] = actual
    require_clean_exit(json.loads(Path(pinned['exit']['path']).read_text()), data['role'])
    process = json.loads(Path(pinned['process']['path']).read_text())
    command = json.loads(Path(pinned['command']['path']).read_text())
    if type(process.get('pid')) is not int or not process.get('identity', {}).get('startTicks'):
        raise ValueError('exact PID/startTicks identity required')
    if Path(command[0]).resolve() != Path(process['identity']['exe']).resolve():
        raise ValueError('command and process executable disagree')
    log_path = Path(pinned['log']['path'])
    if '-abslog=' + str(log_path) not in command or '-TCRender26GpuDir=' + str(Path(dump).resolve()) not in command:
        raise ValueError('command does not bind this log and GPU dump')
    text = log_path.read_text(errors='replace')
    adapters = re.findall(r'LogRHI:\s+Name:\s+([^\r\n]+)', text)
    if not adapters or set(adapters) != {data['adapter']}:
        raise ValueError('actual selected RHI adapter differs or is absent')
    marker = 'look camera: ' + data['pose'] + ' camera '
    if marker not in text or not re.search(r'GPU request pose ' + re.escape(data['pose']) + r' frame \d+ binary 1 handled 1 path ' + re.escape(str(Path(dump).resolve())) + r' filter ', text):
        raise ValueError('intended pose and handled binary dispatch are required')
    return dict(manifest=receipt(manifest), files=pinned, identity=process, pose=data['pose'], adapter=data['adapter'],
                completed=True, limits='Exact retained role/process receipts. No physical input or fresh runtime was performed by this offline reader.')


def transitions(log):
    log = Path(log)
    text = log.read_text(errors='replace')
    current, resource_link, links, requests = None, None, [], []
    for line_number, line in enumerate(text.splitlines(), 1):
        if 'LogTCRender28: request ' in line:
            requests.append(dict(line=line_number, value=json.loads(line.split('LogTCRender28: request ', 1)[1])))
        if 'LogRDG:' not in line:
            continue
        frame = re.match(r'\[[^\]]+\]\[\s*(\d+)\]', line)
        log_frame = int(frame[1]) if frame else None
        header = re.search(r'LogRDG:\s+Display:\s+\[(.+)\(Index: (\d+), Pipeline: ([^)]+)\): Batch: \(([^)]+)\)', line)
        if header:
            current = dict(passName=header[1], passIndex=int(header[2]), pipeline=header[3], batchPointer=header[4], headerLine=line_number, logFrame=log_frame)
            resource_link = None
        resource = re.search(r'RDG\(((?:0x)?[0-9a-fA-F]+)\) RHI\(((?:0x)?[0-9a-fA-F]+)\) (.+)', line)
        if resource and current and current['logFrame'] == log_frame:
            resource_link = dict(**current, rdgPointer=resource[1], rhiPointer=resource[2],
                                 resourceTransition=resource[3], line=line_number, rawLine=line, subresourceTransitions=[])
            links.append(resource_link)
        subresource = re.search(r'Mip\((\d+)\), Array\((\d+)\), Slice\((\d+)\): (.+)', line)
        if subresource and resource_link and resource_link['logFrame'] == log_frame:
            resource_link['subresourceTransitions'].append(dict(mip=int(subresource[1]), array=int(subresource[2]),
                                                               planeSlice=int(subresource[3]), transition=subresource[4],
                                                               line=line_number, rawLine=line))
    return dict(kind='actual-transition-lines', log=receipt(log), requests=requests, links=links,
                failedResourceIdentified=False, newRuntime=False,
                limits='RDG/RHI pointers and pass indices are local to this process/frame. A pass index is not a DumpGPU Id. No Vulkan buffer/image/view range or fault association is inferred. Zero links is unavailable, not success.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='mode', required=True)
    idx = sub.add_parser('index')
    idx.add_argument('--dump', type=Path, required=True)
    raw = sub.add_parser('sample')
    raw.add_argument('--dump', type=Path, required=True)
    raw.add_argument('--subresource', required=True)
    raw.add_argument('--consumer', required=True)
    raw.add_argument('--x', type=int, required=True)
    raw.add_argument('--y', type=int, required=True)
    raw.add_argument('--origin', choices=('top-left', 'bottom-left'), required=True)
    raw.add_argument('--runtime', type=Path, required=True, help='Hash-pinned exact-role log/command/process/exit manifest')
    raw.add_argument('--layout-source', type=Path, required=True, help='Installed Engine 5.8 DumpGPU.cpp for conversion provenance')
    tr = sub.add_parser('transitions')
    tr.add_argument('--log', type=Path, required=True)
    for command in (idx, raw, tr):
        command.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    if args.mode == 'sample':
        evidence = runtime(args.runtime, args.dump)
        result = sample(args.dump, args.subresource, args.consumer, args.x, args.y, args.origin)
        result.update(runtime=evidence, runtimeValidated=True, layoutSource=receipt(args.layout_source), newRuntime=False)
    elif args.mode == 'index':
        result = index(args.dump)
    else:
        result = transitions(args.log)
    with args.out.open('x') as output:
        json.dump(result, output, indent=2, allow_nan=False)
        output.write('\n')


if __name__ == '__main__':
    main()
