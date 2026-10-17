"""Builds /Game/Models/Infantry from the exporter's output (export-infantry.mjs). Run inside the editor:

  UnrealEditor-Cmd <project>.uproject -run=pythonscript -script=import_infantry.py -unattended -nop4 -nosplash
  with INFANTRY_EXPORT=<export dir> INFANTRY_REFERENCE=<reference/ww2-rts> INFANTRY_LOG=<result json path>

Creates (replacing what exists):
  Textures/T_InfantryV3_Color, T_InfantryV3_Normal  the reference atlas (client/textures/models), normal green flipped
                                                    as Unreal's own glTF importer does for glTF normal maps
  SK_Infantry, SKEL_Infantry                        the skinned body, 31 bones, a socket on every kit anchor
  SM_InfantryCrowdBody, SM_InfantryCrowdCrown       the body (and the German crown) as static meshes with joints
                                                    and weights in UV1-4 for the crowd path
  Kit/SM_InfantryKit_*                              the attachment meshes, one per kits.json mesh
  Materials/M_InfantryBody + MI_InfantryBody_{US,DE,SU,UK}      atlas, normal map, faction tunic tint on the cloth mask
  Materials/M_InfantryKit + MI_InfantryKit_<material>           vertex color, reference roughness and metalness
  Materials/M_InfantryCrowd(+Kit) + instances                   the same looks, skinned on the GPU from a keyframe
                                                                texture and per-instance custom data
Results (asset paths, bone list, ref pose, socket list) go to INFANTRY_LOG for the verification step.
"""
import json
import os

import unreal

EXPORT = os.environ['INFANTRY_EXPORT']
REFERENCE = os.environ['INFANTRY_REFERENCE']
LOG_PATH = os.environ['INFANTRY_LOG']
ROOT = '/Game/Models/Infantry'
MANIFEST = json.load(open(os.path.join(EXPORT, 'kits.json')))
log = {'assets': [], 'errors': []}
tools = unreal.AssetToolsHelpers.get_asset_tools()
lib = unreal.EditorAssetLibrary
mel = unreal.MaterialEditingLibrary

# client/models/infantry.js FACTIONS tunic colors and the authored source grey (infantry-authored.js), sRGB hex
FACTION_TUNIC = {'US': 0x5f6046, 'DE': 0x5d6668, 'SU': 0x7f7a5c, 'UK': 0x6e5f42}
SOURCE_GREY = 0x5d6668
# client/model-textures.js LAYERS: roughness, metalness per material tag; 'plain' (skin, faces) keeps the body's 0.9
LAYERS = {'armor-paint': (0.68, 0), 'cast-armor': (0.8, 0), 'gunmetal': (0.42, 0.85), 'track-steel': (0.73, 0.7), 'rubber': (0.95, 0),
          'wood': (0.84, 0), 'canvas': (0.94, 0), 'wool': (1.0, 0), 'leather': (0.8, 0), 'aluminum': (0.38, 1), 'aircraft-paint': (0.6, 0),
          'mud': (1.0, 0), 'asphalt': (0.96, 0), 'concrete': (0.92, 0), 'plain': (0.9, 0)}


def linear(hex_color):
    def channel(c):
        c = c / 255.0
        return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4
    return [channel((hex_color >> 16) & 255), channel((hex_color >> 8) & 255), channel(hex_color & 255)]


def fresh(path):
    if lib.does_asset_exist(path):
        lib.delete_asset(path)


def save(asset):
    lib.save_loaded_asset(asset, only_if_is_dirty=False)
    log['assets'].append(asset.get_path_name())


# ---------------------------------------------------------------------------------------------- textures

def import_texture(file_path, name, normal):
    fresh(f'{ROOT}/Textures/{name}')
    task = unreal.AssetImportTask()
    task.set_editor_property('filename', file_path)
    task.set_editor_property('destination_path', f'{ROOT}/Textures')
    task.set_editor_property('destination_name', name)
    task.set_editor_property('automated', True)
    task.set_editor_property('replace_existing', True)
    task.set_editor_property('save', False)
    tools.import_asset_tasks([task])
    texture = unreal.load_asset(f'{ROOT}/Textures/{name}')
    if normal:
        texture.set_editor_property('compression_settings', unreal.TextureCompressionSettings.TC_NORMALMAP)
        texture.set_editor_property('srgb', False)
        texture.set_editor_property('flip_green_channel', True)
    else:
        texture.set_editor_property('srgb', True)
    save(texture)
    return texture


