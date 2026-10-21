"""Shared by make-crowd-look.py (exec'd there): the crowd's GPU skinning and dithered fade as material nodes. SKIN and
DITHER are make-materials.py's text (itself copied from unreal/Tools/models/infantry/import_infantry.py and
import-models.py OPACITY_HLSL); this file only splits the skinning into its parts so the model look can take the
skinned normal. Needs: unreal, mel, log, BONE, FADE in the caller's globals."""

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
DITHER = '''// Dithered fade (unreal/Tools/models/import-models.py OPACITY_HLSL): opaque pixels in proportion to Fade, so faded
// men stay in the opaque passes. The reference alpha-blends them (main.js camouflage opacity 0.45, the corpses' fade
// attribute); the dither is the deliberate difference, as for every faded model in this client.
float2 p = Parameters.SvPosition.xy + 5.588238 * (float)View.StateFrameIndexMod8;
float n = frac(52.9829189 * frac(dot(p, float2(0.06711056, 0.00583715))));
return Fade - n + 0.5;'''

GRIME = '''// client/unit-models.js mergeParts and grimeAt with LOOKS.soldier (mud 0.3, dust 0.2, most 0.6): every soldier
// vertex carries in its matId the grime it gathered at its height above the figure's feet in the STANDING build (the
// build bakeMeshes merges; the posture and gait morphs do not change it), more dust on faces turned up. Here the
// standing build is keyframe 0 of the crowd's texture (TCInfantryRig.cpp BakeKeyframes: the standing build first),
// without the pose node (stand is not undone by it). Figure units (cm), so y = z / 100 m, ny = the normal's z.
float4 J = Rigid > 0.5 ? float4(Bone, 0, 0, 0) : float4(J01, J23);
float4 W = Rigid > 0.5 ? float4(1, 0, 0, 0) : float4(W01, W23);
float3 q = float3(0, 0, 0), n = float3(0, 0, 0);
[unroll] for (int b = 0; b < 4; b++)
{
    if (W[b] > 0)
    {
        int x = (int)(J[b] + 0.5) * 3;
        float4 c0 = Keys.Load(int3(x, 0, 0)), c1 = Keys.Load(int3(x + 1, 0, 0)), c2 = Keys.Load(int3(x + 2, 0, 0));
        q += W[b] * float3(dot(c0, float4(P, 1)), dot(c1, float4(P, 1)), dot(c2, float4(P, 1)));
        n += W[b] * float3(dot(c0, float4(N, 0)), dot(c1, float4(N, 0)), dot(c2, float4(N, 0)));
    }
}
float y = q.z * 0.01, ny = normalize(n).z;
float k = saturate((y - 0.3 * 0.1) / (0.3 * 0.9));
float mud = 1.0 - k * k * (3.0 - 2.0 * k);
float dust = 0.2 + 0.12 * max(0.0, ny);
return min(min(0.6, max(mud, dust)), 0.98);'''


def expr(material, cls, x, y, **props):
    node = mel.create_material_expression(material, cls, x, y)
    for key, value in props.items():
        node.set_editor_property(key, value)
    return node


OUT_TYPES = {'f1': unreal.CustomMaterialOutputType.CMOT_FLOAT1, 'f2': unreal.CustomMaterialOutputType.CMOT_FLOAT2,
             'f3': unreal.CustomMaterialOutputType.CMOT_FLOAT3, 'f4': unreal.CustomMaterialOutputType.CMOT_FLOAT4}


def custom(material, x, y, code, inputs, output=unreal.CustomMaterialOutputType.CMOT_FLOAT3, description='', outputs=()):
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
    extra = []
    for name, kind in outputs:
        o = unreal.CustomOutput()
        o.set_editor_property('output_name', name)
        o.set_editor_property('output_type', OUT_TYPES[kind])
        extra.append(o)
    if extra:
        node.set_editor_property('additional_outputs', extra)
    return node


def link(src, src_out, dst, dst_in):
    if not mel.connect_material_expressions(src, src_out, dst, dst_in):
        log['errors'].append(f'link {src.get_name()}.{src_out} -> {dst.get_name()}.{dst_in}')


def prop(src, src_out, material_property):
    if not mel.connect_material_property(src, src_out, material_property):
        log['errors'].append(f'property {src.get_name()}.{src_out} -> {material_property}')


def transform(material, x, y, source, dest):
    node = expr(material, unreal.MaterialExpressionTransform, x, y)
    node.set_editor_property('transform_source_type', source)
    node.set_editor_property('transform_type', dest)
    return node


