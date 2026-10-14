"""Wheel spin and track scroll for the Unreal client (client/wheel-motion.js, client/track-motion.js): the tracked
hulls again, with the reference's per-vertex wheel and track data, and a model master that moves them. Run inside the
editor on a host project whose Content/Models is the real project's, AFTER unreal/Tools/models/import-models.py and
unreal/Tools/look/make-look-assets.py (the master copied here must be the patched one):
  UnrealEditor-Cmd <host>.uproject -run=pythonscript -script=make-wheel-tracks.py -unattended -nop4 -nosplash -nullrhi
  with MOTION_LOG=<result json>, MOTION_WORK=<scratch folder for the glbs> and MOTION_SAVED=<the project's Saved folder,
  which holds ModelExport/ and ModelImport/> (default: the host's ../../Saved of Content/Models' target)

What it reads (nothing is remodelled; every number comes from the reference's own build):
  Content/Models/ImportProvenance.json   which catalog mesh came from which reference glb and normalized glb (sha256)
  Saved/ModelExport/<glb>                the reference build (unreal/Tools/models/export-reference-models.mjs): the
                                         hull's _WHEELPIVOT (wheel centre xyz, radius), _WHEELSIDE (the side the wheel
                                         rolls with, wheel-motion.js wheelMesh), _TRACKDATA (phase, loop, side, floor)
                                         and mesh.extras.ww2.trackLoops (the closed loops in the hull's xy plane)
  Saved/ModelImport/<n>/SM_<n>_hull.glb  the importer's normalized glb, the exact vertices Unreal built the hull from
For every hull with wheels it writes a copy of the normalized glb with four more texture channels (vertex i of the
normalized glb is vertex i of the reference's, checked position by position):
  UV4 = wheelPivot.xy          UV5 = (wheelSide, wheelPivot.w: the radius, 0 static)
  UV6 = (trackData.x: phase, the loop's row in the shared path table, -1 not a link)
  UV7 = (trackData.z: the link's side, trackData.w: its floor)
in the reference's hull space (metres, three.js axes), and imports it with the importer's Interchange settings to
/Game/Models/Motion/Tracks/SM_<n>_hull_Tracks. The loops get rows in one table as track-motion.js register() does
(one row per distinct point list, JSON.stringify as the key), in a fixed order (assets sorted, loops in order), and are
written to Content/Models/Motion/WheelTracks.json for the client, which builds the 1024 x 128 RGBA32F path texture
from them at run time (Client/TCUnitMotion.h FTCTrackPaths, the reference's sampling).
The master: M_TCModelTracks, a copy of the patched M_TCModel with
  - world position offset: wheelMaterial's rotateWheel about the pivot and trackShader's trackFrame / trackTurn /
    trackDestination with the floor clamp (WHEEL_TRACK below; travel and turn from per-instance custom data 8 and 9,
    FTCPartMotion WheelTravel and WheelTurn), the path table as the texture parameter TrackPaths;
  - the lighting normal turned the same way (the TCModel and TCLookIndirect nodes' VertexNormal);
  - the model-space position and normal of the texture layers left as authored (LocalPosition without the offset):
    wheelMaterial keeps vModelPos and vModelNormal in the wheel's authored space so the surface turns with it.
and MI_TCModelTracks_VEHICLE_PAINT, MI_TCModel_VEHICLE_PAINT's copy on that master.
"""
import hashlib
import json
import os
import struct
import unreal

ROOT = '/Game/Models/Motion'
TRACKS = ROOT + '/Tracks'
MASTER = '/Game/Models/Materials/M_TCModel'
PAINT = '/Game/Models/Materials/Instances/MI_TCModel_VEHICLE_PAINT'
LOG_PATH = os.environ.get('MOTION_LOG', '')
content = unreal.Paths.convert_relative_path_to_full(unreal.Paths.project_content_dir())
models_dir = os.path.realpath(os.path.join(content, 'Models'))
SAVED = os.environ.get('MOTION_SAVED') or os.path.join(os.path.dirname(os.path.dirname(models_dir)), 'Saved')
WORK = os.environ.get('MOTION_WORK') or os.path.join(SAVED, 'MotionTracks')
os.makedirs(WORK, exist_ok=True)
lib = unreal.EditorAssetLibrary
mel = unreal.MaterialEditingLibrary
log = {'saved': SAVED, 'work': WORK, 'meshes': [], 'errors': [], 'assets': []}