textures = os.path.join(REFERENCE, 'client/textures/models')
color_tex = import_texture(os.path.join(textures, 'infantry-v3-color.jpg'), 'T_InfantryV3_Color', False)
normal_tex = import_texture(os.path.join(textures, 'infantry-v3-normal.jpg'), 'T_InfantryV3_Normal', True)

# ---------------------------------------------------------------------------------------------- meshes

def interchange_import(file_path, dest_path, name, skeletal, full_precision_uvs=False):
    pipeline = unreal.InterchangeGenericAssetsPipeline()
    pipeline.set_editor_property('asset_name', name)
    common = pipeline.get_editor_property('common_meshes_properties')
    common.set_editor_property('force_all_mesh_as_type', unreal.InterchangeForceMeshType.IFMT_SKELETAL_MESH if skeletal else unreal.InterchangeForceMeshType.IFMT_STATIC_MESH)
    common.set_editor_property('recompute_normals', False)
    common.set_editor_property('keep_sections_separate', True)
    common.set_editor_property('use_full_precision_u_vs', full_precision_uvs)
    common.set_editor_property('vertex_color_import_option', unreal.InterchangeVertexColorImportOption.IVCIO_REPLACE)
    common.set_editor_property('import_sockets', False)
    mesh = pipeline.get_editor_property('mesh_pipeline')
    mesh.set_editor_property('import_skeletal_meshes', skeletal)
    mesh.set_editor_property('import_static_meshes', not skeletal)
    mesh.set_editor_property('create_physics_asset', False)
    mesh.set_editor_property('import_morph_targets', False)
    mesh.set_editor_property('build_nanite', False)
    mesh.set_editor_property('collision', False)
    mesh.set_editor_property('use_high_precision_skin_weights', True)
    mesh.set_editor_property('generate_lightmap_u_vs', False)
    pipeline.get_editor_property('material_pipeline').set_editor_property('import_materials', False)
    pipeline.get_editor_property('animation_pipeline').set_editor_property('import_animations', False)
    params = unreal.ImportAssetParameters()
    params.set_editor_property('is_automated', True)
    params.set_editor_property('replace_existing', True)
    params.set_editor_property('override_pipelines', [unreal.SoftObjectPath(pipeline.get_path_name())])
    source = unreal.InterchangeManager.create_source_data(file_path)
    manager = unreal.InterchangeManager.get_interchange_manager_scripted()
    ok = manager.import_asset(dest_path, source, params)
    if not ok:
        log['errors'].append(f'import failed: {file_path}')
    return ok


fresh(f'{ROOT}/SK_Infantry')
fresh(f'{ROOT}/SKEL_Infantry')
fresh(f'{ROOT}/SK_Infantry_Skeleton')
interchange_import(os.path.join(EXPORT, 'SK_Infantry.glb'), ROOT, 'SK_Infantry', True)
body = unreal.load_asset(f'{ROOT}/SK_Infantry')
skeleton = body.get_editor_property('skeleton')
if skeleton and skeleton.get_name() != 'SKEL_Infantry':
    tools.rename_assets([unreal.AssetRenameData(skeleton, ROOT, 'SKEL_Infantry')])
    skeleton = body.get_editor_property('skeleton')

crowd = {}
for name in ('SM_InfantryCrowdBody', 'SM_InfantryCrowdCrown'):
    fresh(f'{ROOT}/{name}')
    interchange_import(os.path.join(EXPORT, f'{name}.glb'), ROOT, name, False, True)
    crowd[name] = unreal.load_asset(f'{ROOT}/{name}')
crowd_body = crowd['SM_InfantryCrowdBody']

