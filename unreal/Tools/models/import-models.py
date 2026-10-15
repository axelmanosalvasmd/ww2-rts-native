"""Import the reference game's models into Unreal: meshes, textures and the master materials that reproduce their look.

Run inside the editor. The project's own C++ module is not needed, so a content-only host project works as well:

  UnrealEditor-Cmd unreal/ThreeCrossroads.uproject -run=pythonscript -EnablePlugins=PythonScriptPlugin -unattended -nullrhi \
    -script="unreal/Tools/models/import-models.py --manifest unreal/Saved/ModelExport/manifest.json"

and afterwards, also inside the editor: --verify REPORT.json (asset checks), --render DIR (offscreen previews; needs a
renderer: -AllowCommandletRendering -RenderOffscreen instead of -nullrhi). Without Unreal, the engine-free checks
(reference tables, manifest, glb normalization) run with any Python 3.9+:

  python3 unreal/Tools/models/import-models.py --selftest [--manifest ...]

Input: the manifest of unreal/Tools/models/export-reference-models.mjs (format ww2-reference-models-v1) and its .glb
files: the reference's own builders run under Node, in the reference's model space (metres, +x forward, +y up, +z right).
The importer reads the shader attributes from their custom glTF names (_MATID, _MODELUV, _UV, _OWNERTINT, _VEHICLETINT,
_COLOR) and the material description from each glTF material's extras.ww2. A smaller bootstrap format
(tc-model-export-v1: models[] with key, type, faction, file, tc:* material names) is read too.

Output under /Game/Models (unreal/Content/Models):
  Textures/   TA_ModelLayers (the model texture layers as one Texture2DArray, MATS order), the infantry atlas, the
              structure detail texture, the surface textures (brick drawn by a port of surfaces.js brickCanvas), maps
  Materials/  M_TCModel, M_TCModelTwoSided, M_TCModelFaded, M_TCModelFadedTwoSided, M_TCSurface, one instance per
              reference material (MI_TC<slot>, and MI_TC<slot>_Faded)
  Meshes/<type>/SM_<key>_<part>: one static mesh per moving part; hierarchy kept as rest transforms and sockets
  ModelCatalog.json      read by Client/TCModelLibrary: models, parts, rest transforms, sockets, materials, colors
  ImportProvenance.json  source glb sha256 -> normalized glb sha256 -> asset -> .uasset sha256, textures, materials

Vertex layout of every imported mesh (the contract the generated materials read):
  COLOR_0  linear base color / scale; the materials read it linear (Interchange's glTF factory converts it so the
           static mesh builder's sRGB encoding round-trips: 8 bit, linear steps)
  UV0      the infantry atlas coordinates (modelUV.xy), else the reference uv (structures: x = surface id)
  UV1      x: matId as the reference writes it (layer, -1 plain, -2 unset, plus grime / 2); y: modelUV.z
  UV2      x: ownerTint, y: vehicleTint: color = base + owner * x + vehicle * y (client/models/blender-baked.js)
  UV3      x: color scale (base colors above 1, such as the uniform multiplier on the infantry atlas); y: 0
Per-instance custom data (TCModelLibrary.h TCModelData): 0..2 owner color, 3..5 vehicle paint (both linear),
6 fade (1 solid; the Faded materials multiply it by their own Fade, e.g. the propeller disc's 0.08), 7 damage (1 = the
reference's wreck).
"""
import argparse
import hashlib
import json
import math
import os
import re
import shutil
import struct
import subprocess
import sys
import zlib
from array import array

try:
    import unreal  # only inside the editor
except ImportError:
    unreal = None

TOOL_VERSION = 2
HERE = os.path.dirname(os.path.abspath(__file__))
PROJECT = os.path.dirname(os.path.dirname(HERE))  # unreal/
REPO = os.path.dirname(PROJECT)
REFERENCE = os.path.join(REPO, 'reference', 'ww2-rts')
CONTENT_ROOT = '/Game/Models'
UV_CHANNELS = 4

# ------------------------------------------------------------------------------------------------ reference tables
# Copies of the reference's tables. --selftest and every import parse the pinned sources and stop on a difference.

# client/models/geom.js
MATS = ['armor-paint', 'cast-armor', 'gunmetal', 'track-steel', 'rubber', 'wood', 'canvas', 'wool', 'leather',
        'aluminum', 'aircraft-paint', 'mud', 'asphalt', 'concrete']
PLAIN, UNSET = -1, -2
SIZE = 512
MUD_TEXELS = 420

# client/model-textures.js LAYERS
LAYERS = {
    'armor-paint': dict(texels=240, strength=0.35, hue=0, fade=0.06, roughness=0.68, metalness=0, relief=0.006, file='armor-paint-refined.jpg'),
    'cast-armor': dict(texels=280, strength=0.3, hue=0.02, fade=0.08, roughness=0.8, metalness=0, relief=0.012),
    'gunmetal': dict(texels=480, strength=0.35, hue=0.03, fade=0.02, film=0.35, roughness=0.42, metalness=0.85, relief=0.002),
    'track-steel': dict(texels=420, strength=0.65, hue=0.12, fade=0, film=0.35, roughness=0.73, metalness=0.7, relief=0.006),
    'rubber': dict(texels=480, strength=0.3, hue=0, fade=0.08, film=0.35, roughness=0.95, metalness=0, relief=0.004),
    'wood': dict(texels=600, strength=0.6, hue=0.3, fade=0.06, roughness=0.84, metalness=0, relief=0.006),
    'canvas': dict(texels=1400, strength=0.25, hue=0.05, fade=0.06, roughness=0.94, metalness=0, relief=0.004),
    'wool': dict(texels=3000, strength=0.18, hue=0, fade=0.06, roughness=1, metalness=0, relief=0.002),
    'leather': dict(texels=1600, strength=0.18, hue=0.12, fade=0, film=0.25, roughness=0.8, metalness=0, relief=0.002),
    'aluminum': dict(texels=280, strength=0.28, hue=0, fade=0, roughness=0.38, metalness=1, relief=0.001),
    'aircraft-paint': dict(texels=240, strength=0.33, hue=0, fade=0.06, roughness=0.6, metalness=0, relief=0.002),
    'mud': dict(texels=380, strength=1, hue=1, fade=0, roughness=1, metalness=0, relief=0.015),
    'asphalt': dict(texels=900, strength=0.22, hue=0, fade=0, roughness=0.96, metalness=0, relief=0.003, file='../road.jpg'),
    'concrete': dict(texels=500, strength=0.16, hue=0, fade=0, roughness=0.92, metalness=0, relief=0.002, file='../plaster.jpg'),
}
RIM = (0.26, 0.29, 0.34)  # uModelRim, linear

# client/unit-models.js: the three shared paints (MeshStandardMaterial roughness and metalness, the modelMaterial default
# layer, grime) and LOOKS. The bootstrap format names them tc:paint, tc:soldier, tc:vehicle; the exporter by label.
PAINTS = {
    'paint': dict(label='unit-models.js PAINT', roughness=0.85, metalness=0, mat='armor-paint', grime=True, atlas=False),
    'soldier': dict(label='unit-models.js INFANTRY_PAINT', roughness=0.9, metalness=0, mat='wool', grime=True, atlas=True),
    'vehicle': dict(label='unit-models.js VEHICLE_PAINT', roughness=0.68, metalness=0, mat='armor-paint', grime=True, atlas=False),
}
LOOKS = {
    'vehicle': dict(mat='armor-paint', mud=0.75, dust=0.18, most=0.9),
    'naval': dict(mat='armor-paint', mud=0, dust=0.03, most=0.1),
    'gun': dict(mat='armor-paint', mud=0.22, dust=0.12, most=0.7),
    'soldier': dict(mat='wool', mud=0.3, dust=0.2, most=0.6),
    'structure': dict(mat='armor-paint', mud=0.6, dust=0.1, most=0.8),
}
LOOK_PAINT = {'vehicle': 'vehicle', 'naval': 'vehicle', 'gun': 'paint', 'soldier': 'soldier', 'structure': 'paint'}
WRECK = 0x1d1b18      # client/main.js wreckMaterial (Lambert)
CAMO_OPACITY = 0.45   # client/main.js, flags & 256
GHOST_OPACITY = 0.35  # client/main.js applyGhosts
# client/structures.js MODEL_MAT: metres per detail repeat by surface id (the shader's perM is its inverse)
STRUCTURE_PER_METRE = (0.42, 0.8, 0.45, 0.5, 2.0, 0.5)

# client/surfaces.js SURF (brick is drawn by brickCanvas)
SURFACES = {
    'plaster': dict(tex='plaster', size=6, flat=0xb8a888, tint=(0.78, 0.76, 0.74), top=(0.5, 0.47, 0.44)),
    'hedge': dict(tex='hedge', size=1.6, flat=0x3f5a2a, tint=(1, 1.35, 1.15), top=(1.15, 1.15, 1)),
    'brick': dict(tex=None, size=2.4, flat=0x8a5545, tint=(1, 1, 1), top=(0.7, 0.68, 0.66)),
    'stone': dict(tex='stone', size=2, flat=0x9a958a, tint=(1.1, 1.15, 1.2), top=(1.1, 1.1, 1.1)),
    'sandbag': dict(tex='sandbag', size=1, flat=0x9c8a60, tint=(1.2, 1.3, 1.3), top=(1.1, 1.1, 1.1)),
    'burlap': dict(tex='burlap', size=0.5, flat=0x9c8a60, tint=(0.82, 0.78, 0.68), top=(1.05, 1.05, 1.05)),
    'canvas': dict(tex='canvas', size=2.4, flat=0x5f6440, tint=(0.8, 0.8, 0.7), top=(1.08, 1.08, 1.04)),
    'straw': dict(tex='burlap', size=1.4, flat=0xa89060, tint=(0.92, 0.82, 0.58), top=(1.05, 1.05, 1.05)),
    'rubble': dict(tex='rubble', size=2, flat=0x8a8070, tint=(1, 1.15, 1.25), top=(1.05, 1.05, 1.05)),
    'wood': dict(tex='planks', size=1.2, flat=0x7a5a3a, tint=(1, 0.85, 0.7), top=(1, 1, 1)),
    'darkwood': dict(tex='planks', size=1.2, flat=0x5a4028, tint=(0.6, 0.48, 0.38), top=(1, 1, 1)),
    'steel': dict(tex='steel', size=1.5, flat=0x4f4f4c, tint=(0.9, 1, 1.1), top=(1.1, 1.1, 1.1)),
    'earth': dict(tex='earth', size=2, flat=0x6b5a3e, tint=(2, 2.3, 2.3), top=(1, 1, 1)),
}

# per-instance custom data slots (TCModelLibrary.h TCModelData)
CPD_OWNER, CPD_VEHICLE, CPD_FADE, CPD_DAMAGE, CPD_COUNT = 0, 3, 6, 7, 8
NEUTRAL = 0xdddddd  # client/main.js look(): COLORS[slot] ?? 0xdddddd


def srgb_to_linear(c):
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def hex_linear(h):
    return tuple(srgb_to_linear(((h >> s) & 255) / 255.0) for s in (16, 8, 0))


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for chunk in iter(lambda: f.read(1 << 20), b''):
            h.update(chunk)
    return h.hexdigest()


def log(msg):
    if unreal:
        unreal.log('[import-models] ' + msg)
    else:
        print('[import-models] ' + msg, flush=True)


def warn(msg):
    if unreal:
        unreal.log_warning('[import-models] ' + msg)
    else:
        print('[import-models] WARNING ' + msg, flush=True)


def safe(s):
    return re.sub(r'[^A-Za-z0-9]+', '_', str(s)).strip('_') or 'x'


def rel_or_abs(path):
    path = os.path.abspath(path)
    return os.path.relpath(path, REPO) if path.startswith(REPO + os.sep) else path


def write_json(path, value):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, 'w', encoding='utf-8') as f:
        json.dump(value, f, indent=1, sort_keys=True)
        f.write('\n')


# ------------------------------------------------------------------------------------------------ reference parity

def _js_object_literal(text, name):
    m = re.search(r'(?:export )?const ' + re.escape(name) + r' = \{\n(.*?)\n\};', text, re.S)
    if not m:
        raise ValueError('table %s not found' % name)
    rows = {}
    for line in m.group(1).split('\n'):
        r = re.match(r"\s*'?([\w-]+)'?: \{ (.*) \},?(\s*//.*)?$", line)
        if not r:
            continue
        fields = {}
        for k, v in re.findall(r"(\w+): (\[[^\]]*\]|'[^']*'|[^,]+)", r.group(2)):
            v = v.strip()
            if v.startswith("'"):
                fields[k] = v.strip("'")
            elif v.startswith('['):
                fields[k] = tuple(float(x) for x in v.strip('[]').split(','))
            elif v.startswith('0x'):
                fields[k] = int(v, 16)
            elif re.match(r'^-?[\d.]+$', v):
                fields[k] = float(v)
            else:
                fields[k] = v
        rows[r.group(1)] = fields
    return rows


def reference_parity(reference=REFERENCE):
    """Compare the tables above with the pinned reference. Returns a list of differences (empty when they agree)."""
    bad = []
    rd = lambda p: open(os.path.join(reference, p), encoding='utf-8').read()
    geom, tex, um, surf, main, st = rd('client/models/geom.js'), rd('client/model-textures.js'), rd('client/unit-models.js'), rd('client/surfaces.js'), rd('client/main.js'), rd('client/structures.js')
    mats = re.findall(r"'([\w-]+)'", re.search(r'export const MATS = \[(.*?)\];', geom).group(1))
    if mats != MATS:
        bad.append('MATS differ: %r' % mats)
    if not re.search(r'export const PLAIN = -1, UNSET = -2;', geom):
        bad.append('PLAIN/UNSET changed')
    if 'const SIZE = %d;' % SIZE not in tex or 'const MUD = { texels: %d };' % MUD_TEXELS not in tex:
        bad.append('SIZE or MUD texels changed')
    layers = _js_object_literal(tex, 'LAYERS')
    for name, want in LAYERS.items():
        got = layers.get(name)
        if got is None:
            bad.append('layer %s missing' % name)
            continue
        for k in set(want) | set(got):
            a, b = want.get(k), got.get(k)
            if isinstance(a, str) or isinstance(b, str):
                if a != b:
                    bad.append('layer %s.%s: %r vs %r' % (name, k, a, b))
            elif a is None or b is None or abs(float(a) - float(b)) > 1e-12:
                bad.append('layer %s.%s: %r vs %r' % (name, k, a, b))
    if set(layers) != set(LAYERS):
        bad.append('LAYERS keys differ: %r' % sorted(set(layers) ^ set(LAYERS)))
    if 'new THREE.Color(%s, %s, %s) }' % RIM not in tex:
        bad.append('uModelRim changed')
    looks = _js_object_literal(um, 'LOOKS')
    for name, want in LOOKS.items():
        got = looks.get(name, {})
        for k, v in want.items():
            if (got.get(k) != v) if isinstance(v, str) else abs(float(got.get(k, -99)) - v) > 1e-12:
                bad.append('LOOKS.%s.%s: %r vs %r' % (name, k, v, got.get(k)))
    for var, key in (('PAINT', 'paint'), ('INFANTRY_PAINT', 'soldier'), ('VEHICLE_PAINT', 'vehicle')):
        m = re.search(r'export const ' + var + r' = .*?roughness: ([\d.]+), metalness: ([\d.]+) \}\), \{ mat: \'([\w-]+)\', grime: (true|false) \}', um)
        p = PAINTS[key]
        if not m or (float(m.group(1)), float(m.group(2)), m.group(3), m.group(4) == 'true') != (p['roughness'], p['metalness'], p['mat'], p['grime']):
            bad.append('%s changed: %r' % (var, m and m.groups()))
    if 'new THREE.MeshLambertMaterial({ color: 0x%06x })' % WRECK not in main:
        bad.append('wreck material changed')
    if 'o.material.opacity = flags & 256 ? %s : 1' % CAMO_OPACITY not in main or 'opacity: %s, depthWrite: false' % GHOST_OPACITY not in main:
        bad.append('camouflage or ghost opacity changed')
    if 'float perM = id < 1.5 ? %s : id < 2.5 ? %s : id < 3.5 ? %s : id < 4.5 ? %s : id < 5.5 ? %s : %s;' % STRUCTURE_PER_METRE not in st:
        bad.append('structures.js MODEL_MAT detail scales changed')
    surfs = _js_object_literal(surf, 'SURF')
    for name, want in SURFACES.items():
        got = surfs.get(name)
        if got is None:
            bad.append('surface %s missing' % name)
            continue
        if want['tex'] is not None and got.get('tex') != want['tex']:
            bad.append('surface %s tex %r' % (name, got.get('tex')))
        if want['tex'] is None and got.get('draw') != 'brickCanvas':
            bad.append('surface %s is no longer drawn by brickCanvas' % name)
        for k in ('size', 'flat'):
            if abs(float(got.get(k, -1)) - want[k]) > 1e-12:
                bad.append('surface %s.%s %r' % (name, k, got.get(k)))
        for k in ('tint', 'top'):
            if tuple(got.get(k, ())) != tuple(float(x) for x in want[k]):
                bad.append('surface %s.%s %r' % (name, k, got.get(k)))
    if set(surfs) != set(SURFACES):
        bad.append('SURF keys differ: %r' % sorted(set(surfs) ^ set(SURFACES)))
    return bad


