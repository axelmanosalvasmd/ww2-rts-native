"""The near soldier crowd in the game's model look: the materials UTCSoldierCrowd (Client/TCUnitMotion.cpp) draws near
men and their kit with, in /Game/Models/Motion (unreal/Content/Models/Motion). Run inside the editor on a host project
whose Content/Models is the real project's, AFTER unreal/Tools/models/import-models.py, unreal/Tools/look/
make-look-assets.py (which patches the model masters) and unreal/Tools/models/infantry/import_infantry.py:
  UnrealEditor-Cmd <host>.uproject -run=pythonscript -script=make-crowd-look.py -unattended -nop4 -nosplash -nullrhi
  with MOTION_LOG=<result json path>

Why (round 14, CLI-122 to 124 scope note (b)): the reference draws every soldier, body and kit, with ONE material,
unit-models.js INFANTRY_PAINT = infantryMaterial(modelMaterial(MeshStandardMaterial({ vertexColors, roughness 0.9 }),
{ mat: 'wool', grime: true })): the model textures (client/model-textures.js: the wool layer, the per-vertex matId of
the kit, the cool rim light uModelRim) under the game's light (client/light.js: hemisphere plus the PMREM environment).
The Unreal model masters (M_TCModel*) carry exactly that (the TCModel node from import-models.py and the TCLookIndirect
node from make-look-assets.py, which replaces the engine sky light: ambient occlusion 0). The near crowd's materials
from import_infantry.py (M_InfantryCrowd, M_InfantryCrowdKit) and the faded copies made by make-materials.py had only
the atlas and a roughness, lit by the engine's sky light: measured 1.86 Delta E too warm and light on the 16 close-up
soldiers, against 0.43 for the same men drawn with the catalog's M_TCModel figure.

So these materials take the two HLSL nodes and every parameter they read from the patched M_TCModel itself (read here,
not copied by hand) and feed them the crowd's own vertex data, as infantry-authored.js builds it:
  body   color = 1 + (tunic / sourceGrey - 1) * cloth  (the faction tunic, MI_InfantryCrowd_<tag> Tunic; cloth = vertex
         color R), matId = cloth > 0.5 ? wool : plain, modelUV = (uv, 1 + cloth); no owner or vehicle tint
  kit    color = the vertex color (linear), matId = the kit piece's material (KitMat, the MATS index of its slot)
plus the grime unit-models.js mergeParts bakes into each soldier vertex's matId (LOOKS.soldier: mud below 0.3 m,
dust 0.2 and more on faces turned up, at most 0.6; found as the rest of the gap: 1.59 Delta E with the look alone),
computed on the standing build (the crowd's keyframe 0) in the vertex shader, and the crowd's GPU skinning (make-materials.py SKIN: position as world position offset, the skinned normal as the
lighting normal and the model-space normal of the triplanar layers). The custom data the master reads for the owner,
vehicle paint and damage (0-7) are the crowd's keyframes, so those inputs are parameters here (no tint: Tint is 0).
Created (replacing what exists):
  M_InfantryCrowdLook, M_InfantryCrowdLookFaded          body (Faded: the dither on custom data 13)
  M_InfantryCrowdKitLook, M_InfantryCrowdKitLookFaded    kit (rigid on custom data 12's bone)
  MI_InfantryCrowdLook_<DE|SU|UK|US>                     MI_TCModel_INFANTRY_PAINT's values plus the faction Tunic
  MI_InfantryCrowdKitLook_<material>                     MI_TCModel_INFANTRY_PAINT's values plus KitMat
The faded masters are made from the same code; the crowd copies an instance's overrides onto them at run time.
"""
import json
import os
import re
import unreal

ROOT = '/Game/Models/Motion'
MASTER = '/Game/Models/Materials/M_TCModel'
PAINT_MI = '/Game/Models/Materials/Instances/MI_TCModel_INFANTRY_PAINT'
LOG_PATH = os.environ.get('MOTION_LOG', '')
log = {'assets': [], 'errors': [], 'copied': {}}
tools = unreal.AssetToolsHelpers.get_asset_tools()
lib = unreal.EditorAssetLibrary
mel = unreal.MaterialEditingLibrary
HERE = os.path.dirname(os.path.abspath(__file__))
BONE, FADE = 12, 13  # TCUnitMotion.h TCCrowdData

exec(open(os.path.join(HERE, 'crowd-shared.py')).read())  # SKIN, DITHER, expr, custom, link, prop (shared with make-materials.py)