kit_meshes = {}
for entry in MANIFEST['meshes']:
    name = f"SM_InfantryKit_{entry['name']}"
    fresh(f'{ROOT}/Kit/{name}')
    interchange_import(os.path.join(EXPORT, entry['file']), f'{ROOT}/Kit', name, False)
    kit_meshes[entry['name']] = unreal.load_asset(f'{ROOT}/Kit/{name}')

# ---------------------------------------------------------------------------------------------- materials

def new_material(name, usage):
    path = f'{ROOT}/Materials/{name}'
    fresh(path)
    material = tools.create_asset(name, f'{ROOT}/Materials', unreal.Material, unreal.MaterialFactoryNew())
    for flag in usage:
        material.set_editor_property(flag, True)
    return material


def expr(material, cls, x, y, **props):
    node = mel.create_material_expression(material, cls, x, y)
    for key, value in props.items():
        node.set_editor_property(key, value)
    return node


def custom(material, x, y, code, inputs, output=unreal.CustomMaterialOutputType.CMOT_FLOAT3, description=''):
    node = expr(material, unreal.MaterialExpressionCustom, x, y)
    node.set_editor_property('code', code)
    node.set_editor_property('output_type', output)
    node.set_editor_property('description', description)
    pins = []
    for name in inputs:
        pin = unreal.CustomInput()
        pin.set_editor_property('input_name', name)
        pins.append(pin)
    node.set_editor_property('inputs', pins)
    return node


def link(src, src_out, dst, dst_in):
    if not mel.connect_material_expressions(src, src_out, dst, dst_in):
        log['errors'].append(f'link {src.get_name()}.{src_out} -> {dst.get_name()}.{dst_in}')


def prop(src, src_out, material_property):
    if not mel.connect_material_property(src, src_out, material_property):
        log['errors'].append(f'property {src.get_name()}.{src_out} -> {material_property}')


ALBEDO = '''// client/infantry-material.js: the atlas, with the strongest baked highlights taken off the cloth so scene light
// supplies them, tinted toward the faction tunic where the cloth mask is set (infantry-authored.js colors)
float peak = max(Albedo.r, max(Albedo.g, Albedo.b));
float3 a = Albedo / (1.0 + Cloth * max(0.0, peak - 0.20) * 3.0);
return a * lerp(float3(1.0, 1.0, 1.0), Tunic, Cloth);'''
NORMAL = '''// client/infantry-material.js: the atlas normal at 0.65 strength
return normalize(float3(N.xy * 0.65, N.z));'''
DECODE = '''// The glTF importer already undoes the sRGB encoding the mesh builder applies to vertex colors
// (GLTFMeshFactory.cpp SRGBToLinear before StaticMeshBuilder ToFColor(true)), so the bytes the vertex factory reads
// are the reference's linear colors: pass them through.
return C;'''
SKIN = '''// GPU skinning from the keyframe texture (TCInfantryRig.cpp BakeKeyframeTexture: 3 texels per bone, the columns of
// its row-vector 4x3 skin matrix, one row per keyframe) with up to four keyframes blended by per-instance weights,
// then unit-models.js's pose node (lean about the man's right axis, height scale, shift). Mode 0: position (cm),
// Mode 1: normal. Rigid: the bone index from Bone instead of the vertex's joints.
float d[12] = { D0.x, D0.y, D0.z, D1.x, D1.y, D1.z, D2.x, D2.y, D2.z, D3.x, D3.y, D3.z };
float4 J = Rigid > 0.5 ? float4(Bone, 0, 0, 0) : float4(J01, J23);
float4 W = Rigid > 0.5 ? float4(1, 0, 0, 0) : float4(W01, W23);
float4 p4 = Mode > 0.5 ? float4(P, 0.0) : float4(P, 1.0);
float3 acc = float3(0, 0, 0);
float total = 0;
[unroll] for (int k = 0; k < 4; k++)
{
    float kw = d[4 + k];
    if (kw > 0)
    {
        int row = (int)(d[k] + 0.5);
        total += kw;
        [unroll] for (int b = 0; b < 4; b++)
        {
            if (W[b] > 0)
            {
                int x = (int)(J[b] + 0.5) * 3;
                float3 q = float3(dot(Keys.Load(int3(x, row, 0)), p4), dot(Keys.Load(int3(x + 1, row, 0)), p4), dot(Keys.Load(int3(x + 2, row, 0)), p4));
                acc += kw * W[b] * q;
            }
        }
    }
}
if (total <= 0) return P;
float sy = d[9] > 0 ? d[9] : 1.0;
acc.z *= Mode > 0.5 ? 1.0 / sy : sy;
float c = cos(d[8]), s = sin(d[8]);
float3 r = float3(acc.x * c - acc.z * s, acc.y, acc.x * s + acc.z * c);
if (Mode > 0.5) return normalize(r);
r.x += d[10];
r.z += d[11];
return r;'''