def reference_factions(reference=REFERENCE):
    """client/main.js FACTIONS (uniform, vehicle) and the seat COLORS."""
    main = open(os.path.join(reference, 'client/main.js'), encoding='utf-8').read()
    factions = [dict(name=m[0], uniform=int(m[1], 16), vehicle=int(m[2], 16))
                for m in re.findall(r"\{ name: '(\w+)', uniform: (0x[0-9a-f]+), vehicle: (0x[0-9a-f]+)", main)]
    colors = [int(x.strip(), 16) for x in re.search(r'const COLORS = \[([^\]]+)\]', main).group(1).split(',')]
    return factions, colors


def reference_commit(reference=REFERENCE):
    try:
        return subprocess.run(['git', '-C', reference, 'rev-parse', 'HEAD'], capture_output=True, text=True, check=True).stdout.strip()
    except Exception:
        return 'unknown'


_GRIME = {}


def label_grime(label, reference=REFERENCE):
    """Whether a reference material bakes grime: its modelMaterial(..., { grime: true }) call, read from the source
    file the exporter's label names ('unit-models.js VEHICLE_PAINT' -> client/unit-models.js)."""
    if label in _GRIME:
        return _GRIME[label]
    result = False
    m = re.match(r'([\w-]+\.js) (\w+)$', label or '')
    if m:
        path = os.path.join(reference, 'client', m.group(1))
        if os.path.exists(path):
            line = re.search(r'const ' + m.group(2) + r' = (.*)', open(path, encoding='utf-8').read())
            result = bool(line and 'modelMaterial(' in line.group(1) and 'grime: true' in line.group(1))
    _GRIME[label] = result
    return result


# ------------------------------------------------------------------------------------------------ glb reading and writing

_COMP = {5120: ('b', 1, 127.0), 5121: ('B', 1, 255.0), 5122: ('h', 2, 32767.0), 5123: ('H', 2, 65535.0), 5125: ('I', 4, None), 5126: ('f', 4, None)}
_NCOMP = {'SCALAR': 1, 'VEC2': 2, 'VEC3': 3, 'VEC4': 4, 'MAT4': 16}


def read_glb(data):
    if data[:4] != b'glTF':
        raise ValueError('not a binary glTF')
    version, length = struct.unpack_from('<II', data, 4)
    if version != 2:
        raise ValueError('glTF version %d' % version)
    off, gltf, binchunk = 12, None, b''
    while off < length:
        clen, ctype = struct.unpack_from('<II', data, off)
        chunk = data[off + 8: off + 8 + clen]
        if ctype == 0x4E4F534A:
            gltf = json.loads(chunk.decode('utf-8'))
        elif ctype == 0x004E4942:
            binchunk = chunk
        off += 8 + clen
    if gltf is None:
        raise ValueError('glb without JSON chunk')
    return gltf, binchunk


def read_accessor(gltf, binchunk, index):
    """Flat list of values and the component count."""
    a = gltf['accessors'][index]
    if 'sparse' in a:
        raise ValueError('sparse accessors are not supported (accessor %d)' % index)
    fmt, size, norm = _COMP[a['componentType']]
    n, count = _NCOMP[a['type']], a['count']
    if 'bufferView' not in a:
        return [0.0] * (count * n), n
    bv = gltf['bufferViews'][a['bufferView']]
    if bv.get('buffer', 0) != 0 or 'uri' in gltf['buffers'][bv.get('buffer', 0)]:
        raise ValueError('only the glb binary chunk is supported')
    off = bv.get('byteOffset', 0) + a.get('byteOffset', 0)
    stride = bv.get('byteStride') or size * n
    if stride == size * n:
        arr = array(fmt)
        arr.frombytes(binchunk[off: off + count * n * size])
        if sys.byteorder != 'little':
            arr.byteswap()
        values = arr.tolist()
    else:
        values = []
        for k in range(count):
            values.extend(struct.unpack_from('<%d%s' % (n, fmt), binchunk, off + k * stride))
    if a.get('normalized') and norm:
        values = [max(v / norm, -1.0) for v in values]
    return values, n


def write_glb(gltf, binchunk):
    js = json.dumps(gltf, separators=(',', ':'), sort_keys=True).encode('utf-8')
    js += b' ' * (-len(js) % 4)
    binchunk += b'\0' * (-len(binchunk) % 4)
    total = 12 + 8 + len(js) + 8 + len(binchunk)
    return (struct.pack('<4sII', b'glTF', 2, total) + struct.pack('<II', len(js), 0x4E4F534A) + js
            + struct.pack('<II', len(binchunk), 0x004E4942) + binchunk)


# 4 x 4 matrices as row-major nested lists acting on column vectors (p' = M p), in the reference's model space

def mat_identity():
    return [[1.0 if i == j else 0.0 for j in range(4)] for i in range(4)]


def mat_mul(a, b):
    return [[sum(a[i][k] * b[k][j] for k in range(4)) for j in range(4)] for i in range(4)]


def node_matrix(node):
    if 'matrix' in node:
        m = node['matrix']  # column-major
        return [[m[c * 4 + r] for c in range(4)] for r in range(4)]
    tx, ty, tz = node.get('translation', (0, 0, 0))
    qx, qy, qz, qw = node.get('rotation', (0, 0, 0, 1))
    sx, sy, sz = node.get('scale', (1, 1, 1))
    r = [[1 - 2 * (qy * qy + qz * qz), 2 * (qx * qy - qz * qw), 2 * (qx * qz + qy * qw)],
         [2 * (qx * qy + qz * qw), 1 - 2 * (qx * qx + qz * qz), 2 * (qy * qz - qx * qw)],
         [2 * (qx * qz - qy * qw), 2 * (qy * qz + qx * qw), 1 - 2 * (qx * qx + qy * qy)]]
    return [[r[0][0] * sx, r[0][1] * sy, r[0][2] * sz, tx], [r[1][0] * sx, r[1][1] * sy, r[1][2] * sz, ty],
            [r[2][0] * sx, r[2][1] * sy, r[2][2] * sz, tz], [0.0, 0.0, 0.0, 1.0]]


def mat3_det(m):
    return (m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]))


def normal_matrix(m):
    """Inverse transpose of the upper 3 x 3."""
    d = mat3_det(m)
    if abs(d) < 1e-20:
        raise ValueError('singular node transform')
    inv = [[(m[(j + 1) % 3][(i + 1) % 3] * m[(j + 2) % 3][(i + 2) % 3] - m[(j + 1) % 3][(i + 2) % 3] * m[(j + 2) % 3][(i + 1) % 3]) / d
            for j in range(3)] for i in range(3)]
    return [[inv[j][i] for j in range(3)] for i in range(3)]


def affine_inverse(m):
    inv3 = [[row[i] for row in normal_matrix(m)] for i in range(3)]
    t = [-sum(inv3[i][k] * m[k][3] for k in range(3)) for i in range(3)]
    return [inv3[0] + [t[0]], inv3[1] + [t[1]], inv3[2] + [t[2]], [0.0, 0.0, 0.0, 1.0]]


def to_unreal_location(p):
    """Reference model metres (x forward, y up, z right) to Unreal centimetres (X forward, Y right, Z up)."""
    return (p[0] * 100.0, p[2] * 100.0, p[1] * 100.0)


def to_unreal_scale(m):
    s = [math.sqrt(m[0][j] ** 2 + m[1][j] ** 2 + m[2][j] ** 2) for j in range(3)]
    return (s[0], s[2], s[1])


def to_unreal_quat(m):
    """Rotation part of a reference-space matrix as an Unreal quaternion (x, y, z, w): R' = S R S, S swapping y and z
    (the same change of basis as GLTFCore ConvertQuat)."""
    cols = []
    for j in range(3):
        v = [m[0][j], m[1][j], m[2][j]]
        n = math.sqrt(sum(x * x for x in v)) or 1.0
        cols.append([x / n for x in v])
    r = [[cols[j][i] for j in range(3)] for i in range(3)]
    s = [0, 2, 1]
    u = [[r[s[i]][s[j]] for j in range(3)] for i in range(3)]
    tr = u[0][0] + u[1][1] + u[2][2]
    if tr > 0:
        k = math.sqrt(tr + 1.0) * 2
        q = ((u[2][1] - u[1][2]) / k, (u[0][2] - u[2][0]) / k, (u[1][0] - u[0][1]) / k, 0.25 * k)
    elif u[0][0] > u[1][1] and u[0][0] > u[2][2]:
        k = math.sqrt(1.0 + u[0][0] - u[1][1] - u[2][2]) * 2
        q = (0.25 * k, (u[0][1] + u[1][0]) / k, (u[0][2] + u[2][0]) / k, (u[2][1] - u[1][2]) / k)
    elif u[1][1] > u[2][2]:
        k = math.sqrt(1.0 + u[1][1] - u[0][0] - u[2][2]) * 2
        q = ((u[0][1] + u[1][0]) / k, 0.25 * k, (u[1][2] + u[2][1]) / k, (u[0][2] - u[2][0]) / k)
    else:
        k = math.sqrt(1.0 + u[2][2] - u[0][0] - u[1][1]) * 2
        q = ((u[0][2] + u[2][0]) / k, (u[1][2] + u[2][1]) / k, 0.25 * k, (u[1][0] - u[0][1]) / k)
    return q


def unreal_transform(m):
    return dict(location=to_unreal_location([m[0][3], m[1][3], m[2][3]]), quat=to_unreal_quat(m), scale=to_unreal_scale(m))


# ------------------------------------------------------------------------------------------------ manifest

def load_manifest(path):
    """Models to import, from the exporter's manifest (ww2-reference-models-v1) or the bootstrap format."""
    with open(path, encoding='utf-8') as f:
        manifest = json.load(f)
    base = os.path.dirname(os.path.abspath(path))
    if manifest.get('format') == 'ww2-reference-models-v1':
        models = _reference_export_models(manifest, base)
    else:
        models = _bootstrap_models(manifest, base)
    keys = [m['key'] for m in models]
    dup = sorted(k for k in set(keys) if keys.count(k) > 1)
    if dup:
        raise ValueError('duplicate model keys in manifest: %r' % dup)
    manifest = dict(manifest)
    manifest['models'] = models
    return manifest


def _reference_export_models(manifest, base):
    out = []
    for a in manifest['assets']:
        cat = a['category']
        if cat == 'source':  # raw art sources (the Blender rig, the retopology), not drawn assets
            continue
        bits = a['id'].split('/')
        for f in a['files']:
            stem = os.path.basename(f['file'])[:-4]
            variants = []
            if cat in ('unit', 'unit-riders'):
                typ = a['type']
                if cat == 'unit-riders':
                    variants.append('riders')
            elif cat in ('plane', 'structure'):
                typ = bits[1] if len(bits) > 2 else stem  # planes/fighter/fighter-f0, planes/bomb, structures/airfield/...
            else:
                typ = '%s/%s' % (cat, a.get('kind') or stem)
            slot = re.search(r'-slot(\d+)$', stem)
            if slot:
                variants.append('slot' + slot.group(1))
            faction = a['faction'] if a.get('faction') is not None else -1
            look = f.get('look') or {}
            out.append(dict(key='|'.join([typ, str(faction)] + variants), type=typ, faction=int(faction), variant='|'.join(variants),
                            category=cat, asset_id=a['id'], file=os.path.join(base, f['file']), sha256=f.get('sha256'),
                            tint_base=(hex_linear(look['owner']) if 'owner' in look else (0.0, 0.0, 0.0),
                                       hex_linear(look['vehicle']) if 'vehicle' in look else (0.0, 0.0, 0.0)),
                            pivots=a.get('pivots', []), fx_tip=(a.get('extras') or {}).get('fxTip')))
    return out


def _bootstrap_models(manifest, base):
    models = manifest.get('models')
    if isinstance(models, dict):
        models = [dict(v, key=k) for k, v in sorted(models.items())]
    if not isinstance(models, list):
        raise ValueError('manifest without models')
    out = []
    for m in models:
        m = dict(m)
        if 'type' not in m or 'faction' not in m:
            bits = str(m.get('key', '')).split('|')
            if len(bits) < 2:
                raise ValueError('model without type/faction: %r' % m.get('key'))
            m.setdefault('type', bits[0])
            m.setdefault('faction', int(bits[1]))
        m['faction'] = int(m['faction'])
        f = m.get('file') or m.get('glb') or m.get('path')
        if not f:
            raise ValueError('model %s|%s has no file' % (m['type'], m['faction']))
        m['file'] = f if os.path.isabs(f) else os.path.join(base, f)
        m['variant'] = '|'.join(str(m[k]) for k in ('kit', 'pose', 'lod', 'variant') if m.get(k) not in (None, ''))
        m.setdefault('key', '|'.join([m['type'], str(m['faction'])] + ([m['variant']] if m['variant'] else [])))
        m.setdefault('look', 'vehicle')
        m.setdefault('category', 'unit')
        m['tint_base'] = ((0.0, 0.0, 0.0), (0.0, 0.0, 0.0))  # the bootstrap writes COLOR_0 with the tints taken out
        out.append(m)
    return out


# ------------------------------------------------------------------------------------------------ materials (no engine)

SPECS = {}  # slot -> spec, across every model of one run


