"""Builds the soldier crowd materials unreal/Source/ThreeCrossroads/Client/TCUnitMotion.cpp needs, in
/Game/Models/Motion (unreal/Content/Models/Motion). Run inside the editor on a host project whose Content/Models is
the real project's (unreal/Tools/motion/run.sh makes one):
  UnrealEditor-Cmd <host>.uproject -run=pythonscript -script=make-materials.py -unattended -nop4 -nosplash
  with MOTION_LOG=<result json path>
Creates (replacing what exists):
  M_InfantryCrowdFaded     M_InfantryCrowd (the infantry atlas, the faction tunic on the cloth mask, skinned on the
                           GPU from the keyframe texture) with the dithered fade of per-instance custom data 13:
                           camouflaged men (main.js opacity 0.45) and fading corpses (unit-models.js corpse fade)
  M_InfantryCrowdKitFaded  M_InfantryCrowdKit (vertex colors, rigid on custom data 12's bone) with the same fade
  M_InfantryFar            the reference's far-away figure (infantry.js farParts, baked by unit-models.js): vertex
                           colors carried at full precision in UV0, UV5 and UV6, the owner's arm band from custom data
                           14-16, every vertex skinned by its own keyframe column (joints and weights in UV1-4), which
                           plays the far figure's morph targets
  M_InfantryFarFaded       the same with the dithered fade
The look and skinning code is unreal/Tools/models/infantry/import_infantry.py's (ALBEDO, DECODE, SKIN, body_look,
kit_look, crowd_skinning), copied so the infantry materials stay untouched; the dither is import-models.py's
OPACITY_HLSL without its alpha-test branch.
"""
import json
import os
import unreal

ROOT = '/Game/Models/Motion'
LOG_PATH = os.environ.get('MOTION_LOG', '')
log = {'assets': [], 'errors': []}
tools = unreal.AssetToolsHelpers.get_asset_tools()
lib = unreal.EditorAssetLibrary
mel = unreal.MaterialEditingLibrary
color_tex = unreal.load_asset('/Game/Models/Infantry/Textures/T_InfantryV3_Color')
if not color_tex:
    log['errors'].append('no /Game/Models/Infantry/Textures/T_InfantryV3_Color (run unreal/Tools/models/infantry/run.sh --unreal first)')

# custom data (TCUnitMotion.h TCCrowdData)
BONE, FADE, OWNER = 12, 13, 14


def fresh(path):
    if lib.does_asset_exist(path):
        lib.delete_asset(path)


def save(asset):
    lib.save_loaded_asset(asset, only_if_is_dirty=False)
    log['assets'].append(asset.get_path_name())


def new_material(name, usage):
    path = f'{ROOT}/{name}'
    fresh(path)
    material = tools.create_asset(name, ROOT, unreal.Material, unreal.MaterialFactoryNew())
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


# ------------------------------------------------------------------ copied from unreal/Tools/models/infantry/import_infantry.py

ALBEDO = '''// client/infantry-material.js: the atlas, with the strongest baked highlights taken off the cloth so scene light
// supplies them, tinted toward the faction tunic where the cloth mask is set (infantry-authored.js colors)
float peak = max(Albedo.r, max(Albedo.g, Albedo.b));
float3 a = Albedo / (1.0 + Cloth * max(0.0, peak - 0.20) * 3.0);
return a * lerp(float3(1.0, 1.0, 1.0), Tunic, Cloth);'''
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

# ------------------------------------------------------------------ added here

DITHER = '''// Dithered fade (unreal/Tools/models/import-models.py OPACITY_HLSL): opaque pixels in proportion to Fade, so faded
// men stay in the opaque passes. The reference alpha-blends them (main.js camouflage opacity 0.45, the corpses' fade
// attribute); the dither is the deliberate difference, as for every faded model in this client.
float2 p = Parameters.SvPosition.xy + 5.588238 * (float)View.StateFrameIndexMod8;
float n = frac(52.9829189 * frac(dot(p, float2(0.06711056, 0.00583715))));
return Fade - n + 0.5;'''
FAR = '''// The far figure's own vertex colors (unit-models.js mergeParts: the figure's colors, its baked shading included),
// carried at full precision in UV0, UV5 and UV6 by TCUnitMotion.cpp; the arm band takes the owner's color
// (infantry.js farParts tunic.lerp(owner, 0.4)): base + tint * owner, linear.
return float3(A.x, A.y, B.x) + float3(B.y, C.x, C.y) * Owner;'''


def dithered(material):
    material.set_editor_property('blend_mode', unreal.BlendMode.BLEND_MASKED)
    material.set_editor_property('opacity_mask_clip_value', 0.5)
    fade = expr(material, unreal.MaterialExpressionPerInstanceCustomData, -500, 500, data_index=FADE, const_default_value=1.0)
    node = custom(material, -300, 500, DITHER, ['Fade'], output=unreal.CustomMaterialOutputType.CMOT_FLOAT1, description='Dithered fade')
    link(fade, '', node, 'Fade')
    prop(node, '', unreal.MaterialProperty.MP_OPACITY_MASK)


def far_look(material):
    a = expr(material, unreal.MaterialExpressionTextureCoordinate, -900, -200, coordinate_index=0)
    b = expr(material, unreal.MaterialExpressionTextureCoordinate, -900, -130, coordinate_index=5)
    c = expr(material, unreal.MaterialExpressionTextureCoordinate, -900, -60, coordinate_index=6)
    owner = expr(material, unreal.MaterialExpressionPerInstanceCustomData3Vector, -900, 10, data_index=OWNER, const_default_value=unreal.LinearColor(1, 1, 1, 1))
    color = custom(material, -500, -100, FAR, ['A', 'B', 'C', 'Owner'], description='Far figure color')
    link(a, '', color, 'A'); link(b, '', color, 'B'); link(c, '', color, 'C'); link(owner, '', color, 'Owner')
    prop(color, '', unreal.MaterialProperty.MP_BASE_COLOR)
    # unit-models.js INFANTRY_PAINT: roughness 0.9, metalness 0
    rough = expr(material, unreal.MaterialExpressionScalarParameter, -500, 150, parameter_name='Roughness', default_value=0.9)
    prop(rough, '', unreal.MaterialProperty.MP_ROUGHNESS)


def finish(material):
    mel.layout_material_expressions(material)
    mel.recompile_material(material)
    save(material)


ISM = ['used_with_instanced_static_meshes']
m = new_material('M_InfantryCrowdFaded', ISM)
body_look(m); crowd_skinning(m, False); dithered(m); finish(m)
m = new_material('M_InfantryCrowdKitFaded', ISM)
kit_look(m); crowd_skinning(m, True); dithered(m); finish(m)
m = new_material('M_InfantryFar', ISM)
far_look(m); crowd_skinning(m, False); finish(m)
m = new_material('M_InfantryFarFaded', ISM)
far_look(m); crowd_skinning(m, False); dithered(m); finish(m)

log['blend'] = {n: str(unreal.load_asset(f'{ROOT}/{n}').get_editor_property('blend_mode')) for n in ('M_InfantryCrowdFaded', 'M_InfantryCrowdKitFaded', 'M_InfantryFar', 'M_InfantryFarFaded')}
if LOG_PATH:
    json.dump(log, open(LOG_PATH, 'w'), indent=1)
print('make-materials: %d assets, %d errors' % (len(log['assets']), len(log['errors'])))