def body_look(material, with_textures_x=-900):
    color = expr(material, unreal.MaterialExpressionTextureSampleParameter2D, with_textures_x, -200, parameter_name='Color', texture=color_tex)
    tunic = expr(material, unreal.MaterialExpressionVectorParameter, with_textures_x, 100, parameter_name='Tunic', default_value=unreal.LinearColor(1, 1, 1, 1))
    vcol = expr(material, unreal.MaterialExpressionVertexColor, with_textures_x, 300)
    albedo = custom(material, -400, -100, ALBEDO, ['Albedo', 'Cloth', 'Tunic'], description='Infantry albedo')
    link(color, 'RGB', albedo, 'Albedo'); link(vcol, 'R', albedo, 'Cloth'); link(tunic, '', albedo, 'Tunic')
    prop(albedo, '', unreal.MaterialProperty.MP_BASE_COLOR)
    rough = expr(material, unreal.MaterialExpressionScalarParameter, -400, 200, parameter_name='Roughness', default_value=0.9)
    prop(rough, '', unreal.MaterialProperty.MP_ROUGHNESS)


def kit_look(material):
    vcol = expr(material, unreal.MaterialExpressionVertexColor, -900, -100)
    rgb = expr(material, unreal.MaterialExpressionComponentMask, -700, -100, r=True, g=True, b=True, a=False)
    link(vcol, '', rgb, '')
    decode = custom(material, -500, -100, DECODE, ['C'], description='Linear vertex color')
    link(rgb, '', decode, 'C')
    prop(decode, '', unreal.MaterialProperty.MP_BASE_COLOR)
    rough = expr(material, unreal.MaterialExpressionScalarParameter, -500, 150, parameter_name='Roughness', default_value=0.9)
    metal = expr(material, unreal.MaterialExpressionScalarParameter, -500, 250, parameter_name='Metallic', default_value=0.0)
    prop(rough, '', unreal.MaterialProperty.MP_ROUGHNESS)
    prop(metal, '', unreal.MaterialProperty.MP_METALLIC)