# the model material layers in the masters' order (import-models.py MATS, the reference's geom.js MATS)
src = open(os.path.join(HERE, '..', 'models', 'import-models.py'), encoding='utf-8').read()
MATS = eval(re.search(r'^MATS = (\[.*?\])', src, re.S | re.M).group(1))
log['mats'] = MATS

BODY_INPUTS = '''// infantry-authored.js authoredInfantry: color = 1 + (tint - 1) * cloth, matId = cloth > .5 ? wool : plain,
// modelUV.z = 1 + cloth; unit-models.js mergeParts adds the baked grime as the id's fraction (id + grime / 2). The model node reads saturate(VertexColor) * Scale.x (import-models.py: colors above 1 are
// carried as a scale), so a tunic tint above 1 goes through Scale.
float3 c = lerp(float3(1.0, 1.0, 1.0), Tunic, Cloth);
float k = max(1.0, max(c.r, max(c.g, c.b)));
Mat = float2((Cloth > 0.5 ? Wool : -1.0) + Grime * 0.5, 1.0 + Cloth);
Scale = float2(k, 0.0);
return c / k;'''
KIT_INPUTS = '''// a kit piece (helmet, weapon, pack): its vertex color as built, its material's layer (geom.js matId) plus the
// baked grime as the id's fraction (unit-models.js mergeParts), no atlas
Mat = float2(KitMat + Grime * 0.5, 0.0);
Scale = float2(1.0, 0.0);
return C;'''


def fresh(path):
    if lib.does_asset_exist(path):
        lib.delete_asset(path)


def save(asset):
    lib.save_loaded_asset(asset, only_if_is_dirty=False)
    log['assets'].append(asset.get_path_name())


master = unreal.load_asset(MASTER)
paint = unreal.load_asset(PAINT_MI)
if not master or not paint:
    raise SystemExit('make-crowd-look: %s or %s is missing (run import-models.py and make-look-assets.py first)' % (MASTER, PAINT_MI))
nodes = {}
for e in mel.get_material_expressions(master):
    if isinstance(e, unreal.MaterialExpressionCustom):
        nodes[e.get_editor_property('description')] = e
if 'TCModel' not in nodes or 'TCLookIndirect' not in nodes:
    raise SystemExit('make-crowd-look: %s has no TCModel or TCLookIndirect node (patch it with make-look-assets.py)' % MASTER)
log['copied'] = {k: len(nodes[k].get_editor_property('code')) for k in ('TCModel', 'TCLookIndirect')}


def master_inputs(node):
    """(input name, source expression, its output name) for each input of a master node, in order."""
    names = [i.get_editor_property('input_name') for i in node.get_editor_property('inputs')]
    sources = mel.get_inputs_for_material_expression(master, node)
    out = []
    for n, s in zip(names, sources):
        out.append((str(n), s, mel.get_input_node_output_name_for_material_expression(node, s) if s else ''))
    return out


def clone_param(material, src_expr, x, y):
    """The same parameter or constant in another material (the master's names, defaults and textures)."""
    # Duplicate the complete expression, including protected collection parameter IDs and texture metadata.
    # Copying only editable properties can lose the binding the compiled material reads.
    node = mel.duplicate_material_expression(material, None, src_expr)
    if node is None:
        raise RuntimeError('cannot copy model parameter ' + src_expr.get_name())
    return node


def copy_custom(material, src_node, x, y):
    node = mel.create_material_expression(material, unreal.MaterialExpressionCustom, x, y)
    for key in ('code', 'output_type', 'description', 'inputs', 'additional_outputs', 'additional_defines', 'include_file_paths'):
        try:
            node.set_editor_property(key, src_node.get_editor_property(key))
        except Exception:
            pass
    return node