def _bootstrap_ww2(name, model):
    """The bootstrap's tc:* names, as the exporter would describe the same reference material."""
    if name.startswith('tc:surface:'):
        bits = name.split(':')
        painted = len(bits) > 3 and bits[3] == 'painted'
        return dict(label="surfaces.js surface('%s'%s)" % (bits[2], ', true' if painted else ''), type='MeshLambertMaterial', vertexColors=painted,
                    shader=[{'patch': 'surfaces.js planar', 'surface': bits[2]}])
    paint = name[3:] if name.startswith('tc:') else LOOK_PAINT.get(model.get('look', 'vehicle'), 'vehicle')
    paint = 'soldier' if paint == 'infantry' else paint
    if paint not in PAINTS:
        raise ValueError('unknown paint %r' % name)
    p = PAINTS[paint]
    shader = [{'patch': 'model-textures.js modelMaterial', 'unsetMatId': p['mat']}]
    if p['atlas']:
        shader.append({'patch': 'infantry-material.js infantryMaterial'})
    return dict(label=p['label'], type='MeshStandardMaterial', color=[1, 1, 1], vertexColors=True, doubleSided=False, transparent=False,
                opacity=1, alphaTest=0, roughness=p['roughness'], metalness=p['metalness'], map=None, shader=shader)


def material_spec(gmat, model, reference=REFERENCE):
    """What a glTF material of the export stands for, as one of the generated material families."""
    name = (gmat or {}).get('name') or ''
    w = ((gmat or {}).get('extras') or {}).get('ww2')
    if w is None:
        w = _bootstrap_ww2(name, model)
    patches = {s.get('patch'): s for s in (w.get('shader') or [])}
    label = w.get('label')
    if 'surfaces.js planar' in patches:
        key = patches['surfaces.js planar']['surface']
        if key not in SURFACES:
            raise ValueError('unknown surface %r' % key)
        painted = bool(w.get('vertexColors'))
        spec = dict(family='surface', slot='Surface_%s%s' % (safe(key), '_Painted' if painted else ''), surface=key, painted=painted,
                    vertex_colors=painted, label=label, gaps=[])
        return _register(spec)
    typ = w.get('type') or 'MeshStandardMaterial'
    model_tex = 'model-textures.js modelMaterial' in patches
    structure = 'structures.js MODEL_MAT' in patches
    unlit, lambert = typ == 'MeshBasicMaterial', typ == 'MeshLambertMaterial'
    mp = w.get('map') or {}
    map_file = mp.get('file') if (mp and not structure) else None
    alpha = float(w.get('alphaTest') or 0)
    transparent = bool(w.get('transparent'))
    gaps = []
    if mp.get('procedural') and not structure:
        gaps.append('procedural map not drawn')
    if 'planes.js paintMaterial' in patches:
        gaps.append('planes.js paintMaterial: camouflage, panel and hinge lines are not drawn')
    if 'wheel-motion.js wheelMaterial' in patches:
        gaps.append('wheel-motion.js wheelMaterial: wheels and track links do not turn')
    if transparent:
        gaps.append('alpha blend (opacity %g) drawn as a dithered fade' % float(w.get('opacity', 1)))
    if unlit:
        gaps.append('MeshBasicMaterial (unlit) drawn as emissive')
    scalars = dict(DefaultMat=float(MATS.index(patches['model-textures.js modelMaterial']['unsetMatId'])) if model_tex else float(PLAIN),
                   BaseRoughness=1.0 if (lambert or unlit) else float(w.get('roughness', 1)),
                   BaseMetalness=0.0 if (lambert or unlit) else float(w.get('metalness', 0)),
                   BaseSpecular=0.0 if (lambert or unlit) else 0.5,
                   Grime=1.0 if model_tex and label_grime(label, reference) else 0.0,
                   ModelTextures=1.0 if model_tex else 0.0,
                   InfantryReady=1.0 if 'infantry-material.js infantryMaterial' in patches else 0.0,
                   Unlit=1.0 if unlit else 0.0, StructureDetail=1.0 if structure else 0.0,
                   MapOn=1.0 if map_file else 0.0, MapFlipY=1.0 if mp.get('flipY') else 0.0, AlphaTest=alpha,
                   # the exporter's ownerColor: the material's color is the owner's (unit-models.js cloth(f.color))
                   OwnerMaterial=1.0 if w.get('ownerColor') else 0.0)
    color = w.get('color') or [1, 1, 1]
    vectors = dict(MaterialColor=[float(c) for c in color], RimColor=list(RIM) if model_tex else [0.0, 0.0, 0.0])
    masked = alpha > 0 or transparent
    master = 'M_TCModel' + ('Faded' if masked else '') + ('TwoSided' if w.get('doubleSided') else '')
    spec = dict(family='model', master=master, scalars=scalars, vectors=vectors, map=map_file,
                opacity=float(w.get('opacity', 1)) if transparent else 1.0, vertex_colors=bool(w.get('vertexColors', True)),
                label=label, gaps=gaps)
    # a labelled material is one reference object (PAINT, BODY_MAT...); an unlabelled one is named by its whole
    # description, so every process names it alike
    spec['slot'] = 'Model_' + safe(label.split(' ', 1)[-1]) if label else 'Model_%s_%s' % (safe(typ), _spec_hash(spec))
    return _register(spec)


def _spec_hash(spec):
    body = {k: v for k, v in spec.items() if k != 'slot'}
    return hashlib.sha256(json.dumps(body, sort_keys=True, default=str).encode()).hexdigest()[:8]


def _register(spec):
    slot = spec['slot']
    if slot in SPECS and _spec_hash(SPECS[slot]) != _spec_hash(spec):
        raise ValueError('material %s is described two ways: %r and %r' % (slot, SPECS[slot], spec))
    SPECS.setdefault(slot, spec)
    return SPECS[slot]


# ------------------------------------------------------------------------------------------------ parts and normalization

ATTR_ALIASES = {
    'matId': ('_MATID', '_matId', 'matId'),
    'modelUV': ('_MODELUV', '_modelUV', 'modelUV'),
    'uv': ('_UV', '_uv'),
    'ownerTint': ('_OWNERTINT', '_ownerTint', 'ownerTint'),
    'vehicleTint': ('_VEHICLETINT', '_vehicleTint', 'vehicleTint'),
    'color': ('_COLOR',),
}


def _attr(prim, name):
    for alias in ATTR_ALIASES[name]:
        if alias in prim['attributes']:
            return prim['attributes'][alias]
    return None


def _parents(gltf):
    p = {}
    for i, n in enumerate(gltf.get('nodes', [])):
        for c in n.get('children', []):
            p[c] = i
    return p


def _roots(gltf):
    scenes = gltf.get('scenes') or [{'nodes': [i for i in range(len(gltf.get('nodes', []))) if i not in _parents(gltf)]}]
    return scenes[gltf.get('scene', 0)]['nodes']


def _world(gltf, node, parent):
    m = node_matrix(gltf['nodes'][node])
    while node in parent:
        node = parent[node]
        m = mat_mul(node_matrix(gltf['nodes'][node]), m)
    return m


def _hidden(gltf, node, parent):
    while True:
        if ((gltf['nodes'][node].get('extras') or {}).get('visible')) is False:
            return True
        if node not in parent:
            return False
        node = parent[node]


def _is_identity(node):
    m = node_matrix(node)
    return all(abs(m[i][j] - (1.0 if i == j else 0.0)) < 1e-9 for i in range(4) for j in range(4))


def find_parts(gltf, model):
    """[(part name, part node, bucket mesh nodes, parent part name)] in depth-first order.
    A group whose direct children are meshes at its own origin and visibility is one part (the reference bakes one mesh
    per material under each moving group); a mesh with a different place or visibility (a hidden far level of detail)
    is a part of its own. The bootstrap may name the parts instead."""
    nodes, parent = gltf.get('nodes', []), _parents(gltf)
    order = []

    def dfs(i):
        order.append(i)
        for c in nodes[i].get('children', []):
            dfs(c)
    for r in _roots(gltf):
        dfs(r)
    leaf = lambda i: 'mesh' in nodes[i] and not nodes[i].get('children')
    hid = lambda i: _hidden(gltf, i, parent)
    parts = []  # (node, buckets)
    if model.get('parts'):
        by_name = {}
        for i in order:
            by_name.setdefault(nodes[i].get('name', ''), i)
        named = {}
        for p in model['parts']:
            node = p.get('node', p['name'])
            if node not in by_name:
                raise ValueError('%s: part node %r not in glb' % (model['key'], node))
            named[by_name[node]] = p
        out = []
        for i in order:
            if i not in named:
                continue
            buckets = []

            def own(j):
                if j != i and j in named:
                    return
                if 'mesh' in nodes[j]:
                    buckets.append(j)
                for c in nodes[j].get('children', []):
                    own(c)
            own(i)
            out.append((named[i]['name'], i, buckets, named[i].get('parent')))
        return out
    covered = set()
    for g in order:
        if g in covered:
            continue  # a mesh its group already took (depth-first: the group comes first); not a second copy of it
        n = nodes[g]
        kids = [c for c in n.get('children', []) if leaf(c)]
        if 'mesh' in n:
            buckets = [g] + [c for c in kids if _is_identity(nodes[c]) and hid(c) == hid(g)]
            parts.append((g, buckets))
            covered.update(buckets)
        elif kids and all(_is_identity(nodes[c]) and hid(c) == hid(g) for c in kids):
            parts.append((g, kids))
            covered.update(kids)
    for i in order:
        if 'mesh' in nodes[i] and i not in covered:
            parts.append((i, [i]))
    rank = {i: k for k, i in enumerate(order)}
    parts.sort(key=lambda p: rank[p[0]])
    part_nodes = {p[0] for p in parts}
    names, seen, out = {}, {}, []
    for g, buckets in parts:
        name = nodes[g].get('name') or 'part%d' % g
        seen[name] = seen.get(name, 0) + 1
        names[g] = name if seen[name] == 1 else '%s_%d' % (name, seen[name] - 1)
    for g, buckets in parts:
        anc, i = None, g
        while i in parent:
            i = parent[i]
            if i in part_nodes:
                anc = i
                break
        out.append((names[g], g, buckets, names[anc] if anc is not None else None))
    if not out:
        raise ValueError('%s: no geometry in glb' % model['key'])
    return out


def _sockets(gltf, parts, parent):
    """Named empty nodes as sockets: on their nearest part, or (when they hang beside the parts, like a gun's muzzle
    point under its traversing group) on every part under the same parent node."""
    nodes = gltf['nodes']
    part_node = {node: name for name, node, _, _ in parts}
    out = {name: [] for name, _, _, _ in parts}
    for e, n in enumerate(nodes):
        if 'mesh' in n or n.get('children') or not n.get('name'):
            continue
        sname = n['name'][len('socket_'):] if n['name'].lower().startswith('socket_') else n['name']
        holders, i = [], e
        while i in parent:
            i = parent[i]
            if i in part_node:
                holders = [i]
                break
        if not holders and e in parent:
            group = parent[e]

            def under(j):
                while j in parent:
                    j = parent[j]
                    if j == group:
                        return True
                return False
            holders = [p for p in part_node if under(p) or p == group]
        we = _world(gltf, e, parent)
        for h in holders:
            rel = mat_mul(affine_inverse(_world(gltf, h, parent)), we)
            out[part_node[h]].append(dict(name=sname, **unreal_transform(rel)))
    return out


def normalize_model(model, out_dir, cache=True, reference=REFERENCE):
    """Split one exported glb into one normalized glb per part (layout in the module docstring), cached in out_dir by
    the source's and this tool's sha256. Returns the parts with rest transforms, sockets, slots and statistics."""
    os.makedirs(out_dir, exist_ok=True)
    meta_path = os.path.join(out_dir, 'normalized.json')
    with open(model['file'], 'rb') as f:
        raw = f.read()
    src_sha, tool_sha = hashlib.sha256(raw).hexdigest(), sha256_file(os.path.abspath(__file__))
    if cache and os.path.exists(meta_path):
        meta = json.load(open(meta_path, encoding='utf-8'))
        if meta.get('source_sha256') == src_sha and meta.get('tool_sha256') == tool_sha and all(
                os.path.exists(p['glb']) for p in meta['parts'] if 'glb' in p):
            for slot, spec in meta.get('specs', {}).items():
                SPECS.setdefault(slot, spec)
            return meta
    gltf, binchunk = read_glb(raw)
    nodes, parent = gltf.get('nodes', []), _parents(gltf)
    parts = find_parts(gltf, model)
    sockets = _sockets(gltf, parts, parent)
    owner_base, vehicle_base = model.get('tint_base', ((0.0, 0.0, 0.0), (0.0, 0.0, 0.0)))
    result = dict(key=model['key'], source=model['file'], source_sha256=src_sha, source_bytes=len(raw), tool_sha256=tool_sha,
                  parts=[], specs={}, warnings=[])
    if any(p.get('targets') for m in gltf.get('meshes', []) for p in m['primitives']):
        result['warnings'].append('morph targets (postures, gait, the fallen build) are not imported: static meshes hold the rest pose')
    by_name = {name: node for name, node, _, _ in parts}
    for name, pnode, buckets, pparent in parts:
        world = _world(gltf, pnode, parent)
        entry = dict(name=name, node=nodes[pnode].get('name', ''), parent=pparent, hidden=_hidden(gltf, pnode, parent),
                     rest=unreal_transform(world), sockets=sockets.get(name, []))
        if pparent is not None:
            rel = mat_mul(affine_inverse(_world(gltf, by_name[pparent], parent)), world)
            entry['socket_on_parent'] = dict(name=name, **unreal_transform(rel))
        inv = affine_inverse(world)
        prims = []
        for b in buckets:
            m = mat_mul(inv, _world(gltf, b, parent))
            for prim in gltf['meshes'][nodes[b]['mesh']]['primitives']:
                if prim.get('mode', 4) != 4:
                    raise ValueError('%s: primitive mode %s (triangles only)' % (model['key'], prim.get('mode')))
                gmat = gltf['materials'][prim['material']] if 'material' in prim else None
                prims.append((m, prim, material_spec(gmat, model, reference)))
        slots, stats, bake_vehicle = _gather(gltf, binchunk, prims, owner_base, vehicle_base, model['key'] + '/' + name)
        for spec in {p[2]['slot']: p[2] for p in prims}.values():
            result['specs'][spec['slot']] = spec
        body = build_part_glb(slots)
        content = hashlib.sha256(body).hexdigest()
        asset = 'SM_%s_%s' % (safe(model['key']), safe(name))
        path = os.path.join(out_dir, asset + '.glb')
        with open(path, 'wb') as f:
            f.write(body)
        opacity = min([SPECS[s]['opacity'] for s in slots if SPECS[s].get('family') == 'model'] or [1.0])
        stats['degenerate'] = _degenerate(slots)
        entry.update(asset=asset, glb=path, glb_sha256=content, slots=sorted(slots), stats=stats, bounds=_bounds(slots),
                     vehicle_tint='baked into COLOR_0 (not linear per channel)' if bake_vehicle else 'UV2.y', opacity=opacity)
        result['parts'].append(entry)
    write_json(meta_path, result)
    return result