def crowd_skinning(material, rigid):
    keys = expr(material, unreal.MaterialExpressionTextureObjectParameter, -1600, -600, parameter_name='Keyframes')
    local = expr(material, unreal.MaterialExpressionLocalPosition, -1600, -450, included_offsets=unreal.PositionIncludedOffsets.EXCLUDE_OFFSETS)
    normal_ws = expr(material, unreal.MaterialExpressionVertexNormalWS, -1600, -350)
    to_local = expr(material, unreal.MaterialExpressionTransform, -1400, -350)
    to_local.set_editor_property('transform_source_type', unreal.MaterialVectorCoordTransformSource.TRANSFORMSOURCE_WORLD)
    to_local.set_editor_property('transform_type', unreal.MaterialVectorCoordTransform.TRANSFORM_LOCAL)
    link(normal_ws, '', to_local, '')
    data = [expr(material, unreal.MaterialExpressionPerInstanceCustomData3Vector, -1600, -250 + 80 * i, data_index=3 * i) for i in range(4)]
    uvs = [expr(material, unreal.MaterialExpressionTextureCoordinate, -1600, 80 + 70 * i, coordinate_index=i + 1) for i in range(4)]
    bone = expr(material, unreal.MaterialExpressionPerInstanceCustomData, -1600, 380, data_index=12)
    nodes = []
    for mode in (0, 1):
        node = custom(material, -1100, -500 + 400 * mode, SKIN, ['Keys', 'P', 'J01', 'J23', 'W01', 'W23', 'D0', 'D1', 'D2', 'D3', 'Bone', 'Rigid', 'Mode'],
                      description='Infantry skin position' if mode == 0 else 'Infantry skin normal')
        link(keys, '', node, 'Keys')
        link(local if mode == 0 else to_local, '', node, 'P')
        for i, name in enumerate(['J01', 'J23', 'W01', 'W23']):
            link(uvs[i], '', node, name)
        for i in range(4):
            link(data[i], '', node, f'D{i}')
        link(bone, '', node, 'Bone')
        flag = expr(material, unreal.MaterialExpressionConstant, -1300, 450 + 60 * mode, r=1.0 if rigid else 0.0)
        link(flag, '', node, 'Rigid')
        m = expr(material, unreal.MaterialExpressionConstant, -1300, 520 + 60 * mode, r=float(mode))
        link(m, '', node, 'Mode')
        nodes.append(node)
    # world position offset: the skinned local position minus the original, carried to world space
    delta = expr(material, unreal.MaterialExpressionSubtract, -800, -500)
    link(nodes[0], '', delta, 'A'); link(local, '', delta, 'B')
    to_world = expr(material, unreal.MaterialExpressionTransform, -650, -500)
    to_world.set_editor_property('transform_source_type', unreal.MaterialVectorCoordTransformSource.TRANSFORMSOURCE_LOCAL)
    to_world.set_editor_property('transform_type', unreal.MaterialVectorCoordTransform.TRANSFORM_WORLD)
    link(delta, '', to_world, '')
    prop(to_world, '', unreal.MaterialProperty.MP_WORLD_POSITION_OFFSET)
    # the skinned normal, interpolated to the pixel shader and used as a world-space normal
    interp = expr(material, unreal.MaterialExpressionVertexInterpolator, -800, -100)
    link(nodes[1], '', interp, '')
    n_world = expr(material, unreal.MaterialExpressionTransform, -600, -100)
    n_world.set_editor_property('transform_source_type', unreal.MaterialVectorCoordTransformSource.TRANSFORMSOURCE_LOCAL)
    n_world.set_editor_property('transform_type', unreal.MaterialVectorCoordTransform.TRANSFORM_WORLD)
    link(interp, '', n_world, '')
    prop(n_world, '', unreal.MaterialProperty.MP_NORMAL)
    material.set_editor_property('tangent_space_normal', False)


def finish(material):
    mel.layout_material_expressions(material)
    mel.recompile_material(material)
    save(material)


def instance(parent, name, scalars=None, vectors=None):
    path = f'{ROOT}/Materials/{name}'
    fresh(path)
    mi = tools.create_asset(name, f'{ROOT}/Materials', unreal.MaterialInstanceConstant, unreal.MaterialInstanceConstantFactoryNew())
    mel.set_material_instance_parent(mi, parent)
    for key, value in (scalars or {}).items():
        mel.set_material_instance_scalar_parameter_value(mi, key, value)
    for key, value in (vectors or {}).items():
        mel.set_material_instance_vector_parameter_value(mi, key, unreal.LinearColor(value[0], value[1], value[2], 1.0))
    save(mi)
    return mi


m_body = new_material('M_InfantryBody', ['used_with_skeletal_mesh'])
body_look(m_body)
normal = expr(m_body, unreal.MaterialExpressionTextureSampleParameter2D, -900, 500, parameter_name='Normal', texture=normal_tex, sampler_type=unreal.MaterialSamplerType.SAMPLERTYPE_NORMAL)
n = custom(m_body, -400, 450, NORMAL, ['N'], description='Infantry normal')
link(normal, 'RGB', n, 'N')
prop(n, '', unreal.MaterialProperty.MP_NORMAL)
finish(m_body)