def model_look(material, kit, rigid, far=False):
    """The master's TCModel and TCLookIndirect nodes on the crowd's data, skinned. Returns nothing; wires every output."""
    pos, n_local, n_world, grime = skin_nodes(material, rigid)
    vcol = expr(material, unreal.MaterialExpressionVertexColor, -2200, -900)
    if far:
        # FarFigures v2 carries the reference's linear base/tint in UV0/5/6 and its matId in UV1.y.
        # UV1.y is an unused second joint with zero weight; no extra UV channel or skinning input is needed.
        code = """float3 c = float3(A.x, A.y, B.x) + float3(B.y, C.x, C.y) * Owner;
float k = max(1.0, max(c.r, max(c.g, c.b)));
Mat = float2(Tag.y, 0.0);
Scale = float2(k, 0.0);
return c / k;"""
        inputs = custom(material, -1900, -900, code, ['A','B','C','Owner','Tag'], description='Far figure as the model', outputs=[('Mat','f2'),('Scale','f2')])
        for pin, channel in [('A',0),('B',5),('C',6),('Tag',1)]:
            uv = expr(material, unreal.MaterialExpressionTextureCoordinate, -2200, -900 + channel * 40, coordinate_index=channel)
            link(uv, '', inputs, pin)
        owner = expr(material, unreal.MaterialExpressionPerInstanceCustomData3Vector, -2200, -560, data_index=14, const_default_value=unreal.LinearColor(1,1,1,1))
        link(owner, '', inputs, 'Owner')
    elif kit:
        inputs = custom(material, -1900, -900, KIT_INPUTS, ['C', 'KitMat', 'Grime'], description='Crowd kit as the model', outputs=[('Mat', 'f2'), ('Scale', 'f2')])
        mask = expr(material, unreal.MaterialExpressionComponentMask, -2050, -900, r=True, g=True, b=True, a=False)
        link(vcol, '', mask, ''); link(mask, '', inputs, 'C')
        kitmat = expr(material, unreal.MaterialExpressionScalarParameter, -2200, -800, parameter_name='KitMat', default_value=-1.0)
        link(kitmat, '', inputs, 'KitMat')
    else:
        inputs = custom(material, -1900, -900, BODY_INPUTS, ['Cloth', 'Tunic', 'Wool', 'Grime'], description='Crowd body as the model', outputs=[('Mat', 'f2'), ('Scale', 'f2')])
        tunic = expr(material, unreal.MaterialExpressionVectorParameter, -2200, -800, parameter_name='Tunic', default_value=unreal.LinearColor(1, 1, 1, 1))
        wool = expr(material, unreal.MaterialExpressionConstant, -2200, -720, r=float(MATS.index('wool')))
        link(vcol, 'R', inputs, 'Cloth'); link(tunic, '', inputs, 'Tunic'); link(wool, '', inputs, 'Wool')
    if not far:
        link(grime, '', inputs, 'Grime')
    uv0 = expr(material, unreal.MaterialExpressionTextureCoordinate, -2200, -640, coordinate_index=0)
    no_tint = expr(material, unreal.MaterialExpressionConstant2Vector, -2200, -580, r=0.0, g=0.0)
    world = expr(material, unreal.MaterialExpressionWorldPosition, -2200, -520)
    cam = expr(material, unreal.MaterialExpressionCameraVectorWS, -2200, -460)
    damage = expr(material, unreal.MaterialExpressionScalarParameter, -2200, -400, parameter_name='Damage', default_value=0.0)
    model = copy_custom(material, nodes['TCModel'], -1200, -600)
    special = {'VertexColor': (inputs, ''), 'AtlasUV': (no_tint if far else uv0, ''), 'Mat': (inputs, 'Mat'), 'Tint': (no_tint, ''), 'Scale': (inputs, 'Scale'),
               'LocalPosition': (pos, ''), 'LocalNormal': (n_local, ''), 'VertexNormal': (n_world, ''), 'WorldPos': (world, ''),
               'CameraVector': (cam, ''), 'Damage': (damage, '')}
    y = 0
    for name, src_expr, out in master_inputs(nodes['TCModel']):
        if name in special:
            link(special[name][0], special[name][1], model, name)
            continue
        if src_expr is None:
            continue
        if isinstance(src_expr, (unreal.MaterialExpressionPerInstanceCustomData, unreal.MaterialExpressionPerInstanceCustomData3Vector)):
            # Owner, Vehicle: the master reads custom data 0-5, the crowd's keyframes here: the parameter behind it
            behind = mel.get_inputs_for_material_expression(master, src_expr)
            src_expr = behind[0] if behind and behind[0] else None
            out = 'RGB' if isinstance(src_expr, unreal.MaterialExpressionVectorParameter) else ''
            if src_expr is None:
                log['errors'].append('TCModel input %s: custom data without a default parameter' % name)
                continue
        p = clone_param(material, src_expr, -1600, -300 + 60 * y); y += 1
        link(p, out, model, name)
    look = copy_custom(material, nodes['TCLookIndirect'], -700, -300)
    cam2 = expr(material, unreal.MaterialExpressionCameraVectorWS, -1000, 400)
    for name, src_expr, out in master_inputs(nodes['TCLookIndirect']):
        if src_expr == nodes['TCModel']:
            link(model, '' if name == 'Color' else name, look, name)
        elif name == 'VertexNormal':
            link(n_world, '', look, name)
        elif name == 'CameraVector':
            link(cam2, '', look, name)
        elif src_expr is not None:
            p = clone_param(material, src_expr, -1000, 460 + 60 * y); y += 1
            link(p, out, look, name)
    P = unreal.MaterialProperty
    prop(model, '', P.MP_BASE_COLOR)
    for o, mp in (('Roughness', P.MP_ROUGHNESS), ('Metallic', P.MP_METALLIC), ('Specular', P.MP_SPECULAR), ('NormalWS', P.MP_NORMAL)):
        prop(model, o, mp)
    prop(look, '', P.MP_EMISSIVE_COLOR)
    zero = expr(material, unreal.MaterialExpressionConstant, -400, 600, r=0.0)
    prop(zero, '', P.MP_AMBIENT_OCCLUSION)
    material.set_editor_property('tangent_space_normal', False)