def skin_nodes(material, rigid):
    """make-materials.py crowd_skinning, in parts: the world position offset is wired; returns the skinned local position
    (pixel shader), the skinned local normal (interpolated) and the skinned world normal."""
    S, D = unreal.MaterialVectorCoordTransformSource, unreal.MaterialVectorCoordTransform
    keys = expr(material, unreal.MaterialExpressionTextureObjectParameter, -3200, -600, parameter_name='Keyframes')
    local = expr(material, unreal.MaterialExpressionLocalPosition, -3200, -450, included_offsets=unreal.PositionIncludedOffsets.EXCLUDE_OFFSETS)
    normal_ws = expr(material, unreal.MaterialExpressionVertexNormalWS, -3200, -350)
    to_local = transform(material, -3000, -350, S.TRANSFORMSOURCE_WORLD, D.TRANSFORM_LOCAL)
    link(normal_ws, '', to_local, '')
    data = [expr(material, unreal.MaterialExpressionPerInstanceCustomData3Vector, -3200, -250 + 80 * i, data_index=3 * i) for i in range(4)]
    uvs = [expr(material, unreal.MaterialExpressionTextureCoordinate, -3200, 80 + 70 * i, coordinate_index=i + 1) for i in range(4)]
    bone = expr(material, unreal.MaterialExpressionPerInstanceCustomData, -3200, 380, data_index=BONE)
    made = []
    for mode in (0, 1):
        node = custom(material, -2700, -500 + 400 * mode, SKIN, ['Keys', 'P', 'J01', 'J23', 'W01', 'W23', 'D0', 'D1', 'D2', 'D3', 'Bone', 'Rigid', 'Mode'],
                      description='Infantry skin position' if mode == 0 else 'Infantry skin normal')
        link(keys, '', node, 'Keys')
        link(local if mode == 0 else to_local, '', node, 'P')
        for i, name in enumerate(['J01', 'J23', 'W01', 'W23']):
            link(uvs[i], '', node, name)
        for i in range(4):
            link(data[i], '', node, f'D{i}')
        link(bone, '', node, 'Bone')
        link(expr(material, unreal.MaterialExpressionConstant, -2900, 450 + 60 * mode, r=1.0 if rigid else 0.0), '', node, 'Rigid')
        link(expr(material, unreal.MaterialExpressionConstant, -2900, 520 + 60 * mode, r=float(mode)), '', node, 'Mode')
        made.append(node)
    delta = expr(material, unreal.MaterialExpressionSubtract, -2400, -500)
    link(made[0], '', delta, 'A'); link(local, '', delta, 'B')
    to_world = transform(material, -2250, -500, S.TRANSFORMSOURCE_LOCAL, D.TRANSFORM_WORLD)
    link(delta, '', to_world, '')
    prop(to_world, '', unreal.MaterialProperty.MP_WORLD_POSITION_OFFSET)
    interp = expr(material, unreal.MaterialExpressionVertexInterpolator, -2400, -100)
    link(made[1], '', interp, '')
    n_world = transform(material, -2250, -100, S.TRANSFORMSOURCE_LOCAL, D.TRANSFORM_WORLD)
    link(interp, '', n_world, '')
    n_unit = expr(material, unreal.MaterialExpressionNormalize, -2100, -100)
    link(n_world, '', n_unit, '')
    # the pixel's own local position, the skin included (three's vModelPos: the posed figure in its own space)
    pos = expr(material, unreal.MaterialExpressionLocalPosition, -2250, -700, included_offsets=unreal.PositionIncludedOffsets.INCLUDE_OFFSETS)
    # unit-models.js mergeParts: the grime each vertex gathered, baked once on the standing build (key 0)
    grime = custom(material, -2700, 300, GRIME, ['Keys', 'P', 'N', 'J01', 'J23', 'W01', 'W23', 'Bone', 'Rigid'], output=unreal.CustomMaterialOutputType.CMOT_FLOAT1,
                   description='Crowd grime on the standing build')
    link(keys, '', grime, 'Keys'); link(local, '', grime, 'P'); link(to_local, '', grime, 'N')
    for i, name in enumerate(['J01', 'J23', 'W01', 'W23']):
        link(uvs[i], '', grime, name)
    link(bone, '', grime, 'Bone')
    link(expr(material, unreal.MaterialExpressionConstant, -2900, 640, r=1.0 if rigid else 0.0), '', grime, 'Rigid')
    grime_px = expr(material, unreal.MaterialExpressionVertexInterpolator, -2400, 300)
    link(grime, '', grime_px, '')
    return pos, interp, n_unit, grime_px


def dithered(material):
    material.set_editor_property('blend_mode', unreal.BlendMode.BLEND_MASKED)
    material.set_editor_property('opacity_mask_clip_value', 0.5)
    fade = expr(material, unreal.MaterialExpressionPerInstanceCustomData, -500, 800, data_index=FADE, const_default_value=1.0)
    node = custom(material, -300, 800, DITHER, ['Fade'], output=unreal.CustomMaterialOutputType.CMOT_FLOAT1, description='Dithered fade')
    link(fade, '', node, 'Fade')
    prop(node, '', unreal.MaterialProperty.MP_OPACITY_MASK)