def _gather(gltf, binchunk, prims, owner_base, vehicle_base, where):
    """Read and convert every primitive of one part, grouped by material slot."""
    raw = []
    bake_vehicle = False
    for m, prim, spec in prims:
        a = prim['attributes']
        pos, _ = read_accessor(gltf, binchunk, a['POSITION'])
        count = len(pos) // 3
        if 'NORMAL' not in a:
            raise ValueError('%s: primitive without normals' % where)
        nor, _ = read_accessor(gltf, binchunk, a['NORMAL'])
        col_i = _attr(prim, 'color') if _attr(prim, 'color') is not None else a.get('COLOR_0')
        if col_i is not None and spec.get('vertex_colors', True):
            col, cn = read_accessor(gltf, binchunk, col_i)
        else:
            col, cn = [1.0] * (count * 3), 3

        def get(name, default, comps):
            i = _attr(prim, name)
            return read_accessor(gltf, binchunk, i) if i is not None else ([default] * (count * comps), comps)
        matid, _ = get('matId', float(UNSET), 1)
        owner, on = get('ownerTint', 0.0, 1)
        vehicle, vn = get('vehicleTint', 0.0, 1)
        plain_gltf = not any(k.startswith('_') for k in a)
        if _attr(prim, 'modelUV') is not None:
            muv, mn = read_accessor(gltf, binchunk, _attr(prim, 'modelUV'))
        elif _attr(prim, 'uv') is not None:
            uv, _ = read_accessor(gltf, binchunk, _attr(prim, 'uv'))
            muv, mn = [x for k in range(count) for x in (uv[k * 2], uv[k * 2 + 1], 0.0)], 3
        elif plain_gltf and 'TEXCOORD_0' in a:
            uv, _ = read_accessor(gltf, binchunk, a['TEXCOORD_0'])
            muv, mn = [x for k in range(count) for x in (uv[k * 2], uv[k * 2 + 1], 0.0)], 3
        else:
            muv, mn = [0.0] * (count * 3), 3
        idx = read_accessor(gltf, binchunk, prim['indices'])[0] if 'indices' in prim else list(range(count))
        for k in range(count):
            o = owner[k * on: k * on + on]
            if max(o) - min(o) > 1e-3:
                raise ValueError('%s: an owner tint that is not grey (the material takes one owner weight per vertex)' % where)
            v = vehicle[k * vn: k * vn + vn]
            if max(v) - min(v) > 1e-3:
                bake_vehicle = True
        raw.append((m, spec, count, pos, nor, col, cn, matid, owner, on, vehicle, vn, muv, mn, idx))
    slots = {}
    stats = dict(vertices=0, triangles=0, owner_masked=0, vehicle_masked=0, atlas=0, clamped=0, scaled=0, flipped_nodes=0, layers={},
                 corner_layers={}, corner_owner=0, corner_atlas=0)
    for m, spec, count, pos, nor, col, cn, matid, owner, on, vehicle, vn, muv, mn, idx in raw:
        nm = normal_matrix(m)
        flip = mat3_det(m) < 0
        stats['flipped_nodes'] += 1 if flip else 0
        s = slots.setdefault(spec['slot'], dict(pos=[], nor=[], col=[], uv0=[], uv1=[], uv2=[], uv3=[], idx=[]))
        base = len(s['pos']) // 3
        for k in range(count):
            x, y, z = pos[k * 3: k * 3 + 3]
            s['pos'] += [m[0][0] * x + m[0][1] * y + m[0][2] * z + m[0][3], m[1][0] * x + m[1][1] * y + m[1][2] * z + m[1][3], m[2][0] * x + m[2][1] * y + m[2][2] * z + m[2][3]]
            a, b, c = nor[k * 3: k * 3 + 3]
            na = [nm[0][0] * a + nm[0][1] * b + nm[0][2] * c, nm[1][0] * a + nm[1][1] * b + nm[1][2] * c, nm[2][0] * a + nm[2][1] * b + nm[2][2] * c]
            ln = math.sqrt(na[0] * na[0] + na[1] * na[1] + na[2] * na[2]) or 1.0
            s['nor'] += [na[0] / ln, na[1] / ln, na[2] / ln]
            o = sum(owner[k * on: k * on + on]) / on
            vv = vehicle[k * vn: k * vn + vn]
            v = 0.0 if bake_vehicle else sum(vv) / vn
            rgb = []
            for ch in range(3):
                c0 = col[k * cn + ch] - owner_base[ch] * o
                if not bake_vehicle:
                    c0 -= vehicle_base[ch] * v
                if c0 < -1e-4:
                    stats['clamped'] += 1
                rgb.append(max(0.0, c0))
            scale = max(rgb)
            if scale > 1.0:
                stats['scaled'] += 1
                rgb = [c / scale for c in rgb]
            else:
                scale = 1.0
            s['col'] += rgb + [1.0]
            stats['owner_masked'] += 1 if o > 1e-4 else 0
            stats['vehicle_masked'] += 1 if v > 1e-4 else 0
            w0 = muv[k * mn + 2] if mn > 2 else 0.0
            stats['atlas'] += 1 if w0 > 0.5 else 0
            s['uv0'] += [muv[k * mn], muv[k * mn + 1]]
            s['uv1'] += [matid[k], w0]
            s['uv2'] += [o, v]
            s['uv3'] += [scale, 0.0]
            layer = math.floor(matid[k] + 0.25)
            lname = 'plain' if layer == PLAIN else 'unset' if layer == UNSET else MATS[layer] if 0 <= layer < len(MATS) else 'invalid'
            stats['layers'][lname] = stats['layers'].get(lname, 0) + 1
        for t in range(0, len(idx) - 2, 3):
            a, b, c = int(idx[t]), int(idx[t + 1]), int(idx[t + 2])
            s['idx'] += [base + a, base + c, base + b] if flip else [base + a, base + b, base + c]
        for k in idx:  # per triangle corner, as Unreal's mesh description stores them (--verify compares these)
            k = int(k)
            lay = str(int(math.floor(matid[k] + 0.25)))
            stats['corner_layers'][lay] = stats['corner_layers'].get(lay, 0) + 1
            stats['corner_owner'] += 1 if sum(owner[k * on: k * on + on]) / on > 1e-3 else 0
            stats['corner_atlas'] += 1 if (muv[k * mn + 2] if mn > 2 else 0.0) > 0.5 else 0
        stats['vertices'] += count
        stats['triangles'] += len(idx) // 3
    if stats['layers'].get('invalid'):
        raise ValueError('%s: matId outside the MATS table' % where)
    return slots, stats, bake_vehicle


def _bounds(slots):
    """Bounds of the vertices triangles use (Unreal drops the others), in Unreal centimetres."""
    lo, hi = [1e30] * 3, [-1e30] * 3
    for s in slots.values():
        pos = s['pos']
        for k in set(s['idx']):
            for i in range(3):
                v = pos[k * 3 + i]
                lo[i] = v if v < lo[i] else lo[i]
                hi[i] = v if v > hi[i] else hi[i]
    if lo[0] > hi[0]:
        return None
    return dict(min=to_unreal_location(lo), max=to_unreal_location(hi))


def _degenerate(slots):
    """Triangles with two corners at one position: Unreal's static mesh build drops them (they cover no pixels)."""
    n = 0
    for s in slots.values():
        pos, idx = s['pos'], s['idx']
        for t in range(0, len(idx), 3):
            a, b, c = (pos[idx[t + k] * 3: idx[t + k] * 3 + 3] for k in range(3))
            if a == b or b == c or a == c:
                n += 1
    return n