def make(name, kit, faded, far=False):
    path = f'{ROOT}/{name}'
    fresh(path)
    m = tools.create_asset(name, ROOT, unreal.Material, unreal.MaterialFactoryNew())
    m.set_editor_property('used_with_instanced_static_meshes', True)
    model_look(m, kit, kit, far)
    if faded:
        dithered(m)
    mel.layout_material_expressions(m)
    mel.recompile_material(m)
    save(m)
    return m


def instance(parent, name, scalars, vectors):
    path = f'{ROOT}/{name}'
    fresh(path)
    mi = tools.create_asset(name, ROOT, unreal.MaterialInstanceConstant, unreal.MaterialInstanceConstantFactoryNew())
    mel.set_material_instance_parent(mi, parent)
    # MI_TCModel_INFANTRY_PAINT's values (the reference's INFANTRY_PAINT: roughness 0.9, wool, grime, the atlas)
    for p in paint.get_editor_property('scalar_parameter_values'):
        mel.set_material_instance_scalar_parameter_value(mi, p.parameter_info.name, p.parameter_value)
    for p in paint.get_editor_property('vector_parameter_values'):
        mel.set_material_instance_vector_parameter_value(mi, p.parameter_info.name, p.parameter_value)
    for p in paint.get_editor_property('texture_parameter_values'):
        if p.parameter_value:
            mel.set_material_instance_texture_parameter_value(mi, p.parameter_info.name, p.parameter_value)
    for k, v in scalars.items():
        mel.set_material_instance_scalar_parameter_value(mi, k, v)
    for k, v in vectors.items():
        mel.set_material_instance_vector_parameter_value(mi, k, v)
    save(mi)
    return mi


if os.environ.get('MOTION_FAR_ONLY') != '1':
    body = make('M_InfantryCrowdLook', False, False)
    make('M_InfantryCrowdLookFaded', False, True)
    kit = make('M_InfantryCrowdKitLook', True, False)
    make('M_InfantryCrowdKitLookFaded', True, True)
    log['tunics'] = {}
    for tag in ('DE', 'SU', 'UK', 'US'):
        old = unreal.load_asset(f'/Game/Models/Infantry/Materials/MI_InfantryCrowd_{tag}')
        if not old:
            log['errors'].append(f'no MI_InfantryCrowd_{tag} (import_infantry.py) for the {tag} tunic')
            continue
        tunic = mel.get_material_instance_vector_parameter_value(old, 'Tunic')
        log['tunics'][tag] = [tunic.r, tunic.g, tunic.b]
        instance(body, f'MI_InfantryCrowdLook_{tag}', {}, {'Tunic': tunic})
    log['kit'] = {}
    for path in sorted(lib.list_assets('/Game/Models/Infantry/Materials', recursive=False)):
        name = path.split('/')[-1].split('.')[0]
        if not name.startswith('MI_InfantryCrowdKit_'):
            continue
        mat = name[len('MI_InfantryCrowdKit_'):].replace('_', '-')
        index = -1.0 if mat == 'plain' else float(MATS.index(mat)) if mat in MATS else None
        if index is None:
            log['errors'].append(f'{name}: {mat} is not a model material layer')
            continue
        log['kit'][mat] = index
        instance(kit, 'MI_InfantryCrowdKitLook_' + mat.replace('-', '_'), {'KitMat': index}, {})

# The far figure uses the same INFANTRY_PAINT parameters and the master's exact layer/lighting nodes as the near men.
far = make('M_InfantryFarLook', False, False, True)
instance(far, 'MI_InfantryFarLook', {}, {})
far_faded = make('M_InfantryFarLookFaded', False, True, True)
instance(far_faded, 'MI_InfantryFarLookFaded', {}, {})

if LOG_PATH:
    json.dump(log, open(LOG_PATH, 'w'), indent=1)
print('make-crowd-look: %d assets, %d errors' % (len(log['assets']), len(log['errors'])))
for e in log['errors']:
    print('make-crowd-look error: ' + e)