WHEEL_TRACK = '''// client/wheel-motion.js wheelMaterial and client/track-motion.js trackShader (reference/ww2-rts @ e4e579a), in the
// hull's own space: three.js axes in metres, Unreal (x, y, z) cm = three (x, z, y) * 100. Mode 0: the local offset of
// the position (cm); Mode 1: the turned normal (unit, Unreal local axes).
// Pivot = wheelPivot.xy, Wheel = (wheelSide, wheelPivot.w), Track0 = (trackData.x, row or -1), Track1 = trackData.zw,
// Travel = the instance's wheelTravel (metres rolled, radians turned).
float3 t = float3(P.x, P.z, P.y) * (Mode > 0.5 ? 1.0 : 0.01);
// rotateWheel: a = -(travel + turn * side) / radius about the wheel's local z axle; radius 0 is static
float a = Wheel.y > 0.0 ? -(Travel.x + Travel.y * Wheel.x) / Wheel.y : 0.0;
float c = cos(a), s = sin(a);
if (Mode > 0.5) t.xy = float2(c * t.x - s * t.y, s * t.x + c * t.y);
else
{
    float2 q = t.xy - Pivot;
    t.xy = Pivot + float2(c * q.x - s * q.y, s * q.x + c * q.y);
}
if (Track0.y > -0.5)
{
    int row = (int)(Track0.y + 0.5);
    float len = Paths.Load(int3(0, row, 0)).z;
    if (len > 0.0)
    {
        // trackFrame(trackData.x) and trackDestination(): the sampled point and direction where the link rests and
        // where the travel has carried it along its loop
        float at0 = frac(Track0.x) * 1024.0;
        int i0 = (int)floor(at0);
        float4 a0 = Paths.Load(int3(i0, row, 0)), b0 = Paths.Load(int3((i0 + 1) % 1024, row, 0));
        float2 p0 = lerp(a0.xy, b0.xy, frac(at0)), d0 = normalize(b0.xy - a0.xy);
        float at1 = frac(Track0.x - (Travel.x + Travel.y * Track1.x) / len) * 1024.0;
        int i1 = (int)floor(at1);
        float4 a1 = Paths.Load(int3(i1, row, 0)), b1 = Paths.Load(int3((i1 + 1) % 1024, row, 0));
        float2 p1 = lerp(a1.xy, b1.xy, frac(at1)), d1 = normalize(b1.xy - a1.xy);
        // trackTurn: the rotation from the rest frame's direction to the travelled one
        float tc = dot(d0, d1), ts = d0.x * d1.y - d0.y * d1.x;
        if (Mode > 0.5) t.xy = float2(tc * t.x - ts * t.y, ts * t.x + tc * t.y);
        else
        {
            float2 r = t.xy - p0;
            t.xy = p1 + float2(tc * r.x - ts * r.y, ts * r.x + tc * r.y);
            t.y = max(t.y, Track1.y);
        }
    }
}
if (Mode > 0.5) return normalize(float3(t.x, t.z, t.y));
return float3(t.x, t.z, t.y) * 100.0 - P;'''