m_kit = new_material('M_InfantryKit', [])
kit_look(m_kit)
finish(m_kit)

m_crowd = new_material('M_InfantryCrowd', ['used_with_instanced_static_meshes'])
body_look(m_crowd)
crowd_skinning(m_crowd, False)
finish(m_crowd)

m_crowd_kit = new_material('M_InfantryCrowdKit', ['used_with_instanced_static_meshes'])
kit_look(m_crowd_kit)
crowd_skinning(m_crowd_kit, True)
finish(m_crowd_kit)

grey = linear(SOURCE_GREY)
body_mis, crowd_mis = {}, {}
for tag, hex_color in FACTION_TUNIC.items():
    tint = [c / g for c, g in zip(linear(hex_color), grey)]
    body_mis[tag] = instance(m_body, f'MI_InfantryBody_{tag}', vectors={'Tunic': tint})
    crowd_mis[tag] = instance(m_crowd, f'MI_InfantryCrowd_{tag}', vectors={'Tunic': tint})
kit_mis, crowd_kit_mis = {}, {}
used_mats = sorted({m for entry in MANIFEST['meshes'] for m in entry['materials']})
for mat in used_mats:
    rough, metal = LAYERS.get(mat, (0.9, 0))
    safe = mat.replace('-', '_')
    kit_mis[mat] = instance(m_kit, f'MI_InfantryKit_{safe}', scalars={'Roughness': rough, 'Metallic': float(metal)})
    crowd_kit_mis[mat] = instance(m_crowd_kit, f'MI_InfantryCrowdKit_{safe}', scalars={'Roughness': rough, 'Metallic': float(metal)})

# ---------------------------------------------------------------------------------------------- assignments, sockets

def assign_static(mesh, by_slot):
    materials = mesh.get_editor_property('static_materials')
    for i, slot in enumerate(materials):
        key = str(slot.get_editor_property('material_slot_name'))
        if key in by_slot:
            mesh.set_material(i, by_slot[key])
        else:
            log['errors'].append(f'{mesh.get_name()}: no material for slot {key}')


for name, mesh in kit_meshes.items():
    assign_static(mesh, {f'Kit_{m}': mi for m, mi in kit_mis.items()})
    save(mesh)
for mesh in crowd.values():
    assign_static(mesh, {'InfantryCrowd_Body': crowd_mis['DE'], 'InfantryCrowd_Crown': crowd_mis['DE']})
    save(mesh)

slots = body.get_editor_property('materials')
for slot in slots:
    slot.set_editor_property('material_interface', body_mis['DE'])
body.set_editor_property('materials', slots)
anchors = MANIFEST['bones'][16:]
for bone in anchors:
    existing = body.find_socket(bone)
    if existing:
        body.remove_socket(bone)
    # SocketName and BoneName are read-only to scripts: add the socket, parent it, then rename it
    socket = unreal.SkeletalMeshSocket(outer=body)
    body.add_socket(socket, False)
    socket.set_socket_parent(body, bone)
    if not body.rename_socket(socket.get_editor_property('socket_name'), bone):
        log['errors'].append(f'socket {bone} could not be named')
save(body)
if skeleton:
    save(skeleton)

# ---------------------------------------------------------------------------------------------- what was built

log['skeletalMesh'] = body.get_path_name()
log['skeleton'] = skeleton.get_path_name() if skeleton else None
log['materialSlots'] = [str(s.get_editor_property('material_slot_name')) for s in body.get_editor_property('materials')]
log['sockets'] = [str(body.get_socket_by_index(i).get_editor_property('socket_name')) for i in range(body.num_sockets())]
log['kitMeshes'] = len(kit_meshes)
log['kitSlots'] = {name: [str(s.get_editor_property('material_slot_name')) for s in mesh.get_editor_property('static_materials')] for name, mesh in list(kit_meshes.items())[:5]}
log['crowdSlots'] = {n: [str(s.get_editor_property('material_slot_name')) for s in m.get_editor_property('static_materials')] for n, m in crowd.items()}
json.dump(log, open(LOG_PATH, 'w'), indent=1)