def build_part_glb(slots):
    """One mesh, one primitive per material slot (named after the slot), float attributes, 32 bit indices. Node and
    mesh carry fixed names, so identical geometry gives identical bytes (the importer names the asset itself)."""
    gltf = dict(asset=dict(version='2.0', generator='import-models.py'), scene=0, scenes=[dict(nodes=[0])],
                nodes=[dict(name='part', mesh=0)], meshes=[dict(name='part', primitives=[])], materials=[],
                accessors=[], bufferViews=[], buffers=[])
    blob = bytearray()

    def add(values, fmt, comp, typ, target=None, minmax=False):
        arr = array(fmt, values)
        if sys.byteorder != 'little':
            arr.byteswap()
        data = arr.tobytes()
        blob.extend(b'\0' * (-len(blob) % 4))
        view = dict(buffer=0, byteOffset=len(blob), byteLength=len(data))
        if target:
            view['target'] = target
        blob.extend(data)
        gltf['bufferViews'].append(view)
        acc = dict(bufferView=len(gltf['bufferViews']) - 1, componentType=comp, count=len(values) // _NCOMP[typ], type=typ)
        if minmax:
            n = _NCOMP[typ]
            acc['min'] = [min(values[i::n]) for i in range(n)]
            acc['max'] = [max(values[i::n]) for i in range(n)]
        gltf['accessors'].append(acc)
        return len(gltf['accessors']) - 1

    for slot in sorted(slots):
        s = slots[slot]
        if not s['idx']:
            continue
        gltf['materials'].append(dict(name=slot))
        attrs = dict(POSITION=add(s['pos'], 'f', 5126, 'VEC3', 34962, True), NORMAL=add(s['nor'], 'f', 5126, 'VEC3', 34962),
                     COLOR_0=add(s['col'], 'f', 5126, 'VEC4', 34962), TEXCOORD_0=add(s['uv0'], 'f', 5126, 'VEC2', 34962),
                     TEXCOORD_1=add(s['uv1'], 'f', 5126, 'VEC2', 34962), TEXCOORD_2=add(s['uv2'], 'f', 5126, 'VEC2', 34962),
                     TEXCOORD_3=add(s['uv3'], 'f', 5126, 'VEC2', 34962))
        gltf['meshes'][0]['primitives'].append(dict(attributes=attrs, indices=add(s['idx'], 'I', 5125, 'SCALAR', 34963), material=len(gltf['materials']) - 1))
    gltf['buffers'].append(dict(byteLength=len(blob) + (-len(blob) % 4)))
    return write_glb(gltf, bytes(blob))


def _normalize_job(args):
    model, out_dir, reference = args
    try:
        return normalize_model(model, out_dir, True, reference), None
    except Exception as e:  # reported by the caller with the model key
        return None, '%s: %s' % (model['key'], e)


def normalize_all(models, work, reference=REFERENCE, jobs=1):
    jobs_list = [(m, os.path.join(work, safe(m['key'])), reference) for m in models]
    if jobs > 1:
        import multiprocessing
        with multiprocessing.Pool(jobs) as pool:
            results = pool.map(_normalize_job, jobs_list, chunksize=1)
    else:
        results = [_normalize_job(j) for j in jobs_list]
    errors = [e for _, e in results if e]
    if errors:
        raise RuntimeError('normalization failed:\n  ' + '\n  '.join(errors))
    out = [r for r, _ in results]
    for r in out:
        for slot, spec in r['specs'].items():
            _register(json.loads(json.dumps(spec)))  # the same material from two processes must agree
    # identical geometry (a destroyer's four gun mounts, a squad's men of one kit, or across factions) imports once
    first = {}
    for r in out:
        for p in r['parts']:
            if p['glb_sha256'] in first:
                p['same_as'] = first[p['glb_sha256']]
            else:
                first[p['glb_sha256']] = dict(key=r['key'], asset=p['asset'])
    return out


# ------------------------------------------------------------------------------------------------ textures (no engine)

def layer_file(name, reference=REFERENCE):
    f = LAYERS[name].get('file', name + '.jpg')
    return os.path.normpath(os.path.join(reference, 'client', 'textures', 'models', f))


def layer_means(reference=REFERENCE):
    """Linear mean color of each layer drawn at SIZE x SIZE, as client/model-textures.js pack() computes it.
    Needs ImageMagick ('magick'); without it every layer reads the reference's own fallback (a flat mid grey)."""
    tool = shutil.which('magick')
    means, how = [], {}
    for name in MATS:
        path = layer_file(name, reference)
        if tool and os.path.exists(path):
            out = subprocess.run([tool, path, '-resize', '%dx%d!' % (SIZE, SIZE), '-depth', '8', '-colorspace', 'RGB',
                                  '-format', '%[fx:mean.r] %[fx:mean.g] %[fx:mean.b]', 'info:'], capture_output=True, text=True, check=True).stdout
            means.append(tuple(float(x) for x in out.split()))
            how[name] = 'magick'
        else:
            g = srgb_to_linear(128 / 255.0)
            means.append((g, g, g))
            how[name] = 'fallback-grey'
    version = subprocess.run([tool, '-version'], capture_output=True, text=True).stdout.split('\n')[0] if tool else None
    return means, how, version


def brick_png():
    """client/surfaces.js brickCanvas, ported: the same seeded draws. The browser's canvas antialiasing and gradient
    rasterization are approximated (fractional rectangle edges by coverage, gradients unpremultiplied), so the
    texture matches the reference's pattern and colors, not its every pixel."""
    N = 512
    bw, bh = N / 10.0, N / 32.0
    seed = [7]

    def r():
        seed[0] = seed[0] * 16807 % 2147483647
        return seed[0] / 2147483647.0
    img = [[0xb3, 0xa7, 0x93] * N for _ in range(N)]

    def hsl(h, s, l):
        h, s, l = (h % 360) / 360.0, s / 100.0, l / 100.0
        q = l * (1 + s) if l < 0.5 else l + s - l * s
        p = 2 * l - q

        def ch(t):
            t %= 1.0
            return p + (q - p) * 6 * t if t < 1 / 6 else q if t < 0.5 else p + (q - p) * (2 / 3 - t) * 6 if t < 2 / 3 else p
        return [int(round(ch(h + 1 / 3) * 255)), int(round(ch(h) * 255)), int(round(ch(h - 1 / 3) * 255))]

    def fill(x0, y0, w, h, rgb):
        x1, y1 = x0 + w, y0 + h
        for py in range(max(0, int(math.floor(y0))), min(N, int(math.ceil(y1)))):
            cy = min(py + 1, y1) - max(py, y0)
            row = img[py]
            for px in range(max(0, int(math.floor(x0))), min(N, int(math.ceil(x1)))):
                a = cy * (min(px + 1, x1) - max(px, x0))
                if a <= 0:
                    continue
                for c in range(3):
                    row[px * 3 + c] = int(round(row[px * 3 + c] * (1 - a) + rgb[c] * a))
    for row in range(32):
        for i in range(-1, 10):
            v = r()
            burnt = r() < 0.08
            s = 20 if burnt else 40 + r() * 14
            l = 30 + r() * 5 if burnt else 35 + v * 12
            fill((i + (0.5 if row & 1 else 0)) * bw + 1, row * bh + 1, bw - 2, bh - 2, hsl(11 + v * 10, s, l))
    for _ in range(36):
        x, y, rad, dark = r() * N, r() * N, 40 + r() * 90, r() < 0.6
        stop = (40, 30, 26, 0.16) if dark else (225, 205, 185, 0.12)
        for ox in (-N, 0, N):
            for oy in (-N, 0, N):
                cx, cy = x + ox, y + oy
                for py in range(max(0, int(cy - rad)), min(N, int(math.ceil(cy + rad)))):
                    rowp = img[py]
                    for px in range(max(0, int(cx - rad)), min(N, int(math.ceil(cx + rad)))):
                        t = math.hypot(px + 0.5 - cx, py + 0.5 - cy) / rad
                        if t >= 1:
                            continue
                        a = stop[3] * (1 - t)
                        for c in range(3):
                            col = stop[c] * (1 - t)  # unpremultiplied toward rgba(0,0,0,0)
                            rowp[px * 3 + c] = int(round(rowp[px * 3 + c] * (1 - a) + col * a))
    for py in range(N):
        rowp = img[py]
        for px in range(N):
            n = (r() - 0.5) * 18
            for c in range(3):
                vv = rowp[px * 3 + c] + n
                rowp[px * 3 + c] = 0 if vv < 0 else 255 if vv > 255 else int(round(vv))
    raw = b''.join(b'\0' + bytes(row) for row in img)

    def chunk(t, d):
        return struct.pack('>I', len(d)) + t + d + struct.pack('>I', zlib.crc32(t + d) & 0xffffffff)
    return b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', N, N, 8, 2, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(raw, 9)) + chunk(b'IEND', b'')


# ------------------------------------------------------------------------------------------------ material code

# client/infantry-material.js. Shared with refresh-normal-atlas.py for existing master/crowd/track assets.
INFANTRY_NORMAL_HLSL = '''if (atlas && InfantryDetail > 0.5) {
	// Swapping Three Y/Z reflects the coordinate system. With UE screen derivatives, use ddy here.
	float2 st0 = ddx(AtlasUV), st1 = ddy(AtlasUV);
	float3 q1perp = cross(-q1, nrm), q0perp = cross(nrm, q0);
	float3 t = q1perp * st0.x + q0perp * st1.x;
	float3 b = q1perp * st0.y + q0perp * st1.y;
	float tscale = rsqrt(max(max(dot(t, t), dot(b, b)), 1e-12));
	float3 mapN = InfantryNormal.Sample(InfantryNormalSampler, AtlasUV).rgb * 2.0 - 1.0;
	mapN.xy *= 0.65;
	nrm = normalize(t * tscale * mapN.x + b * tscale * mapN.y + nrm * mapN.z);
}'''


def model_hlsl(means):
    """The Custom node of the M_TCModel masters: client/model-textures.js (MODEL_TEX, MODEL_SURFACE, MODEL_GRIME, the
    rim), client/infantry-material.js (the atlas and its normal map), client/models/blender-baked.js (owner and vehicle
    tint), client/structures.js MODEL_MAT (the detail texture by surface id), a plain map for foliage, and unlit, in the
    reference's own model space (three: x forward, y up, z right; metres)."""
    f = lambda v: '%.9g' % v
    layer = ', '.join('float4(%s, %s, %s, %s)' % (f(LAYERS[n]['texels'] / float(SIZE)), f(LAYERS[n]['strength']), f(LAYERS[n]['hue']), f(LAYERS[n]['fade'])) for n in MATS)
    surface = ', '.join('float3(%s, %s, %s)' % (f(LAYERS[n]['roughness']), f(LAYERS[n]['metalness']), f(LAYERS[n]['relief'])) for n in MATS)
    film = ', '.join(f(LAYERS[n].get('film', 1)) for n in MATS)
    mean = ', '.join('float3(%s, %s, %s)' % tuple(f(c) for c in m) for m in means)
    return '''// Generated by unreal/Tools/models/import-models.py from the pinned reference. Edit the generator, not this.
static const float4 LAYER[%(n)d] = { %(layer)s };
static const float3 SURFACE[%(n)d] = { %(surface)s };
static const float FILM[%(n)d] = { %(film)s };
static const float3 MEAN[%(n)d] = { %(mean)s };
static const float3 LUMA = float3(0.2126, 0.7152, 0.0722);
static const float MUD_LAYER = %(mud)d.0;
static const float MUD_SCALE = %(mudscale)s;
struct TCTriplanar {
	float3 Sample(Texture2DArray T, SamplerState S, float layer, float3 p, float3 dx, float3 dy, float3 w) {
		float3 c = 0;
		if (w.x > 0.0) c += w.x * T.SampleGrad(S, float3(p.zy, layer), dx.zy, dy.zy).rgb;
		if (w.y > 0.0) c += w.y * T.SampleGrad(S, float3(p.xz, layer), dx.xz, dy.xz).rgb;
		if (w.z > 0.0) c += w.z * T.SampleGrad(S, float3(p.xy, layer), dx.xy, dy.xy).rgb;
		return c;
	}
};
TCTriplanar tri;
// vertex color: linear in the reference and as VertexColor reads it (Interchange's glTF factory undoes the sRGB
// encoding the static mesh builder applies: GLTFMeshFactory.cpp SRGBToLinear), divided by the scale in UV3.x
float3 s = saturate(VertexColor.rgb);
float3 color = s * Scale.x;
color += Owner * Tint.x + Vehicle * Tint.y;
color *= OwnerMaterial > 0.5 ? Owner : MaterialColor; // a material colored by its owner takes the instance's color
float vmat = floor(Mat.x + 0.25);
float grime = (Mat.x - vmat) * 2.0;
float id = vmat < -1.5 ? DefaultMat : vmat;
int layer = (int)(max(id, 0.0) + 0.5);
float3 surf = id > -0.5 ? SURFACE[layer] : float3(BaseRoughness, BaseMetalness, 0.0);
float detail = 0.0, height = 0.0;
float3 P = float3(LocalPosition.x, LocalPosition.z, LocalPosition.y) * 0.01;
float3 MN = normalize(float3(LocalNormal.x, LocalNormal.z, LocalNormal.y));
float3 N = normalize(VertexNormal);
float3 dx = ddx(P), dy = ddy(P);
if (MapOn > 0.5) {
	// three's map (foliage bark and leaves): uv as built, flipped like three's TextureLoader
	color *= Map.Sample(MapSampler, float2(AtlasUV.x, MapFlipY > 0.5 ? 1.0 - AtlasUV.y : AtlasUV.y)).rgb;
}
if (StructureDetail > 0.5) {
	// client/structures.js MODEL_MAT: uv.x is a surface id; the detail texture is mapped from model-space axes
	float sid = floor(AtlasUV.x + 0.5);
	if (sid > 0.5) {
		float3 an = abs(MN);
		float2 duv = an.y > 0.6 ? P.xz : an.x > an.z ? P.zy : P.xy;
		float perM = sid < 1.5 ? %(pm0)s : sid < 2.5 ? %(pm1)s : sid < 3.5 ? %(pm2)s : sid < 4.5 ? %(pm3)s : sid < 5.5 ? %(pm4)s : %(pm5)s;
		duv *= perM;
		float3 d = Detail.Sample(DetailSampler, float2(duv.x, 1.0 - duv.y)).rgb;
		float k = sid < 1.5 || (sid > 4.5 && sid < 5.5) ? d.r : sid < 2.5 ? d.g : d.b;
		if ((sid > 3.5 && sid < 4.5) || sid > 5.5) {
			float ph = (sid > 5.5 ? P.z : P.x) * 41.9;
			k *= 1.0 + 0.3 * sin(ph) * clamp(1.0 - fwidth(ph) / 2.5, 0.0, 1.0);
		}
		color *= 2.0 * k;
	}
}
float lv = dot(color, LUMA);
if (ModelTextures > 0.5) {
	float4 look = LAYER[layer];
	float strength = id > -0.5 ? look.y : 0.0;
	float3 w = pow(abs(MN), 4.0);
	w /= max(w.x + w.y + w.z, 1e-6);
	w = max(w - 0.04, 0.0);
	w /= max(w.x + w.y + w.z, 1e-6);
	float px = min(length(dx), length(dy)), lod = log2(max(px * look.x * %(size)d.0, 1e-6));
	float3 mean = MEAN[layer];
	float3 tex = mean;
	if (lod < 7.0) tex = lerp(tri.Sample(Layers, LayersSampler, (float)layer, P * look.x, dx * look.x, dy * look.x, w), mean, smoothstep(6.0, 7.0, lod));
	float lt = dot(tex, LUMA), lm = dot(mean, LUMA);
	float3 paint = lerp(color, lv.xxx, look.w);
	float3 detailed = lerp(paint * clamp(lt / max(lm, 1e-4), 0.55, 1.65), tex * (lv / max(lm, 1e-4)), look.z);
	color = lerp(color, detailed, strength);
	detail = clamp(lt / max(lm, 1e-4) - 1.0, -1.0, 1.0) * strength;
	height = detail * surf.z;
	if (Grime > 0.5) {
		float g = grime, film = FILM[layer];
		float3 mm = MEAN[%(mud)d];
		float lmm = dot(mm, LUMA);
		float3 dust = lerp(mm, lmm.xxx, 0.55) * 1.5;
		dust *= min(1.0, (lv * 2.0 + 0.015) / dot(dust, LUMA));
		color = lerp(color, dust, clamp(g * (0.25 + 0.2 * lt / lm), 0.0, 0.6) * film);
		if (g > 0.3 && log2(max(px * MUD_SCALE * %(size)d.0, 1e-6)) < 7.0) {
			float3 mud = tri.Sample(Layers, LayersSampler, MUD_LAYER, P * MUD_SCALE, dx * MUD_SCALE, dy * MUD_SCALE, w);
			float h = dot(mud, LUMA) / lmm, edge = lerp(2.3, 0.9, g);
			float clump = smoothstep(edge - 0.2, edge + 0.2, h) * smoothstep(0.3, 0.55, g) * (0.5 + 0.5 * film);
			float3 caked = lerp(mm * 1.1, mud, 0.6);
			caked = lerp(caked, dot(caked, LUMA).xxx, 0.55) * 0.8;
			caked *= min(1.0, (lv * 2.5 + 0.02) / max(dot(caked, LUMA), 1e-4));
			color = lerp(color, caked, clump);
		}
	}
}
// client/infantry-material.js: the authored atlas on vertices with modelUV.z > 0.5 (z - 1: how much is cloth)
bool atlas = Mat.y > 0.5 && InfantryReady > 0.5;
if (atlas) {
	float3 albedo = InfantryColor.Sample(InfantryColorSampler, AtlasUV).rgb;
	float cloth = clamp(Mat.y - 1.0, 0.0, 1.0);
	float peak = max(albedo.r, max(albedo.g, albedo.b));
	albedo /= 1.0 + cloth * max(0.0, peak - 0.20) * 3.0;
	color *= albedo;
}
float rough = ModelTextures > 0.5 ? clamp(surf.x - detail * 0.055 + max(grime, 0.0) * 0.12, 0.32, 1.0) : BaseRoughness;
float metal = ModelTextures > 0.5 ? surf.y * (1.0 - clamp(grime, 0.0, 0.85)) : BaseMetalness;
float3 nrm = N;
float3 q0 = ddx(WorldPos), q1 = -ddy(WorldPos); // the reference's derivatives run up the screen (GL); Unreal's run down
%(atlas_normal)s
if (ModelTextures > 0.5) {
	// surface-gradient relief from the detail sample (invariant to the screen's y direction)
	float2 hg = float2(ddx(height), ddy(height));
	float3 sx = normalize(q0), sy = normalize(-q1);
	float3 rx = cross(sy, nrm), ry = cross(nrm, sx);
	float det = dot(sx, rx);
	float3 gradient = sign(det) * (hg.x * rx + hg.y * ry);
	nrm = normalize(abs(det) * nrm - gradient);
}
float rim = 1.0 - saturate(dot(nrm, normalize(CameraVector)));
rim *= rim;
float3 emissive = RimColor * rim * rim * (0.2 + 0.8 * color);
float spec = BaseSpecular;
if (Unlit > 0.5) {
	// MeshBasicMaterial: its color, unlit
	emissive = color;
	color = 0;
	spec = 0.0;
	rough = 1.0;
	metal = 0.0;
}
// damage: 1 is the reference's wreck (a flat Lambert 0x1d1b18), in between a straight blend
color = lerp(color, WreckColor, Damage);
Roughness = lerp(rough, 1.0, Damage);
Metallic = metal * (1.0 - Damage);
Specular = spec * (1.0 - Damage);
NormalWS = normalize(lerp(nrm, N, Damage));
Emissive = emissive * (1.0 - Damage);
return color;
''' % dict(atlas_normal=INFANTRY_NORMAL_HLSL, n=len(MATS), layer=layer, surface=surface, film=film, mean=mean, mud=MATS.index('mud'), mudscale=f(MUD_TEXELS / float(SIZE)), size=SIZE,
           pm0=STRUCTURE_PER_METRE[0], pm1=STRUCTURE_PER_METRE[1], pm2=STRUCTURE_PER_METRE[2], pm3=STRUCTURE_PER_METRE[3], pm4=STRUCTURE_PER_METRE[4], pm5=STRUCTURE_PER_METRE[5])


OPACITY_HLSL = '''// Dithered fade (camouflage 0.45, ghosts 0.35, corpses, the propeller disc's 0.08): opaque pixels in proportion to
// Fade, so faded units stay in the opaque passes. The reference alpha-blends them; the dither is the deliberate
// difference. A map with an alpha test (foliage leaves) cuts first, as three's alphaTest does.
if (MapOn > 0.5 && AlphaTest > 0.0) {
	float a = Map.Sample(MapSampler, float2(AtlasUV.x, MapFlipY > 0.5 ? 1.0 - AtlasUV.y : AtlasUV.y)).a;
	if (a < AlphaTest) return -1.0;
}
float2 p = Parameters.SvPosition.xy + 5.588238 * (float)View.StateFrameIndexMod8;
float n = frac(52.9829189 * frac(dot(p, float2(0.06711056, 0.00583715))));
return Fade - n + 0.5;
'''

SURFACE_HLSL = '''// client/surfaces.js planar(): world-planar coordinates in metres (three axes), the texture times tint, top faces tinted.
float3 gw = float3(WorldPos.x, WorldPos.z, WorldPos.y) * 0.01;
float3 gn = normalize(float3(WorldNormal.x, WorldNormal.z, WorldNormal.y));
float3 an = abs(gn);
float2 uv = (an.y > 0.5 ? gw.xz : an.x > an.z ? gw.zy : gw.xy) * UVScale;
float top = smoothstep(0.5, 0.9, gn.y);
// three loads these with flipY: v runs up the image
float3 c = Tex.Sample(TexSampler, float2(uv.x, -uv.y)).rgb * Tint;
if (Painted > 0.5) {
	float3 s = saturate(VertexColor.rgb); // linear as read (see the model master)
	c *= s * Scale.x;
}
return c * lerp(float3(1.0, 1.0, 1.0), TopTint, top);
'''


# ------------------------------------------------------------------------------------------------ Unreal side

class UE(object):
    """Everything that needs the editor."""

    def __init__(self, args):
        self.args = args
        self.tools = unreal.AssetToolsHelpers.get_asset_tools()
        self.eal = unreal.EditorAssetLibrary
        self.mel = unreal.MaterialEditingLibrary
        self.provenance = dict(textures=[], materials=[], models=[])
        self.maps = {}

    # ---------- assets
    def path(self, sub, name):
        return '%s/%s/%s' % (CONTENT_ROOT, sub, name)

    def uasset_file(self, asset_path):
        rel = asset_path.split('.')[0][len('/Game/'):]
        return os.path.join(unreal.Paths.convert_relative_path_to_full(unreal.Paths.project_content_dir()), rel + '.uasset')

    def save(self, asset):
        self.eal.save_loaded_asset(asset, only_if_is_dirty=False)

    def create(self, sub, name, cls, factory):
        full = self.path(sub, name)
        if self.eal.does_asset_exist(full):
            asset = self.eal.load_asset(full)
            if asset and isinstance(asset, cls):
                return asset
            self.eal.delete_asset(full)
        return self.tools.create_asset(name, '%s/%s' % (CONTENT_ROOT, sub), cls, factory)

    def import_file(self, src, sub, name):
        task = unreal.AssetImportTask()
        task.set_editor_property('filename', src)
        task.set_editor_property('destination_path', '%s/%s' % (CONTENT_ROOT, sub))
        task.set_editor_property('destination_name', name)
        task.set_editor_property('automated', True)
        task.set_editor_property('replace_existing', True)
        task.set_editor_property('save', False)
        self.tools.import_asset_tasks([task])
        asset = self.eal.load_asset(self.path(sub, name))
        if not asset:
            raise RuntimeError('import failed: %s' % src)
        return asset

    def record_texture(self, src, tex, sources=None, generated=None):
        entry = dict(asset=tex.get_path_name().split('.')[0], uasset_sha256=None)
        if src:
            entry.update(source=rel_or_abs(src), source_sha256=sha256_file(src))
        if sources:
            entry['sources'] = [dict(source=rel_or_abs(s), source_sha256=sha256_file(s)) for s in sources]
        if generated:
            entry['generated_by'] = generated
        self.provenance['textures'].append(entry)

    def texture(self, src, sub, name, srgb=True, compression=None, wrap=True, generated=None, original=None, stream=True):
        tex = self.import_file(src, sub, name)
        tex.set_editor_property('srgb', srgb)
        # the infantry atlas packs its charts on black: a streamed-out mip bleeds the black into every seam
        tex.set_editor_property('never_stream', not stream)
        if compression is not None:
            tex.set_editor_property('compression_settings', compression)
        address = unreal.TextureAddress.TA_WRAP if wrap else unreal.TextureAddress.TA_CLAMP
        tex.set_editor_property('address_x', address)
        tex.set_editor_property('address_y', address)
        self.save(tex)
        self.record_texture(original or src, tex, generated=generated)
        return tex

    # ---------- textures
    def textures(self):
        ref = self.args.reference
        layers = [self.texture(layer_file(n, ref), 'Textures/Layers', 'T_Layer_' + safe(n)) for n in MATS]
        unreal.SystemLibrary.execute_console_command(None, 'r.AllowTexture2DArrayCreation 1')
        arr = self.create('Textures', 'TA_ModelLayers', unreal.Texture2DArray, unreal.Texture2DArrayFactory())
        arr.set_editor_property('source_textures', layers)
        arr.set_editor_property('address_x', unreal.TextureAddress.TA_WRAP)
        arr.set_editor_property('address_y', unreal.TextureAddress.TA_WRAP)
        arr.set_editor_property('srgb', True)
        arr.set_editor_property('mip_gen_settings', unreal.TextureMipGenSettings.TMGS_FROM_TEXTURE_GROUP)
        arr.set_editor_property('max_texture_size', SIZE)
        self.save(arr)
        self.record_texture(None, arr, sources=[layer_file(n, ref) for n in MATS])
        self.layers = arr
        models = os.path.join(ref, 'client/textures/models')
        self.infantry = (self.texture(os.path.join(models, 'infantry-v3-color.jpg'), 'Textures', 'T_InfantryColor', stream=False),
                         self.texture(os.path.join(models, 'infantry-v3-normal.jpg'), 'Textures', 'T_InfantryNormal', srgb=False,
                                      compression=unreal.TextureCompressionSettings.TC_VECTOR_DISPLACEMENTMAP, stream=False))
        # structures.js MODEL_MAT samples detail.jpg without color conversion (NoColorSpace)
        self.detail = self.texture(os.path.join(ref, 'client/textures/detail.jpg'), 'Textures', 'T_StructureDetail', srgb=False)
        self.surface_tex = {}
        for key, s in sorted(SURFACES.items()):
            if s['tex'] is None:
                png = os.path.join(self.args.work, 'T_Surface_brick.png')
                os.makedirs(self.args.work, exist_ok=True)
                with open(png, 'wb') as f:
                    f.write(brick_png())
                src, name = png, 'T_Surface_brick'
            else:
                src, name = os.path.join(ref, 'client/textures', s['tex'] + '.jpg'), 'T_Surface_' + safe(s['tex'])
            if name not in self.surface_tex:
                self.surface_tex[name] = self.texture(src, 'Textures/Surfaces', name, generated='client/surfaces.js brickCanvas, ported' if s['tex'] is None else None)
            s['asset'] = self.surface_tex[name]

    def map_texture(self, rel, wrap):
        """A three map a material binds (foliage bark and leaves). WebP goes through ImageMagick to PNG first."""
        if rel in self.maps:
            return self.maps[rel]
        src = os.path.join(self.args.reference, rel)
        name = 'T_Map_' + safe(os.path.splitext(os.path.basename(rel))[0])
        use = src
        if src.lower().endswith('.webp'):
            tool = shutil.which('magick')
            if not tool:
                raise RuntimeError('%s is WebP: ImageMagick (magick) converts it' % rel)
            use = os.path.join(self.args.work, name + '.png')
            os.makedirs(self.args.work, exist_ok=True)
            subprocess.run([tool, src, use], check=True)
        self.maps[rel] = self.texture(use, 'Textures/Maps', name, wrap=wrap, original=src,
                                      generated='magick %s -> png' % os.path.basename(rel) if use != src else None)
        return self.maps[rel]

    # ---------- materials
    def expr(self, mat, cls, x, y, **props):
        e = self.mel.create_material_expression(mat, cls, x, y)
        for k, v in props.items():
            e.set_editor_property(k, v)
        return e

    def custom(self, mat, code, inputs, outputs, ret, x, y, desc):
        c = self.expr(mat, unreal.MaterialExpressionCustom, x, y)
        c.set_editor_property('code', code)
        c.set_editor_property('description', desc)
        c.set_editor_property('output_type', ret)
        ins = []
        for n, _ in inputs:
            ci = unreal.CustomInput()
            ci.set_editor_property('input_name', n)
            ins.append(ci)
        c.set_editor_property('inputs', ins)
        outs = []
        for n, t in outputs:
            o = unreal.CustomOutput()
            o.set_editor_property('output_name', n)
            o.set_editor_property('output_type', t)
            outs.append(o)
        c.set_editor_property('additional_outputs', outs)
        for n, src in inputs:
            if not self.mel.connect_material_expressions(src, '', c, n):
                raise RuntimeError('could not connect %s' % n)
        return c

    def usage(self, mat, usage):
        if hasattr(self.mel, 'set_base_material_usage'):
            self.mel.set_base_material_usage(mat, usage)
        else:
            self.mel.set_material_usage(mat, usage)

    def model_master(self, name, masked, two_sided, means):
        mat = self.create('Materials', name, unreal.Material, unreal.MaterialFactoryNew())
        self.mel.delete_all_material_expressions(mat)
        mat.set_editor_property('blend_mode', unreal.BlendMode.BLEND_MASKED if masked else unreal.BlendMode.BLEND_OPAQUE)
        mat.set_editor_property('two_sided', two_sided)
        mat.set_editor_property('tangent_space_normal', False)
        mat.set_editor_property('shading_model', unreal.MaterialShadingModel.MSM_DEFAULT_LIT)
        T = unreal.CustomMaterialOutputType
        y = [0]

        def row():
            y[0] += 110
            return y[0]
        X = -1500

        def scalar(n, v):
            return self.expr(mat, unreal.MaterialExpressionScalarParameter, X, row(), parameter_name=n, default_value=v)

        def vector(n, v):
            return self.expr(mat, unreal.MaterialExpressionVectorParameter, X, row(), parameter_name=n, default_value=unreal.LinearColor(v[0], v[1], v[2], 1.0))

        def texobj(n, t):
            return self.expr(mat, unreal.MaterialExpressionTextureObjectParameter, X, row(), parameter_name=n, texture=t)

        def uv(i):
            return self.expr(mat, unreal.MaterialExpressionTextureCoordinate, X, row(), coordinate_index=i)

        def instance_data(index, default_param, three):
            cls = unreal.MaterialExpressionPerInstanceCustomData3Vector if three else unreal.MaterialExpressionPerInstanceCustomData
            e = self.expr(mat, cls, X + 250, y[0], data_index=index)
            self.mel.connect_material_expressions(default_param, '', e, 'DefaultValue')
            return e
        vn = self.expr(mat, unreal.MaterialExpressionVertexNormalWS, X, row())
        ln = self.expr(mat, unreal.MaterialExpressionTransform, X + 250, y[0], transform_source_type=unreal.MaterialVectorCoordTransformSource.TRANSFORMSOURCE_WORLD,
                       transform_type=unreal.MaterialVectorCoordTransform.TRANSFORM_INSTANCE)
        self.mel.connect_material_expressions(vn, '', ln, '')
        atlas_uv = uv(0)
        map_tex = texobj('Map', self.infantry[0])
        map_on, map_flip = scalar('MapOn', 0.0), scalar('MapFlipY', 1.0)
        inputs = [('VertexColor', self.expr(mat, unreal.MaterialExpressionVertexColor, X, row())), ('AtlasUV', atlas_uv), ('Mat', uv(1)), ('Tint', uv(2)), ('Scale', uv(3)),
                  ('LocalPosition', self.expr(mat, unreal.MaterialExpressionLocalPosition, X, row(), local_origin=unreal.LocalPositionOrigin.INSTANCE)),
                  ('LocalNormal', ln), ('VertexNormal', vn),
                  ('WorldPos', self.expr(mat, unreal.MaterialExpressionWorldPosition, X, row())),
                  ('CameraVector', self.expr(mat, unreal.MaterialExpressionCameraVectorWS, X, row())),
                  ('Owner', instance_data(CPD_OWNER, vector('OwnerColor', hex_linear(NEUTRAL)), True)),
                  ('Vehicle', instance_data(CPD_VEHICLE, vector('VehicleColor', (0.0, 0.0, 0.0)), True)),
                  ('Damage', instance_data(CPD_DAMAGE, scalar('Damage', 0.0), False)),
                  ('Layers', texobj('ModelLayers', self.layers)), ('InfantryColor', texobj('InfantryColor', self.infantry[0])),
                  ('InfantryNormal', texobj('InfantryNormal', self.infantry[1])), ('Detail', texobj('StructureDetail', self.detail)), ('Map', map_tex),
                  ('MaterialColor', vector('MaterialColor', (1.0, 1.0, 1.0))), ('RimColor', vector('RimColor', RIM)), ('WreckColor', vector('WreckColor', hex_linear(WRECK))),
                  ('DefaultMat', scalar('DefaultMat', 0.0)), ('BaseRoughness', scalar('BaseRoughness', 0.85)), ('BaseMetalness', scalar('BaseMetalness', 0.0)),
                  ('BaseSpecular', scalar('BaseSpecular', 0.5)), ('Grime', scalar('Grime', 1.0)), ('ModelTextures', scalar('ModelTextures', 1.0)),
                  ('InfantryReady', scalar('InfantryReady', 0.0)), ('InfantryDetail', scalar('InfantryDetail', 1.0)), ('Unlit', scalar('Unlit', 0.0)), ('OwnerMaterial', scalar('OwnerMaterial', 0.0)),
                  ('StructureDetail', scalar('StructureDetail', 0.0)), ('MapOn', map_on), ('MapFlipY', map_flip)]
        code = model_hlsl(means)
        c = self.custom(mat, code, inputs, [('Roughness', T.CMOT_FLOAT1), ('Metallic', T.CMOT_FLOAT1), ('Specular', T.CMOT_FLOAT1),
                                            ('NormalWS', T.CMOT_FLOAT3), ('Emissive', T.CMOT_FLOAT3)], T.CMOT_FLOAT3, -700, 0, 'TCModel')
        P = unreal.MaterialProperty
        for out, prop in (('', P.MP_BASE_COLOR), ('Roughness', P.MP_ROUGHNESS), ('Metallic', P.MP_METALLIC), ('Specular', P.MP_SPECULAR),
                          ('NormalWS', P.MP_NORMAL), ('Emissive', P.MP_EMISSIVE_COLOR)):
            if not self.mel.connect_material_property(c, out, prop):
                raise RuntimeError('could not connect %s' % prop)
        if masked:
            # the instance's fade (camouflage, ghosts; 1 solid) times the material's own (the propeller disc's 0.08):
            # an instance written solid must not lift a see-through material to opaque
            inst_fade = instance_data(CPD_FADE, self.expr(mat, unreal.MaterialExpressionConstant, X, row(), r=1.0), False)
            fade = self.expr(mat, unreal.MaterialExpressionMultiply, X + 500, y[0])
            self.mel.connect_material_expressions(inst_fade, '', fade, 'A')
            self.mel.connect_material_expressions(scalar('Fade', 1.0), '', fade, 'B')
            oc = self.custom(mat, OPACITY_HLSL, [('Fade', fade), ('Map', map_tex), ('MapOn', map_on), ('AlphaTest', scalar('AlphaTest', 0.0)),
                                                 ('AtlasUV', atlas_uv), ('MapFlipY', map_flip)], [], T.CMOT_FLOAT1, -700, 700, 'TCModelOpacity')
            self.mel.connect_material_property(oc, '', P.MP_OPACITY_MASK)
            mat.set_editor_property('opacity_mask_clip_value', 0.5)
        self.usage(mat, unreal.MaterialUsage.MATUSAGE_INSTANCED_STATIC_MESHES)
        self.mel.recompile_material(mat)
        self.save(mat)
        self.provenance['materials'].append(dict(asset=mat.get_path_name().split('.')[0], hlsl_sha256=hashlib.sha256((code + (OPACITY_HLSL if masked else '')).encode()).hexdigest(),
                                                 generated_from=['client/model-textures.js', 'client/infantry-material.js', 'client/models/blender-baked.js',
                                                                 'client/unit-models.js', 'client/structures.js MODEL_MAT', 'client/main.js']))
        return mat

    def surface_master(self):
        mat = self.create('Materials', 'M_TCSurface', unreal.Material, unreal.MaterialFactoryNew())
        self.mel.delete_all_material_expressions(mat)
        T = unreal.CustomMaterialOutputType
        first = next(iter(self.surface_tex.values()))
        inputs = [('WorldPos', self.expr(mat, unreal.MaterialExpressionWorldPosition, -1200, 0)),
                  ('WorldNormal', self.expr(mat, unreal.MaterialExpressionVertexNormalWS, -1200, 110)),
                  ('VertexColor', self.expr(mat, unreal.MaterialExpressionVertexColor, -1200, 220)),
                  ('Scale', self.expr(mat, unreal.MaterialExpressionTextureCoordinate, -1200, 330, coordinate_index=3)),
                  ('Tex', self.expr(mat, unreal.MaterialExpressionTextureObjectParameter, -1200, 440, parameter_name='Surface', texture=first)),
                  ('UVScale', self.expr(mat, unreal.MaterialExpressionScalarParameter, -1200, 550, parameter_name='UVScale', default_value=1.0)),
                  ('Tint', self.expr(mat, unreal.MaterialExpressionVectorParameter, -1200, 660, parameter_name='Tint', default_value=unreal.LinearColor(1, 1, 1, 1))),
                  ('TopTint', self.expr(mat, unreal.MaterialExpressionVectorParameter, -1200, 770, parameter_name='TopTint', default_value=unreal.LinearColor(1, 1, 1, 1))),
                  ('Painted', self.expr(mat, unreal.MaterialExpressionScalarParameter, -1200, 880, parameter_name='Painted', default_value=0.0))]
        c = self.custom(mat, SURFACE_HLSL, inputs, [], T.CMOT_FLOAT3, -600, 0, 'TCSurface')
        P = unreal.MaterialProperty
        self.mel.connect_material_property(c, '', P.MP_BASE_COLOR)
        # Lambert: fully rough, no specular
        self.mel.connect_material_property(self.expr(mat, unreal.MaterialExpressionConstant, -300, 200, r=1.0), '', P.MP_ROUGHNESS)
        self.mel.connect_material_property(self.expr(mat, unreal.MaterialExpressionConstant, -300, 300, r=0.0), '', P.MP_SPECULAR)
        self.usage(mat, unreal.MaterialUsage.MATUSAGE_INSTANCED_STATIC_MESHES)
        self.mel.recompile_material(mat)
        self.save(mat)
        self.provenance['materials'].append(dict(asset=mat.get_path_name().split('.')[0], hlsl_sha256=hashlib.sha256(SURFACE_HLSL.encode()).hexdigest(), generated_from=['client/surfaces.js']))
        return mat

    def instance(self, name, parent, scalars=None, vectors=None, textures=None):
        mi = self.create('Materials/Instances', name, unreal.MaterialInstanceConstant, unreal.MaterialInstanceConstantFactoryNew())
        self.mel.set_material_instance_parent(mi, parent)
        self.mel.clear_all_material_instance_parameters(mi)
        for k, v in sorted((scalars or {}).items()):
            self.mel.set_material_instance_scalar_parameter_value(mi, k, float(v))
        for k, v in sorted((vectors or {}).items()):
            self.mel.set_material_instance_vector_parameter_value(mi, k, unreal.LinearColor(v[0], v[1], v[2], 1.0))
        for k, v in sorted((textures or {}).items()):
            self.mel.set_material_instance_texture_parameter_value(mi, k, v)
        self.mel.update_material_instance(mi)
        self.save(mi)
        return mi

    def masters_and_instances(self, means, specs):
        self.masters = {name: self.model_master(name, 'Faded' in name, 'TwoSided' in name, means)
                        for name in ('M_TCModel', 'M_TCModelTwoSided', 'M_TCModelFaded', 'M_TCModelFadedTwoSided')}
        self.surface = self.surface_master()
        self.slot_material, self.slot_faded, self.slot_info = {}, {}, {}
        for slot, spec in sorted(specs.items()):
            if spec['family'] == 'surface':
                s = SURFACES[spec['surface']]
                mi = self.instance('MI_TC' + slot, self.surface, dict(UVScale=1.0 / s['size'], Painted=1.0 if spec['painted'] else 0.0),
                                   dict(Tint=s['tint'], TopTint=s['top']), dict(Surface=s['asset']))
                self.slot_material[slot] = self.slot_faded[slot] = mi  # no faded surface yet: ghosted pieces draw solid
            else:
                textures = {}
                if spec.get('map'):
                    textures['Map'] = self.map_texture(spec['map'], wrap=not spec['map'].endswith('.webp'))
                scalars = dict(spec['scalars'], Fade=spec.get('opacity', 1.0))
                mi = self.instance('MI_TC' + slot, self.masters[spec['master']], scalars, spec['vectors'], textures)
                self.slot_material[slot] = mi
                if 'Faded' in spec['master']:
                    self.slot_faded[slot] = mi
                else:
                    faded = spec['master'].replace('M_TCModel', 'M_TCModelFaded')
                    self.slot_faded[slot] = self.instance('MI_TC' + slot + '_Faded', self.masters[faded], scalars, spec['vectors'], textures)
            self.slot_info[slot] = dict(label=spec.get('label'), material=self.slot_material[slot].get_path_name(), gaps=spec.get('gaps', []))

    # ---------- meshes
    def import_part(self, model, part):
        dest = '%s/Meshes/%s' % (CONTENT_ROOT, safe(model['type']))
        pipe = unreal.InterchangeGenericAssetsPipeline()
        common = pipe.get_editor_property('common_meshes_properties')
        common.set_editor_property('force_all_mesh_as_type', unreal.InterchangeForceMeshType.IFMT_STATIC_MESH)
        common.set_editor_property('bake_meshes', True)
        common.set_editor_property('recompute_normals', False)
        common.set_editor_property('recompute_tangents', True)
        common.set_editor_property('use_full_precision_u_vs', True)
        common.set_editor_property('remove_degenerates', False)
        common.set_editor_property('import_sockets', False)
        common.set_editor_property('vertex_color_import_option', unreal.InterchangeVertexColorImportOption.IVCIO_REPLACE)
        common.set_editor_property('convert_statics_with_morph_targets_to_skeletals', False)
        mesh = pipe.get_editor_property('mesh_pipeline')
        mesh.set_editor_property('import_static_meshes', True)
        mesh.set_editor_property('import_skeletal_meshes', False)
        mesh.set_editor_property('build_nanite', False)
        mesh.set_editor_property('generate_lightmap_u_vs', False)
        for prop, value in (('collision', False), ('import_collision_according_to_mesh_name', False)):
            try:
                mesh.set_editor_property(prop, value)
            except Exception as e:
                warn('mesh pipeline %s: %s' % (prop, e))
        matp = pipe.get_editor_property('material_pipeline')
        matp.set_editor_property('import_materials', False)
        try:
            matp.get_editor_property('texture_pipeline').set_editor_property('import_textures', False)
        except Exception as e:
            warn('texture pipeline: %s' % e)
        pipe.get_editor_property('animation_pipeline').set_editor_property('import_animations', False)
        params = unreal.ImportAssetParameters()
        params.set_editor_property('is_automated', True)
        params.set_editor_property('replace_existing', True)
        params.set_editor_property('destination_name', part['asset'])
        params.set_editor_property('override_pipelines', [unreal.SoftObjectPath(pipe.get_path_name())])
        source = unreal.InterchangeManager.create_source_data(part['glb'])
        result = unreal.InterchangeManager.get_interchange_manager_scripted().import_asset(dest, source, params)
        sm = self.eal.load_asset('%s/%s' % (dest, part['asset']))
        if not isinstance(sm, unreal.StaticMesh):
            raise RuntimeError('import of %s did not produce %s/%s (%r)' % (part['glb'], dest, part['asset'], result))
        return sm

    def finish_part(self, part, sm, children):
        mats, faded = [], []
        for i, sl in enumerate(list(sm.get_editor_property('static_materials'))):
            slot = str(sl.get_editor_property('material_slot_name'))
            if slot not in self.slot_material:
                raise RuntimeError('%s: slot %r has no material' % (part['asset'], slot))
            sm.set_material(i, self.slot_material[slot])
            mats.append(self.slot_material[slot].get_path_name())
            faded.append(self.slot_faded[slot].get_path_name())
        for s in list(sm.get_sockets_by_tag('tc-model') or []):  # this tool's sockets from an earlier import
            sm.remove_socket(s)
        names = []
        for s in part['sockets'] + [c['socket_on_parent'] for c in children]:
            if s['name'] in names:
                continue
            sock = unreal.StaticMeshSocket(sm)
            sock.set_editor_property('socket_name', s['name'])
            sock.set_editor_property('tag', 'tc-model')
            sock.set_editor_property('relative_location', unreal.Vector(*s['location']))
            sock.set_editor_property('relative_rotation', unreal.Quat(*s['quat']).rotator())
            sock.set_editor_property('relative_scale', unreal.Vector(*s['scale']))
            sm.add_socket(sock)
            names.append(s['name'])
        self.save(sm)
        return mats, faded, names

    def models(self, manifest, normalized):
        catalog, done = [], {}
        total = sum(1 for n in normalized for p in n['parts'] if 'same_as' not in p)
        count = 0
        for model, norm in zip(manifest['models'], normalized):
            parts = norm['parts']
            entry = dict(key=model['key'], type=model['type'], faction=model['faction'], variant=model.get('variant', ''),
                         look=model.get('look') or model.get('category', ''), category=model.get('category', ''), scale=float(model.get('scale', 1.0)), parts=[])
            prov = dict(key=model['key'], source=rel_or_abs(norm['source']), source_sha256=norm['source_sha256'], manifest_sha256=model.get('sha256'),
                        warnings=norm.get('warnings', []), parts=[])
            if model.get('sha256') and model['sha256'] != norm['source_sha256']:
                raise RuntimeError('%s: glb sha256 %s does not match the manifest %s' % (model['key'], norm['source_sha256'], model['sha256']))
            for part in parts:
                if 'same_as' in part:
                    path, mats, faded, sockets = done[part['glb_sha256']]
                else:
                    count += 1
                    children = [c for c in parts if c.get('parent') == part['name']]
                    sm = self.import_part(model, part)
                    mats, faded, sockets = self.finish_part(part, sm, children)
                    path = sm.get_path_name()
                    done[part['glb_sha256']] = (path, mats, faded, sockets)
                    if count % 25 == 0:
                        log('imported %d of %d meshes' % (count, total))
                entry['parts'].append(dict(name=part['name'], mesh=path, parent=part.get('parent') or '', socket=part['name'] if part.get('parent') else '',
                                           materials=mats, faded_materials=faded, sockets=sockets, rest=part['rest'], hidden=part['hidden'], opacity=part.get('opacity', 1.0)))
                prov['parts'].append(dict(part=part['name'], normalized_glb_sha256=part['glb_sha256'], asset=path.split('.')[0],
                                          uasset_sha256=None if 'same_as' in part else sha256_file(self.uasset_file(path)),
                                          same_as=part.get('same_as'), stats=part['stats'], bounds_cm=part['bounds'], sockets=sockets,
                                          vehicle_tint=part.get('vehicle_tint')))
            catalog.append(entry)
            self.provenance['models'].append(prov)
        return catalog


def catalog_document(models, factions, colors, commit, slot_info):
    return dict(format='tc-model-catalog-v1', reference_commit=commit,
                custom_data=dict(floats=CPD_COUNT, owner=CPD_OWNER, vehicle=CPD_VEHICLE, fade=CPD_FADE, damage=CPD_DAMAGE),
                factions=[dict(name=f['name'], vehicle=hex_linear(f['vehicle']), uniform=hex_linear(f['uniform'])) for f in factions],
                seat_colors=[hex_linear(c) for c in colors], neutral_color=hex_linear(NEUTRAL),
                fade=dict(camouflage=CAMO_OPACITY, ghost=GHOST_OPACITY), materials=slot_info, models=sorted(models, key=lambda m: m['key']))


# ------------------------------------------------------------------------------------------------ checks inside Unreal

def content_models_dir():
    return os.path.join(unreal.Paths.convert_relative_path_to_full(unreal.Paths.project_content_dir()), 'Models')


def verify_unreal(args):
    """Load every mesh the catalog names and check it against the contract: per triangle corner, the material ids,
    owner weights, atlas flags and color scales in UV1 to UV3 equal the normalized glb's; the catalog's materials sit in
    its slots; sockets, bounds and triangle counts match; Nanite is off."""
    catalog = json.load(open(os.path.join(content_models_dir(), 'ModelCatalog.json'), encoding='utf-8'))
    prov = json.load(open(os.path.join(content_models_dir(), 'ImportProvenance.json'), encoding='utf-8'))
    stats = {}
    for m in prov['models']:
        for p in m['parts']:
            stats.setdefault(p['asset'], p)
    report, failures, seen = dict(meshes=[], textures={}), [], set()
    for model in catalog['models']:
        for part in model['parts']:
            if part['mesh'] in seen:
                continue
            seen.add(part['mesh'])
            sm = unreal.load_asset(part['mesh'])
            r = dict(key=model['key'], part=part['name'], mesh=part['mesh'])
            if not isinstance(sm, unreal.StaticMesh):
                failures.append('%s: not a static mesh' % part['mesh'])
                continue
            r['vertices'], r['triangles'], r['sections'], r['lods'] = sm.get_num_vertices(0), sm.get_num_triangles(0), sm.get_num_sections(0), sm.get_num_lods()
            r['nanite'] = bool(sm.get_editor_property('nanite_settings').get_editor_property('enabled'))
            b = sm.get_bounding_box()
            r['bounds_cm'] = dict(min=[b.min.x, b.min.y, b.min.z], max=[b.max.x, b.max.y, b.max.z])
            r['materials'] = [sm.get_material(i).get_path_name() if sm.get_material(i) else None for i in range(len(sm.get_editor_property('static_materials')))]
            r['sockets'] = {}
            for name in part['sockets']:
                sock = sm.find_socket(name)
                if not sock:
                    failures.append('%s: socket %s missing' % (part['mesh'], name))
                    continue
                loc = sock.get_editor_property('relative_location')
                r['sockets'][name] = [loc.x, loc.y, loc.z]
            # the editor's mesh queries need the editor subsystem, which a commandlet lacks: read the mesh description
            desc = sm.get_static_mesh_description(0)
            layers, owner, atlas, scaled = {}, 0, 0, 0
            for i in range(desc.get_vertex_instance_count()):
                vi = unreal.VertexInstanceID(i)
                m1, m2, m3 = desc.get_vertex_instance_uv(vi, 1), desc.get_vertex_instance_uv(vi, 2), desc.get_vertex_instance_uv(vi, 3)
                lay = str(int(math.floor(m1.x + 0.25)))
                layers[lay] = layers.get(lay, 0) + 1
                owner += 1 if m2.x > 1e-3 else 0
                atlas += 1 if m1.y > 0.5 else 0
                scaled += 1 if m3.x > 1.0 + 1e-6 else 0
            r.update(corner_layers=layers, corner_owner=owner, corner_atlas=atlas, corners_scaled=scaled)
            if r['materials'] != part['materials']:
                failures.append('%s: materials %r, catalog %r' % (part['mesh'], r['materials'], part['materials']))
            if r['nanite']:
                failures.append('%s: Nanite is on' % part['mesh'])
            st = stats.get(part['mesh'].split('.')[0])
            if st:
                for k in ('corner_layers', 'corner_owner', 'corner_atlas'):
                    if r[k] != st['stats'][k]:
                        failures.append('%s: %s %r in Unreal, %r in the normalized glb' % (part['mesh'], k, r[k], st['stats'][k]))
                if r['triangles'] != st['stats']['triangles'] - st['stats'].get('degenerate', 0):
                    failures.append('%s: %d triangles, normalized glb %d less %d degenerate' % (part['mesh'], r['triangles'], st['stats']['triangles'], st['stats'].get('degenerate', 0)))
                for k in ('min', 'max'):
                    if any(abs(a - c) > 0.05 for a, c in zip(r['bounds_cm'][k], st['bounds_cm'][k])):
                        failures.append('%s: bounds %s %r, normalized glb %r' % (part['mesh'], k, r['bounds_cm'][k], st['bounds_cm'][k]))
            else:
                failures.append('%s: not in the provenance' % part['mesh'])
            report['meshes'].append(r)
    arr = unreal.load_asset(CONTENT_ROOT + '/Textures/TA_ModelLayers')
    src = arr.get_editor_property('source_textures')
    report['textures']['TA_ModelLayers'] = dict(slices=len(src), order=[t.get_name() for t in src], srgb=arr.get_editor_property('srgb'),
                                                mip_gen=str(arr.get_editor_property('mip_gen_settings')))
    if [t.get_name() for t in src] != ['T_Layer_' + safe(n) for n in MATS]:
        failures.append('TA_ModelLayers is not in MATS order')
    report['failures'] = failures
    write_json(args.verify, report)
    log('verify: %d meshes, %s' % (len(report['meshes']), 'ok' if not failures else 'FAILED (%d): %r' % (len(failures), failures[:10])))


DEFAULT_RENDER = 'rifle|1,medium|0,armoredcar|0,tiger|1,destroyer|2,at|2,fighter|3,barracks|0,flakpos|1,kaiju|0,foliage/broadleaf|-1'


def render_unreal(args):
    """Draw catalog models with instanced static meshes and their custom data from a three-quarter view: owner blue,
    owner red, a wreck (damage 1) and a camouflaged one (the faded materials at fade 0.45), then a close look."""
    catalog = json.load(open(os.path.join(content_models_dir(), 'ModelCatalog.json'), encoding='utf-8'))
    want = [k for k in (args.render_keys or DEFAULT_RENDER).split(',') if k]
    models = [m for k in want for m in catalog['models'] if m['key'] == k]
    world = unreal.EditorLoadingAndSavingUtils.new_blank_map(False)  # unsaved, nothing written
    eas = unreal.get_editor_subsystem(unreal.EditorActorSubsystem)
    sds = unreal.get_engine_subsystem(unreal.SubobjectDataSubsystem)

    def add_ism(actor):
        handles = sds.k2_gather_subobject_data_for_instance(actor)
        params = unreal.AddNewSubobjectParams(parent_handle=handles[0], new_class=unreal.InstancedStaticMeshComponent, blueprint_context=None)
        handle, fail = sds.add_new_subobject(params)
        if not fail.is_empty():
            raise RuntimeError('could not add an instanced static mesh component: %s' % fail)
        return unreal.SubobjectDataBlueprintFunctionLibrary.get_object(unreal.SubobjectDataBlueprintFunctionLibrary.get_data(handle))
    sun = eas.spawn_actor_from_class(unreal.DirectionalLight, unreal.Vector(0, 0, 1000), unreal.Rotator(roll=0, pitch=-42, yaw=-150))
    sun.light_component.set_intensity(6.0)
    fill = eas.spawn_actor_from_class(unreal.DirectionalLight, unreal.Vector(0, 0, 1000), unreal.Rotator(roll=0, pitch=-30, yaw=30))
    fill.light_component.set_intensity(2.0)
    fill.light_component.set_light_color(unreal.LinearColor(0.6, 0.7, 0.9, 1))
    fill.light_component.set_editor_property('cast_shadows', False)
    floor = eas.spawn_actor_from_class(unreal.StaticMeshActor, unreal.Vector(0, 0, -1), unreal.Rotator())
    floor.static_mesh_component.set_static_mesh(unreal.load_asset('/Engine/BasicShapes/Plane.Plane'))
    floor.set_actor_scale3d(unreal.Vector(2000, 2000, 1))
    floor.static_mesh_component.set_material(0, unreal.load_asset('/Engine/BasicShapes/BasicShapeMaterial.BasicShapeMaterial'))
    cap = eas.spawn_actor_from_class(unreal.SceneCapture2D, unreal.Vector(0, 0, 0), unreal.Rotator())
    comp = cap.capture_component2d
    rt = unreal.RenderingLibrary.create_render_target2d(world, 1280, 720, unreal.TextureRenderTargetFormat.RTF_RGBA8_SRGB, unreal.LinearColor(0, 0, 0, 1), False)
    comp.set_editor_property('texture_target', rt)
    comp.set_editor_property('capture_source', unreal.SceneCaptureSource.SCS_FINAL_TONE_CURVE_HDR)
    comp.set_editor_property('capture_every_frame', False)
    comp.set_editor_property('fov_angle', 35.0)
    pp = comp.get_editor_property('post_process_settings')
    pp.set_editor_property('override_auto_exposure_method', True)
    pp.set_editor_property('auto_exposure_method', unreal.AutoExposureMethod.AEM_MANUAL)
    pp.set_editor_property('override_auto_exposure_bias', True)
    pp.set_editor_property('auto_exposure_bias', 11.0)
    comp.set_editor_property('post_process_settings', pp)
    os.makedirs(args.render, exist_ok=True)
    seats, factions = catalog['seat_colors'], catalog['factions']
    looks = [(seats[0], 0.0, 1.0, False), (seats[1], 0.0, 1.0, False), (seats[0], 1.0, 1.0, False), (seats[1], 0.0, catalog['fade']['camouflage'], True)]
    written = []
    for model in models:
        holder = eas.spawn_actor_from_class(unreal.StaticMeshActor, unreal.Vector(0, 0, 0), unreal.Rotator())
        lo, hi = [1e9] * 3, [-1e9] * 3
        drawn = [p for p in model['parts'] if not p['hidden']]
        for part in drawn:
            b = unreal.load_asset(part['mesh']).get_bounding_box()
            r = part['rest']
            for k, (a, c) in enumerate(((b.min.x, b.max.x), (b.min.y, b.max.y), (b.min.z, b.max.z))):
                lo[k] = min(lo[k], r['location'][k] + a * r['scale'][k])
                hi[k] = max(hi[k], r['location'][k] + c * r['scale'][k])
        width = max(hi[0] - lo[0], hi[1] - lo[1], 50.0)
        spacing = width * 1.25
        vehicle = factions[model['faction']]['vehicle'] if 0 <= model['faction'] < len(factions) else [0, 0, 0]
        comps = {}
        for li, (owner, damage, fade, faded) in enumerate(looks):
            for part in drawn:
                key = (part['mesh'], faded)
                ism = comps.get(key)
                if ism is None:
                    ism = add_ism(holder)
                    ism.set_static_mesh(unreal.load_asset(part['mesh']))
                    ism.set_editor_property('num_custom_data_floats', catalog['custom_data']['floats'])
                    for i, mpath in enumerate(part['faded_materials'] if faded else part['materials']):
                        ism.set_material(i, unreal.load_asset(mpath))
                    comps[key] = ism
                r = part['rest']
                xf = unreal.Transform(location=unreal.Vector(r['location'][0], r['location'][1] + (li - 1.5) * spacing, r['location'][2]),
                                      rotation=unreal.Quat(*r['quat']).rotator(), scale=unreal.Vector(*r['scale']))
                idx = ism.add_instance(xf, False)
                for k, value in enumerate(list(owner) + list(vehicle) + [fade * part.get('opacity', 1.0), damage]):
                    ism.set_custom_data_value(idx, k, float(value), True)
        cz = (lo[2] + hi[2]) * 0.5
        views = [('row', unreal.Vector(spacing * 3.0, spacing * 2.6, spacing * 1.9 + cz), unreal.Vector(0, 0, cz)),
                 ('close', unreal.Vector(width * 0.9, -1.5 * spacing + width * 0.9, width * 0.55 + cz), unreal.Vector(0, -1.5 * spacing, cz))]
        for view, eye, target in views:
            comp.set_world_location_and_rotation(eye, unreal.MathLibrary.find_look_at_rotation(eye, target), False, False)
            # shaders compile asynchronously in a commandlet too: finish them (this also waits for meshes and textures),
            # then capture twice so the second frame has settled history
            unreal.AutomationLibrary.finish_loading_before_screenshot()
            comp.capture_scene()
            unreal.AutomationLibrary.finish_loading_before_screenshot()
            comp.capture_scene()
            name = 'model-%s-%s' % (safe(model['key']), view)
            unreal.RenderingLibrary.export_render_target(world, rt, args.render, name + '.png')
            written.append(name + '.png')
        eas.destroy_actor(holder)
    log('render: wrote %d images to %s: %s' % (len(written), args.render, ', '.join(written)))


# ------------------------------------------------------------------------------------------------ entry points

def parse_args(argv):
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('--manifest', default=os.path.join(PROJECT, 'Saved', 'ModelExport', 'manifest.json'))
    ap.add_argument('--reference', default=REFERENCE)
    ap.add_argument('--work', default=os.path.join(PROJECT, 'Saved', 'ModelImport'), help='normalized glb files (a cache) go here')
    ap.add_argument('--only', default='', help='comma-separated model keys, types or categories to import')
    ap.add_argument('--jobs', type=int, default=1, help='normalization processes (outside Unreal only)')
    ap.add_argument('--selftest', action='store_true', help='engine-free checks: reference tables, manifest, normalization')
    ap.add_argument('--skip-textures', action='store_true', help='reuse the textures already imported')
    ap.add_argument('--verify', default='', help='inside Unreal: check every catalog mesh, write a JSON report here (no import)')
    ap.add_argument('--render', default='', help='inside Unreal with a renderer: draw catalog models into PNGs here (no import)')
    ap.add_argument('--render-keys', default='', help='models to render (default: %s)' % DEFAULT_RENDER)
    return ap.parse_args(argv)


def select(manifest, only):
    if not only:
        return manifest
    want = set(x.strip() for x in only.split(',') if x.strip())
    manifest = dict(manifest)
    manifest['models'] = [m for m in manifest['models'] if m['key'] in want or m['type'] in want or m.get('category') in want]
    return manifest


def selftest(args):
    failures = []
    bad = reference_parity(args.reference)
    log('reference tables: %s' % ('match' if not bad else '%d differences' % len(bad)))
    failures += bad
    q = to_unreal_quat(node_matrix(dict(rotation=[0, math.sin(math.pi / 4), 0, math.cos(math.pi / 4)])))
    if abs(abs(q[2]) - math.sin(math.pi / 4)) > 1e-9 or abs(q[0]) > 1e-9 or abs(q[1]) > 1e-9:
        failures.append('a yaw about the reference y axis should become a yaw about Unreal Z: %r' % (q,))
    if to_unreal_location((1, 2, 3)) != (100.0, 300.0, 200.0):
        failures.append('axis mapping')
    for label, want in (('unit-models.js VEHICLE_PAINT', True), ('unit-models.js PAINT', True), ('aircraft.js BODY_MAT', False), ('structures.js MODEL_MAT', False)):
        if label_grime(label, args.reference) != want:
            failures.append('grime of %s should be %s' % (label, want))
    png = brick_png()
    log('brick texture: %d bytes, sha256 %s' % (len(png), hashlib.sha256(png).hexdigest()))
    if os.path.exists(args.manifest):
        manifest = select(load_manifest(args.manifest), args.only)
        normalized = normalize_all(manifest['models'], args.work, args.reference, args.jobs)
        unique = verts = tris = 0
        for model, norm in zip(manifest['models'], normalized):
            if model.get('sha256') and model['sha256'] != norm['source_sha256']:
                failures.append('%s: sha256 mismatch' % model['key'])
            for p in norm['parts']:
                if 'same_as' in p:
                    continue
                unique += 1
                g, b = read_glb(open(p['glb'], 'rb').read())
                v = sum(g['accessors'][pr['attributes']['POSITION']]['count'] for pr in g['meshes'][0]['primitives'])
                t = sum(g['accessors'][pr['indices']]['count'] for pr in g['meshes'][0]['primitives']) // 3
                if (v, t) != (p['stats']['vertices'], p['stats']['triangles']):
                    failures.append('%s/%s: round trip %d/%d vs %d/%d' % (model['key'], p['name'], v, t, p['stats']['vertices'], p['stats']['triangles']))
                if len(g['meshes'][0]['primitives'][0]['attributes']) != 3 + UV_CHANNELS:
                    failures.append('%s/%s: attribute layout' % (model['key'], p['name']))
                verts += v
                tris += t
        parts = sum(len(n['parts']) for n in normalized)
        log('normalized %d models: %d parts, %d unique meshes, %d vertices, %d triangles, %d material slots' % (len(normalized), parts, unique, verts, tris, len(SPECS)))
        for slot, spec in sorted(SPECS.items()):
            log('  %-34s %-8s %-24s %s' % (slot, spec['family'], spec.get('master', 'M_TCSurface'), '; '.join(spec.get('gaps', [])) or '-'))
        warned = sorted({w for n in normalized for w in n.get('warnings', [])})
        for w in warned:
            log('  note: %s' % w)
    else:
        log('no manifest at %s; glb checks skipped' % args.manifest)
    for f in failures:
        warn(f)
    log('selftest: %s' % ('ok' if not failures else 'FAILED (%d)' % len(failures)))
    return not failures


def run_unreal(args):
    bad = reference_parity(args.reference)
    if bad:
        raise RuntimeError('reference tables differ from the pin: %r' % bad)
    manifest = select(load_manifest(args.manifest), args.only)
    factions, colors = reference_factions(args.reference)
    commit = reference_commit(args.reference)
    normalized = normalize_all(manifest['models'], args.work, args.reference, 1)
    log('normalized %d models' % len(normalized))
    used = {}
    for n in normalized:
        for p in n['parts']:
            for slot in p['slots']:
                used[slot] = SPECS[slot]
    ue = UE(args)
    means, how, magick = layer_means(args.reference)
    if args.skip_textures:
        load = lambda sub, name: unreal.load_asset(ue.path(sub, name))
        ue.layers = load('Textures', 'TA_ModelLayers')
        ue.infantry = (load('Textures', 'T_InfantryColor'), load('Textures', 'T_InfantryNormal'))
        ue.detail = load('Textures', 'T_StructureDetail')
        ue.surface_tex = {}
        for key, s in SURFACES.items():
            name = 'T_Surface_brick' if s['tex'] is None else 'T_Surface_' + safe(s['tex'])
            s['asset'] = ue.surface_tex.setdefault(name, load('Textures/Surfaces', name))
        for slot, spec in used.items():
            if spec.get('map'):
                ue.maps[spec['map']] = load('Textures/Maps', 'T_Map_' + safe(os.path.splitext(os.path.basename(spec['map']))[0]))
    else:
        ue.textures()
    ue.masters_and_instances(means, used)
    catalog_models = ue.models(manifest, normalized)
    out_dir = content_models_dir()
    cat_path, prov_path = os.path.join(out_dir, 'ModelCatalog.json'), os.path.join(out_dir, 'ImportProvenance.json')
    slot_info = ue.slot_info
    if args.only and os.path.exists(cat_path):
        old = json.load(open(cat_path, encoding='utf-8'))
        keep = {m['key']: m for m in old.get('models', [])}
        keep.update({m['key']: m for m in catalog_models})
        catalog_models = list(keep.values())
        slot_info = dict(old.get('materials', {}), **slot_info)
    write_json(cat_path, catalog_document(catalog_models, factions, colors, commit, slot_info))
    for group in ('textures', 'materials'):
        for t in ue.provenance[group]:
            f = ue.uasset_file(t['asset'])
            t['uasset_sha256'] = sha256_file(f) if os.path.exists(f) else None
    prov = dict(format='tc-model-provenance-v1', tool='unreal/Tools/models/import-models.py', tool_version=TOOL_VERSION, tool_sha256=sha256_file(os.path.abspath(__file__)),
                engine=unreal.SystemLibrary.get_engine_version(), reference_commit=commit, manifest=rel_or_abs(args.manifest),
                manifest_sha256=sha256_file(args.manifest), manifest_format=manifest.get('format'), manifest_generator=manifest.get('generator'),
                layer_means=dict(zip(MATS, means)), layer_means_tool=dict(how=how, magick=magick), **ue.provenance)
    if args.only and os.path.exists(prov_path):
        old = json.load(open(prov_path, encoding='utf-8'))
        keep = {m['key']: m for m in old.get('models', [])}
        keep.update({m['key']: m for m in prov['models']})
        prov['models'] = sorted(keep.values(), key=lambda m: m['key'])
        if args.skip_textures:
            prov['textures'] = old.get('textures', [])
            prov['materials'] = old.get('materials', []) + [m for m in prov['materials'] if m['asset'] not in {o['asset'] for o in old.get('materials', [])}]
    write_json(prov_path, prov)
    log('imported %d models (%d parts); catalog %s' % (len(catalog_models), sum(len(m['parts']) for m in catalog_models), cat_path))


def main(argv):
    args = parse_args(argv)
    args.manifest = os.path.abspath(args.manifest)
    args.work = os.path.abspath(args.work)
    if args.selftest or unreal is None:
        if unreal is None and not args.selftest:
            log('not inside Unreal: running the engine-free selftest only')
        ok = selftest(args)
        if unreal is None:
            sys.exit(0 if ok else 1)
        return
    if args.verify or args.render:
        if args.verify:
            verify_unreal(args)
        if args.render:
            render_unreal(args)
        return
    run_unreal(args)


if __name__ == '__main__':
    main(sys.argv[1:])