def sha256(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for chunk in iter(lambda: f.read(1 << 20), b''):
            h.update(chunk)
    return h.hexdigest()


# ------------------------------------------------------------------------------------------------ glb in and out

def read_glb(path):
    b = open(path, 'rb').read()
    magic, version, total = struct.unpack_from('<III', b, 0)
    if magic != 0x46546C67 or version != 2:
        raise RuntimeError('%s is not a glTF 2 binary' % path)
    jlen, jtype = struct.unpack_from('<II', b, 12)
    g = json.loads(b[20:20 + jlen])
    off = 20 + jlen
    blen, btype = struct.unpack_from('<II', b, off)
    return g, bytearray(b[off + 8:off + 8 + blen])


def accessor(g, data, index):
    a = g['accessors'][index]
    bv = g['bufferViews'][a['bufferView']]
    comps = {'SCALAR': 1, 'VEC2': 2, 'VEC3': 3, 'VEC4': 4}[a['type']]
    fmt = {5126: 'f', 5125: 'I', 5123: 'H', 5121: 'B'}[a['componentType']]
    size = struct.calcsize(fmt)
    stride = bv.get('byteStride', size * comps)
    base = bv.get('byteOffset', 0) + a.get('byteOffset', 0)
    return [struct.unpack_from('<%d%s' % (comps, fmt), data, base + k * stride) for k in range(a['count'])]


def write_glb(path, g, data, extra):
    """g and data with the extra (name, list of (u, v)) channels added as float VEC2 attributes of primitive 0."""
    prim = g['meshes'][0]['primitives'][0]
    for name, values in extra:
        while len(data) % 4:
            data.append(0)
        offset = len(data)
        data.extend(struct.pack('<%df' % (2 * len(values)), *[x for uv in values for x in uv]))
        g['bufferViews'].append(dict(buffer=0, byteOffset=offset, byteLength=8 * len(values), target=34962))
        g['accessors'].append(dict(bufferView=len(g['bufferViews']) - 1, componentType=5126, count=len(values), type='VEC2'))
        prim['attributes'][name] = len(g['accessors']) - 1
    while len(data) % 4:
        data.append(0)
    g['buffers'][0]['byteLength'] = len(data)
    j = json.dumps(g, separators=(',', ':'), sort_keys=True).encode()
    while len(j) % 4:
        j += b' '
    out = struct.pack('<III', 0x46546C67, 2, 12 + 8 + len(j) + 8 + len(data)) + struct.pack('<II', len(j), 0x4E4F534A) + j + struct.pack('<II', len(data), 0x004E4942) + bytes(data)
    with open(path, 'wb') as f:
        f.write(out)


# ------------------------------------------------------------------------------------------------ which hulls

prov = json.load(open(os.path.join(models_dir, 'ImportProvenance.json'), encoding='utf-8'))
hulls = {}
for model in prov['models']:
    src = os.path.join(SAVED, model['source'].split('unreal/Saved/', 1)[1]) if 'unreal/Saved/' in model['source'] else model['source']
    for part in model['parts']:
        if part.get('same_as'):
            continue
        hulls.setdefault(part['asset'], dict(asset=part['asset'], key=model['key'], part=part['part'], source=src, source_sha256=model['source_sha256'],
                                             normalized_sha256=part['normalized_glb_sha256']))
candidates = []
for asset in sorted(hulls):
    h = hulls[asset]
    if not os.path.exists(h['source']):
        continue
    g, _ = read_glb(h['source'])
    meshes = [m for m in g['meshes'] if any('_WHEELPIVOT' in p['attributes'] for p in m['primitives'])]
    if not meshes or h['part'] != 'hull':
        continue
    candidates.append(h)
log['candidates'] = len(candidates)

# the shared path table: track-motion.js register(), in a fixed order
rows, loops = {}, []
prepared = []
for h in candidates:
    name = h['asset'].split('/')[-1]
    folder = name[len('SM_'):-len('_hull')]
    normalized = os.path.join(SAVED, 'ModelImport', folder, name + '.glb')
    rec = dict(asset=h['asset'], key=h['key'], source=h['source'], normalized=normalized)
    try:
        if sha256(h['source']) != h['source_sha256']:
            raise RuntimeError('the reference glb changed since the import (sha256)')
        if not os.path.exists(normalized) or sha256(normalized) != h['normalized_sha256']:
            raise RuntimeError('the normalized glb is missing or changed since the import (sha256)')
        S, sbin = read_glb(h['source'])
        N, nbin = read_glb(normalized)
        mesh = [m for m in S['meshes'] if any('_WHEELPIVOT' in p['attributes'] for p in m['primitives'])]
        if len(mesh) != 1 or len(mesh[0]['primitives']) != 1 or len(N['meshes']) != 1 or len(N['meshes'][0]['primitives']) != 1:
            raise RuntimeError('expected one hull primitive in each glb')
        sa = mesh[0]['primitives'][0]['attributes']
        SP, NP = accessor(S, sbin, sa['POSITION']), accessor(N, nbin, N['meshes'][0]['primitives'][0]['attributes']['POSITION'])
        if len(SP) != len(NP):
            raise RuntimeError('%d reference vertices, %d normalized' % (len(SP), len(NP)))
        worst = max(max(abs(a - b) for a, b in zip(p, q)) for p, q in zip(SP, NP))
        if worst > 1e-6:
            raise RuntimeError('vertex positions differ by up to %g m' % worst)
        W = accessor(S, sbin, sa['_WHEELPIVOT'])
        side = accessor(S, sbin, sa['_WHEELSIDE']) if '_WHEELSIDE' in sa else None
        T = accessor(S, sbin, sa['_TRACKDATA']) if '_TRACKDATA' in sa else None
        own = (mesh[0].get('extras') or {}).get('ww2', {}).get('trackLoops') or []
        ids = []
        for loop in own:
            key = json.dumps(loop['points'], separators=(',', ':'))
            if key not in rows:
                if len(loops) >= 128:
                    raise RuntimeError('Native track path table is full')
                rows[key] = len(loops)
                loops.append(dict(row=len(loops), points=loop['points'], first=h['asset']))
            ids.append(rows[key])
        # wheel-motion.js wheelMesh: wheelSide is the track side nearest the pivot's z (else z); the exporter stores it
        centers = sorted({t[2] for t in T if t[1] >= 0}) if T else []
        recomputed_side_err = 0.0
        uv4, uv5, uv6, uv7 = [], [], [], []
        wheels = links = 0
        for i in range(len(SP)):
            px, py, pz, pr = W[i]
            sd = side[i][0] if side else pz
            want = min(centers, key=lambda c: abs(c - pz)) if centers else pz
            if pr > 0:  # static vertices carry a side too (a tie between the two sides), never used
                recomputed_side_err = max(recomputed_side_err, abs(want - sd))
            uv4.append((px, py))
            uv5.append((sd, pr))
            wheels += 1 if pr > 0 else 0
            if T and T[i][1] >= 0:
                uv6.append((T[i][0], float(ids[int(round(T[i][1]))])))
                uv7.append((T[i][2], T[i][3]))
                links += 1
            else:
                uv6.append((0.0, -1.0))
                uv7.append((0.0, 0.0))
        out = os.path.join(WORK, name + '_Tracks.glb')
        write_glb(out, N, nbin, [('TEXCOORD_4', uv4), ('TEXCOORD_5', uv5), ('TEXCOORD_6', uv6), ('TEXCOORD_7', uv7)])
        rec.update(vertices=len(SP), wheel_vertices=wheels, link_vertices=links, loops=ids, wheel_side_vs_recomputed=recomputed_side_err,
                   glb=out, glb_sha256=sha256(out), reference_sha256=h['source_sha256'], normalized_sha256=h['normalized_sha256'])
        prepared.append(rec)
    except Exception as e:
        rec['error'] = str(e)
        log['errors'].append('%s: %s' % (h['asset'], e))
    log['meshes'].append(rec)


# ------------------------------------------------------------------------------------------------ the Unreal side

def import_tracks(rec):
    """The importer's import_part settings (unreal/Tools/models/import-models.py), into /Game/Models/Motion/Tracks."""
    name = rec['asset'].split('/')[-1] + '_Tracks'
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
        except Exception:
            pass
    matp = pipe.get_editor_property('material_pipeline')
    matp.set_editor_property('import_materials', False)
    try:
        matp.get_editor_property('texture_pipeline').set_editor_property('import_textures', False)
    except Exception:
        pass
    pipe.get_editor_property('animation_pipeline').set_editor_property('import_animations', False)
    params = unreal.ImportAssetParameters()
    params.set_editor_property('is_automated', True)
    params.set_editor_property('replace_existing', True)
    params.set_editor_property('destination_name', name)
    params.set_editor_property('override_pipelines', [unreal.SoftObjectPath(pipe.get_path_name())])
    source = unreal.InterchangeManager.create_source_data(rec['glb'])
    unreal.InterchangeManager.get_interchange_manager_scripted().import_asset(TRACKS, source, params)
    sm = lib.load_asset('%s/%s' % (TRACKS, name))
    if not isinstance(sm, unreal.StaticMesh):
        raise RuntimeError('import of %s did not produce %s/%s' % (rec['glb'], TRACKS, name))
    return sm


def custom(material, x, y, code, inputs, output, description):
    node = mel.create_material_expression(material, unreal.MaterialExpressionCustom, x, y)
    node.set_editor_property('code', code)
    node.set_editor_property('output_type', output)
    node.set_editor_property('description', description)
    pins = []
    for n in inputs:
        pin = unreal.CustomInput()
        pin.set_editor_property('input_name', n)
        pins.append(pin)
    node.set_editor_property('inputs', pins)
    return node


def link(a, out, b, inp):
    if not mel.connect_material_expressions(a, out, b, inp):
        log['errors'].append('link %s.%s -> %s.%s' % (a.get_name(), out, b.get_name(), inp))


def expr(material, cls, x, y, **props):
    node = mel.create_material_expression(material, cls, x, y)
    for k, v in props.items():
        node.set_editor_property(k, v)
    return node


def make_master():
    path = ROOT + '/M_TCModelTracks'
    if lib.does_asset_exist(path):
        lib.delete_asset(path)
    m = lib.duplicate_asset(MASTER, path)
    if not m:
        raise RuntimeError('could not copy ' + MASTER)
    nodes = {}
    for e in mel.get_material_expressions(m):
        if isinstance(e, unreal.MaterialExpressionCustom):
            nodes[e.get_editor_property('description')] = e
        if isinstance(e, unreal.MaterialExpressionLocalPosition):
            # the texture layers' model space stays the authored one (wheel-motion.js keeps vModelPos/vModelNormal)
            e.set_editor_property('included_offsets', unreal.PositionIncludedOffsets.EXCLUDE_OFFSETS)
    model, look = nodes.get('TCModel'), nodes.get('TCLookIndirect')
    if not model or not look:
        raise RuntimeError('M_TCModel has no TCModel or TCLookIndirect node (run make-look-assets.py)')
    S, D = unreal.MaterialVectorCoordTransformSource, unreal.MaterialVectorCoordTransform
    X, Y = -2600, 1400
    local = expr(m, unreal.MaterialExpressionLocalPosition, X, Y, included_offsets=unreal.PositionIncludedOffsets.EXCLUDE_OFFSETS)
    nws = expr(m, unreal.MaterialExpressionVertexNormalWS, X, Y + 80)
    nlocal = expr(m, unreal.MaterialExpressionTransform, X + 200, Y + 80)
    nlocal.set_editor_property('transform_source_type', S.TRANSFORMSOURCE_WORLD)
    nlocal.set_editor_property('transform_type', D.TRANSFORM_LOCAL)
    link(nws, '', nlocal, '')
    uv = [expr(m, unreal.MaterialExpressionTextureCoordinate, X, Y + 160 + 70 * i, coordinate_index=4 + i) for i in range(4)]
    travel = expr(m, unreal.MaterialExpressionPerInstanceCustomData, X, Y + 460, data_index=8, const_default_value=0.0)
    turn = expr(m, unreal.MaterialExpressionPerInstanceCustomData, X, Y + 530, data_index=9, const_default_value=0.0)
    both = expr(m, unreal.MaterialExpressionAppendVector, X + 200, Y + 490)
    link(travel, '', both, 'A'); link(turn, '', both, 'B')
    black = unreal.load_asset('/Engine/EngineResources/Black')
    paths = expr(m, unreal.MaterialExpressionTextureObjectParameter, X, Y + 600, parameter_name='TrackPaths', texture=black)
    outs = []
    for mode in (0, 1):
        node = custom(m, X + 600, Y + 300 * mode, WHEEL_TRACK, ['P', 'Pivot', 'Wheel', 'Track0', 'Track1', 'Travel', 'Paths', 'Mode'],
                      unreal.CustomMaterialOutputType.CMOT_FLOAT3, 'Wheels and tracks offset' if mode == 0 else 'Wheels and tracks normal')
        link(local if mode == 0 else nlocal, '', node, 'P')
        for i, n in enumerate(['Pivot', 'Wheel', 'Track0', 'Track1']):
            link(uv[i], '', node, n)
        link(both, '', node, 'Travel')
        link(paths, '', node, 'Paths')
        link(expr(m, unreal.MaterialExpressionConstant, X + 400, Y + 700 + 60 * mode, r=float(mode)), '', node, 'Mode')
        outs.append(node)
    wpo = expr(m, unreal.MaterialExpressionTransform, X + 900, Y)
    wpo.set_editor_property('transform_source_type', S.TRANSFORMSOURCE_LOCAL)
    wpo.set_editor_property('transform_type', D.TRANSFORM_WORLD)
    link(outs[0], '', wpo, '')
    if not mel.connect_material_property(wpo, '', unreal.MaterialProperty.MP_WORLD_POSITION_OFFSET):
        log['errors'].append('world position offset')
    nworld = expr(m, unreal.MaterialExpressionTransform, X + 900, Y + 300)
    nworld.set_editor_property('transform_source_type', S.TRANSFORMSOURCE_LOCAL)
    nworld.set_editor_property('transform_type', D.TRANSFORM_WORLD)
    link(outs[1], '', nworld, '')
    nunit = expr(m, unreal.MaterialExpressionNormalize, X + 1100, Y + 300)
    link(nworld, '', nunit, '')
    link(nunit, '', model, 'VertexNormal')
    link(nunit, '', look, 'VertexNormal')
    mel.recompile_material(m)
    lib.save_loaded_asset(m, only_if_is_dirty=False)
    log['assets'].append(m.get_path_name())
    return m


def make_instance(master):
    path = ROOT + '/MI_TCModelTracks_VEHICLE_PAINT'
    if lib.does_asset_exist(path):
        lib.delete_asset(path)
    mi = lib.duplicate_asset(PAINT, path)
    mel.set_material_instance_parent(mi, master)
    lib.save_loaded_asset(mi, only_if_is_dirty=False)
    log['assets'].append(mi.get_path_name())
    return mi


master = make_master()
mi = make_instance(master)
for rec in prepared:
    try:
        sm = import_tracks(rec)
        for i in range(len(sm.get_editor_property('static_materials'))):
            sm.set_material(i, mi)
        lib.save_loaded_asset(sm, only_if_is_dirty=False)
        # the channel count and the values Unreal holds are checked by the client's own test (ThreeCrossroads.Motion.
        # WheelTracks reads the mesh description: 8 channels, every wheel and link vertex): the scripting API can read
        # a channel that is not there only by crashing
        rec.update(mesh=sm.get_path_name(), triangles=sm.get_num_triangles(0), materials=[mi.get_path_name()])
        log['assets'].append(sm.get_path_name())
    except Exception as e:
        rec['error'] = str(e)
        log['errors'].append('%s: %s' % (rec['asset'], e))

catalog = {}
for rec in prepared:
    if 'mesh' in rec and 'error' not in rec:
        catalog[rec['asset'] + '.' + rec['asset'].split('/')[-1]] = dict(mesh=rec['mesh'], materials=rec['materials'], loops=rec['loops'],
                                                                         wheel_vertices=rec['wheel_vertices'], link_vertices=rec['link_vertices'])
out = dict(format='tc-wheel-tracks-v1', generator='unreal/Tools/motion/make-wheel-tracks.py', reference_commit=prov.get('reference_commit'),
           custom_data=dict(travel=8, turn=9, floats=10), samples=1024, rows=128, texture_parameter='TrackPaths',
           loops=[dict(row=l['row'], points=l['points']) for l in loops], meshes=catalog)
with open(os.path.join(models_dir, 'Motion', 'WheelTracks.json'), 'w', encoding='utf-8') as f:
    json.dump(out, f, indent=1, sort_keys=True)
log['loops'] = len(loops)
log['catalog'] = len(catalog)
if LOG_PATH:
    json.dump(log, open(LOG_PATH, 'w'), indent=1)
print('make-wheel-tracks: %d hulls, %d loops, %d errors' % (len(catalog), len(loops), len(log['errors'])))
for e in log['errors']:
    print('make-wheel-tracks error: ' + e)
