"""Generates /Game/Water (unreal/Content/Water): the water and apron materials, ports of the pinned reference shaders.

    M_TCWater   client/water.js FRAG_COLOR, FRAG_ROUGH, FRAG_NORMAL and FRAG_SHEEN on a MeshStandardMaterial (roughness
                0.46, metalness 0, transparent, no depth write): translucent, lit per pixel (Surface ForwardShading),
                world-space normal. Parameters: Mask (the 4-texel-per-cell mask: R coverage, G depth, B fords, A bridges),
                Noise (the 128 x 128 ripple table), Fog (one texel per cell: 1 seen, 0.55 explored, 0.32 never seen),
                Size (map metres x, z), Time (seconds, wrapped at 1000), Low (1: gfx.low, the drift frozen), FogGamma (the
                fog multiplier is raised to it: 1 multiplies linear colour as the world pieces' FogMap does; 2.2 dims as
                ATCTerrain's vertex colours, which scale sRGB bytes). UV0 is the
                per-vertex flow (water.js attribute 'flow'). Colours are three.Color hexes in linear (r186 converts them).
    M_TCApron   client/apron.js FRAG_COLOR on a MeshLambertMaterial: opaque, specular 0, roughness 1. Parameters:
                Grass, Dirt, Mud, Field, Road, Water (the tiles, sampled raw like three's DataTextures), Cells (one texel
                per cell: main class, secondary class, amount; read with Load like texelFetch), Noise (ground.js NP and
                NA), Fog, MetersA, MetersB, MapSize, CellDim, Unseen, TileSRGB (1: the tiles decode sRGB), TileTint (1: the
                tiles are the raw client/textures photos, tinted in the shader as ground.js buildTiles tints them). The fog of war over it is surfaces.js
                fogOverlayShader's: the edge cells' fog carries on outward, then fades to unseen between 6 and 32 m.
Both write /Game/Water/WaterMaterials.json (sources, hashes, the generated HLSL's hash).

Run in an editor whose Content is this repo's:
  UnrealEditor-Cmd <project> -run=pythonscript -EnablePlugins=PythonScriptPlugin -unattended -nop4 -nosplash -nullrhi \
    -script="<repo>/unreal/Tools/water/make-water-materials.py"
  ... -script="<repo>/unreal/Tools/water/make-water-materials.py --verify <report.json>"   (loads and checks them)
"""
import hashlib
import json
import os
import sys

import unreal

ROOT = '/Game/Water'
TOOL_VERSION = 3
HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, '..', '..', '..'))
REF = os.path.join(REPO, 'reference', 'ww2-rts')

tools = unreal.AssetToolsHelpers.get_asset_tools()
eal = unreal.EditorAssetLibrary
mel = unreal.MaterialEditingLibrary
P = unreal.MaterialProperty
T = unreal.CustomMaterialOutputType


def log(msg):
    unreal.log('[water] ' + msg)


def srgb_linear(c):
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def hex_linear(h):
    return [srgb_linear(((h >> s) & 255) / 255.0) for s in (16, 8, 0)]


def hlsl3(v):
    return 'float3(%.9f, %.9f, %.9f)' % tuple(v)


# client/water.js uniforms (THREE.Color: sRGB hex -> linear working colour)
DEEP, SHALLOW, FORD, FOAM, SKY = (hex_linear(h) for h in (0x234a4c, 0x3a6a66, 0x7a9a88, 0xddd6bc, 0xb9c6c2))

# FRAG_COLOR, FRAG_ROUGH, FRAG_NORMAL and FRAG_SHEEN of client/water.js, in Unreal's axes (reference x, z = Unreal X, Y;
# up = Z). 'Which' picks the output: 0 colour and opacity, 1 the world normal, 2 the sheen (emissive), 3 roughness and
# specular. The fog of war dims colour, sheen and specular alike (the browser's dark overlay over the lit water).
WATER_HLSL = '''float2 xz = WPos.xy * 0.01;
float4 wm = Mask.SampleLevel(MaskSampler, xz / Size.xy, 0);
float cover = smoothstep(0.26, 0.34, wm.r);
float deep = smoothstep(0.3, 0.85, wm.g), ford = smoothstep(0.25, 0.6, wm.b), bridge = wm.a;
float2 nuv = xz / 9.0;
float4 nA = Noise.Sample(NoiseSampler, xz / 31.0 + Time * float2(0.004, -0.003));
float4 n0;
if (Low > 0.5) n0 = Noise.Sample(NoiseSampler, nuv);
else
{
	// two copies of the ripples slide along the flow half a cycle apart and cross-fade, so nothing stretches forever
	float p0 = frac(Time * 0.22), p1 = frac(Time * 0.22 + 0.5), k0 = 1.0 - abs(1.0 - 2.0 * p0);
	float2 drift = Flow * (2.2 / 9.0);
	n0 = lerp(Noise.Sample(NoiseSampler, nuv - drift * p1 + 0.37), Noise.Sample(NoiseSampler, nuv - drift * p0), k0);
}
// fine detail fades out when a pixel covers more ground than a ripple, so far water does not shimmer
float detail = 1.0 - smoothstep(0.12, 0.45, length(fwidth(xz)));
// calmer in open water, with slow patches of rougher and smoother water so a big sea does not show the tiling
float chop = lerp(0.32, 0.16, ford) * lerp(1.0, 0.6, deep) * (0.45 + 0.9 * nA.a);
float2 slope = (n0.gb * 2.0 - 1.0) * chop * lerp(0.35, 1.0, detail) + (nA.gb * 2.0 - 1.0) * 0.08;
float3 wN = normalize(float3(-slope.x, 1.0, -slope.y));
float3 wCol = lerp(%(shallow)s, %(deep)s, deep);
wCol = lerp(wCol, %(ford)s, ford);
wCol *= 0.93 + 0.14 * nA.r;
// pebbles on the ford bed show through the shallow water
wCol *= 1.0 - 0.2 * ford * smoothstep(0.62, 0.8, Noise.Sample(NoiseSampler, xz / 2.3 + 0.21).a);
// light dry-brushed ripple crests that ride the flow, more of them over the shallow fords
float crest = smoothstep(lerp(0.64, 0.56, ford), 0.84, n0.r) * (1.0 - 0.5 * deep) * detail;
wCol = lerp(wCol, %(foam)s, crest * (0.14 + 0.3 * ford));
// a broken pale band where the water meets the bank
float brk = smoothstep(0.35, 0.75, n0.a * 0.6 + nA.a * 0.6 - 0.1);
float wFoam = (1.0 - smoothstep(0.31, 0.52, wm.r)) * (0.25 + 0.75 * brk) * 0.8;
// foam hugging the bridge piers, plus a short wake downstream of each bridge
float ring = smoothstep(0.03, 0.14, bridge) * (1.0 - smoothstep(0.3, 0.55, bridge));
if (Low < 0.5)
{
	float up = Mask.SampleLevel(MaskSampler, (xz - Flow * 3.5) / Size.xy, 0).a;
	ring = max(ring, smoothstep(0.25, 0.65, up) * (1.0 - smoothstep(0.08, 0.35, bridge)) * brk);
}
wFoam = max(wFoam, ring * (0.55 + 0.45 * brk));
wCol = lerp(wCol, %(foam)s, wFoam);
float alpha = cover < 0.004 ? 0.0 : cover * max(lerp(0.86, 0.6, ford), wFoam * 0.95);
float fogK = pow(Fog.SampleLevel(FogSampler, xz / Size.xy, 0).r, FogGamma);
float3 nW = float3(wN.x, wN.z, wN.y);
// a little sky sheen, stronger at grazing angles, so the surface reads as glossy under any sun angle
float wFres = 1.0 - saturate(dot(nW, normalize(CamVec)));
float3 sheen = %(sky)s * (0.015 + 0.25 * wFres * wFres * wFres) * (1.0 - wFoam);
if (Which < 0.5) return float4(wCol * fogK, alpha);
if (Which < 1.5) return float4(nW, 0.0);
if (Which < 2.5) return float4(sheen * fogK, 0.0);
return float4(lerp(0.46, 0.95, wFoam), 0.5 * fogK, 0.0, 0.0);
''' % dict(shallow=hlsl3(SHALLOW), deep=hlsl3(DEEP), ford=hlsl3(FORD), foam=hlsl3(FOAM), sky=hlsl3(SKY))

# client/apron.js FRAG_COLOR (and STEP_GLSL, apNoise, apLinear), then fogOverlayShader's darkening, as the base colour
APRON_HLSL = '''float2 apw = WPos.xy * 0.01;
float apy = WPos.z * 0.01;
float2 apu = apw / 2.0;
#define AP_STEP(x) (saturate(x) * saturate(x) * (3.0 - 2.0 * saturate(x)))
#define AP_NOISE(u) Noise.SampleLevel(NoiseSampler, ((u) + 0.5) / 512.0, 0).rg
// the four nearest cell centres, looked up at a warped position (the same warp and patch noise as ground.js)
float2 apwarp = float2(AP_NOISE(float2(apu.x * 16.0 + 1101.0, apu.y * 16.0 + 37.0)).g, AP_NOISE(float2(apu.x * 16.0 + 311.0, apu.y * 16.0 + 1211.0)).g) * 2.0 - 1.0;
float2 apf = apu + apwarp * 0.32 - 0.5;
float2 api = floor(apf);
float2 apt = apf - api;
float appn = AP_NOISE(apu * 6.0 + float2(57.0, 213.0)).r;
float4 apk = float4((1.0 - apt.x) * (1.0 - apt.y), apt.x * (1.0 - apt.y), (1.0 - apt.x) * apt.y, apt.x * apt.y);
apk *= apk; apk /= apk.x + apk.y + apk.z + apk.w;
float apwt[7] = { 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0 };
[unroll] for (int k = 0; k < 4; k++)
{
	int2 cc = clamp(int2(api) + int2(k & 1, k >> 1), int2(0, 0), int2(CellDim.xy) - 1);
	float4 cell = Cells.Load(int3(cc, 0)) * 255.0;
	int prim = int(cell.r + 0.5), sec = int(cell.g + 0.5);
	float amt = cell.b / 255.0;
	if (amt > 0.0)
	{
		float s = AP_STEP((amt - appn) * 5.5 + 0.5);
		apwt[sec] += apk[k] * s; apwt[prim] += apk[k] * (1.0 - s);
	}
	else apwt[prim] += apk[k];
}
// distance from the map: fields fade out, then everything settles into plain grass and dirt (water keeps going)
float apd = length(apw - clamp(apw, float2(0.0, 0.0), MapSize.xy));
float apFieldFade = AP_STEP((apd - 14.0) / 22.0);
float apFields = apwt[3] + apwt[6];
apwt[0] += apFields * apFieldFade; apwt[3] *= 1.0 - apFieldFade; apwt[6] *= 1.0 - apFieldFade;
float apFar = AP_STEP((apd - 40.0) / 200.0), apDry = 1.0 - apwt[5], apPatch = 0.6 * AP_STEP((0.11 - appn) * 5.5 + 0.5);
[unroll] for (int i = 0; i < 7; i++) if (i != 5) apwt[i] *= 1.0 - apFar;
apwt[0] += apFar * apDry * (1.0 - apPatch); apwt[1] += apFar * apDry * apPatch;
// only the tiles that count are sampled; the derivatives are taken first, so the filtering stays right in the branches.
// A tile is either ground.js's own tinted tile (TileTint 0: the flat colours before the textures load) or the raw
// client/textures photo (TileTint 1), tinted here as buildTiles does: contrast about the LOOK average, the colour
// multiplier, then saturation about the luma, in sRGB bytes. TileSRGB 1: the texture decodes sRGB, so it is encoded back.
const float3 apAvg[6] = { float3(107, 103, 44) / 255.0, float3(160, 117, 72) / 255.0, float3(87, 64, 43) / 255.0, float3(103, 70, 44) / 255.0, float3(162, 136, 109) / 255.0, float3(57, 83, 82) / 255.0 };
const float3 apMul[6] = { float3(1.02, 1.06, 1.35), float3(0.95, 1.0, 1.04), float3(1.0, 1.0, 1.0), float3(0.95, 0.95, 1.0), float3(0.9, 0.9, 0.88), float3(0.95, 1.0, 1.08) };
const float apSat[6] = { 0.78, 0.5, 0.72, 0.5, 0.62, 1.0 };
const float apCon[6] = { 0.9, 1.0, 1.0, 0.55, 1.0, 1.0 };
#define AP_ADD(w, T, uv, dx, dy, i) if ((w) > 0.003) { float3 c = T.SampleGrad(T##Sampler, uv, dx, dy).rgb; \
	if (TileSRGB > 0.5) c = lerp(1.055 * pow(max(c, 1e-6), 1.0 / 2.4) - 0.055, c * 12.92, step(c, 0.0031308)); \
	if (TileTint > 0.5) { float3 r = (apAvg[i] + (c - apAvg[i]) * apCon[i]) * apMul[i]; float yy = dot(r, float3(0.3, 0.59, 0.11)); c = saturate(yy + (r - yy) * apSat[i]); } \
	apc += (w) * c; }
float2 apdx = ddx(apw), apdy = ddy(apw);
float3 apc = float3(0.0, 0.0, 0.0);
AP_ADD(apwt[0], Grass, apw / MetersA.x, apdx / MetersA.x, apdy / MetersA.x, 0)
AP_ADD(apwt[1], Dirt, apw / MetersA.y, apdx / MetersA.y, apdy / MetersA.y, 1)
AP_ADD(apwt[2], Mud, apw / MetersA.z, apdx / MetersA.z, apdy / MetersA.z, 2)
AP_ADD(apwt[3], Field, apw / MetersB.x, apdx / MetersB.x, apdy / MetersB.x, 3)
AP_ADD(apwt[6], Field, apw.yx / MetersB.x, apdx.yx / MetersB.x, apdy.yx / MetersB.x, 3)
AP_ADD(apwt[4], Road, apw / MetersB.y, apdx / MetersB.y, apdy / MetersB.y, 4)
AP_ADD(apwt[5], Water, apw / MetersB.z, apdx / MetersB.z, apdy / MetersB.z, 5)
#undef AP_ADD
// broad light and dark sweeps, so the repeat of the tiles does not show
apc *= 1.0 + 0.14 * (AP_NOISE(apu * 2.3 + float2(900.0, 431.0)).g * 2.0 - 1.0);
// a pale line where water meets land
float apWet = apwt[5];
if (apWet > 0.06 && apWet < 0.94) { float kk = 1.0 - abs(apWet - 0.5) * 2.2; if (kk > 0.0) apc = lerp(apc, float3(0.7216, 0.698, 0.5725), kk * kk * 0.45); }
apc = lerp(dot(apc, float3(0.3, 0.59, 0.11)).xxx, apc, 1.0 - 0.25 * AP_STEP(apd / 14.0));
float3 apcl = saturate(apc);
float3 col = lerp(pow(apcl * 0.9478672986 + 0.0521327014, 2.4), apcl * 0.0773993808, step(apcl, 0.04045));
// the same warm lift with height as the map's relief paint, so the two match at the edge
col *= lerp(float3(1.01, 0.98, 0.92), float3(1.12, 1.05, 0.98), smoothstep(-5.0, 10.0, apy));
// the playable area's edge: past it the ground is darker and greyer, easing in over 14 m (a soft margin, no line)
col *= 1.0 - 0.2 * AP_STEP(apd / 14.0);
// the fog of war: the map's edge cells carry on outward for a few metres, then the ground counts as never seen
float fogK = pow(lerp(Fog.SampleLevel(FogSampler, apw / MapSize.xy, 0).r, Unseen, smoothstep(6.0, 32.0, apd)), FogGamma);
#undef AP_STEP
#undef AP_NOISE
return col * fogK;
'''


def fresh(path, cls, factory):
    folder, name = path.rsplit('/', 1)
    if eal.does_asset_exist(path):
        asset = eal.load_asset(path)
        if asset and isinstance(asset, cls):
            return asset
        eal.delete_asset(path)
    return tools.create_asset(name, folder, cls, factory)


def expr(mat, cls, x, y, **props):
    e = mel.create_material_expression(mat, cls, x, y)
    for k, v in props.items():
        e.set_editor_property(k, v)
    return e


def custom(mat, code, inputs, ret, x, y, desc):
    c = expr(mat, unreal.MaterialExpressionCustom, x, y)
    c.set_editor_property('code', code)
    c.set_editor_property('description', desc)
    c.set_editor_property('output_type', ret)
    ins = []
    for n, _src, _out in inputs:
        ci = unreal.CustomInput()
        ci.set_editor_property('input_name', n)
        ins.append(ci)
    c.set_editor_property('inputs', ins)
    for n, src, out in inputs:
        if not mel.connect_material_expressions(src, out, c, n):
            raise RuntimeError('%s: could not connect %s' % (desc, n))
    return c


def mask(mat, src, x, y, r=False, g=False, b=False, a=False):
    m = expr(mat, unreal.MaterialExpressionComponentMask, x, y, r=r, g=g, b=b, a=a)
    mel.connect_material_expressions(src, '', m, '')
    return m


def connect_prop(src, out, prop):
    if not mel.connect_material_property(src, out, prop):
        raise RuntimeError('could not connect %s' % prop)


def new_material(name):
    mat = fresh(ROOT + '/' + name, unreal.Material, unreal.MaterialFactoryNew())
    mel.delete_all_material_expressions(mat)
    return mat


def white():
    return unreal.load_asset('/Engine/EngineResources/WhiteSquareTexture')


def water_material():
    mat = new_material('M_TCWater')
    mat.set_editor_property('blend_mode', unreal.BlendMode.BLEND_TRANSLUCENT)
    mat.set_editor_property('shading_model', unreal.MaterialShadingModel.MSM_DEFAULT_LIT)
    mat.set_editor_property('translucency_lighting_mode', unreal.TranslucencyLightingMode.TLM_SURFACE_PER_PIXEL_LIGHTING)
    mat.set_editor_property('tangent_space_normal', False)
    mat.set_editor_property('two_sided', False)
    X, y = -1400, [0]

    def row():
        y[0] += 110
        return y[0]

    def scalar(n, v):
        return expr(mat, unreal.MaterialExpressionScalarParameter, X, row(), parameter_name=n, default_value=v)

    def texobj(n):
        return expr(mat, unreal.MaterialExpressionTextureObjectParameter, X, row(), parameter_name=n, texture=white())

    ins = [('WPos', expr(mat, unreal.MaterialExpressionWorldPosition, X, row()), ''),
           ('Flow', expr(mat, unreal.MaterialExpressionTextureCoordinate, X, row(), coordinate_index=0), ''),
           ('CamVec', expr(mat, unreal.MaterialExpressionCameraVectorWS, X, row()), ''),
           ('Mask', texobj('Mask'), ''), ('Noise', texobj('Noise'), ''), ('Fog', texobj('Fog'), ''),
           ('Size', expr(mat, unreal.MaterialExpressionVectorParameter, X, row(), parameter_name='Size', default_value=unreal.LinearColor(160, 160, 0, 0)), ''),
           ('Time', scalar('Time', 0.0), ''), ('Low', scalar('Low', 0.0), ''), ('FogGamma', scalar('FogGamma', 1.0), '')]
    outs = []
    for which in range(4):
        k = expr(mat, unreal.MaterialExpressionConstant, X, row(), r=float(which))
        outs.append(custom(mat, WATER_HLSL, ins + [('Which', k, '')], T.CMOT_FLOAT4, -800, which * 250, 'TCWater%d' % which))
    connect_prop(mask(mat, outs[0], -450, 0, r=True, g=True, b=True), '', P.MP_BASE_COLOR)
    connect_prop(mask(mat, outs[0], -450, 100, a=True), '', P.MP_OPACITY)
    connect_prop(mask(mat, outs[1], -450, 250, r=True, g=True, b=True), '', P.MP_NORMAL)
    connect_prop(mask(mat, outs[2], -450, 500, r=True, g=True, b=True), '', P.MP_EMISSIVE_COLOR)
    connect_prop(mask(mat, outs[3], -450, 750, r=True), '', P.MP_ROUGHNESS)
    connect_prop(mask(mat, outs[3], -450, 850, g=True), '', P.MP_SPECULAR)
    connect_prop(expr(mat, unreal.MaterialExpressionConstant, -450, 950, r=0.0), '', P.MP_METALLIC)
    return mat


def apron_material():
    mat = new_material('M_TCApron')
    mat.set_editor_property('blend_mode', unreal.BlendMode.BLEND_OPAQUE)
    mat.set_editor_property('shading_model', unreal.MaterialShadingModel.MSM_DEFAULT_LIT)
    X, y = -1400, [0]

    def row():
        y[0] += 110
        return y[0]

    def vector(n, v):
        return expr(mat, unreal.MaterialExpressionVectorParameter, X, row(), parameter_name=n, default_value=unreal.LinearColor(v[0], v[1], v[2], 0))

    def texobj(n):
        return expr(mat, unreal.MaterialExpressionTextureObjectParameter, X, row(), parameter_name=n, texture=white())

    ins = [('WPos', expr(mat, unreal.MaterialExpressionWorldPosition, X, row()), '')]
    ins += [(n, texobj(n), '') for n in ('Grass', 'Dirt', 'Mud', 'Field', 'Road', 'Water', 'Cells', 'Noise', 'Fog')]
    ins += [('MetersA', vector('MetersA', (9, 8, 7)), ''), ('MetersB', vector('MetersB', (10, 8, 12)), ''),
            ('MapSize', vector('MapSize', (160, 160, 0)), ''), ('CellDim', vector('CellDim', (80, 80, 0)), ''),
            ('Unseen', expr(mat, unreal.MaterialExpressionScalarParameter, X, row(), parameter_name='Unseen', default_value=82.0 / 255.0), ''),
            ('TileSRGB', expr(mat, unreal.MaterialExpressionScalarParameter, X, row(), parameter_name='TileSRGB', default_value=0.0), ''),
            ('TileTint', expr(mat, unreal.MaterialExpressionScalarParameter, X, row(), parameter_name='TileTint', default_value=0.0), ''),
            ('FogGamma', expr(mat, unreal.MaterialExpressionScalarParameter, X, row(), parameter_name='FogGamma', default_value=1.0), '')]
    c = custom(mat, APRON_HLSL, ins, T.CMOT_FLOAT3, -700, 0, 'TCApron')
    connect_prop(c, '', P.MP_BASE_COLOR)
    connect_prop(expr(mat, unreal.MaterialExpressionConstant, -450, 200, r=0.0), '', P.MP_SPECULAR)
    connect_prop(expr(mat, unreal.MaterialExpressionConstant, -450, 300, r=1.0), '', P.MP_ROUGHNESS)
    return mat


def sha_file(p):
    with open(p, 'rb') as f:
        return hashlib.sha256(f.read()).hexdigest()


def build():
    made = {'M_TCWater': water_material(), 'M_TCApron': apron_material()}
    report = dict(format='tc-water-materials-v1', tool_version=TOOL_VERSION, materials={}, sources={})
    for rel in ('client/water.js', 'client/apron.js', 'client/surfaces.js', 'client/ground.js'):
        p = os.path.join(REF, rel)
        if os.path.exists(p):
            report['sources'][rel] = sha_file(p)
    for name, m in made.items():
        mel.recompile_material(m)
        eal.save_loaded_asset(m, only_if_is_dirty=False)
        code = WATER_HLSL if name == 'M_TCWater' else APRON_HLSL
        report['materials'][name] = dict(path=ROOT + '/' + name, hlsl_sha256=hashlib.sha256(code.encode()).hexdigest())
        log('saved %s' % name)
    out = os.path.join(REPO, 'unreal', 'Content', 'Water', 'WaterMaterials.json')
    os.makedirs(os.path.dirname(out), exist_ok=True)
    with open(out, 'w') as f:
        json.dump(report, f, indent=1)
    log('wrote ' + out)


def verify(path):
    res = dict(materials={})
    ok = True
    for name in ('M_TCWater', 'M_TCApron'):
        m = eal.load_asset(ROOT + '/' + name)
        entry = dict(loaded=bool(m))
        if m:
            entry['blend_mode'] = str(m.get_editor_property('blend_mode'))
            entry['shading_model'] = str(m.get_editor_property('shading_model'))
            entry['base_color_node'] = str(mel.get_material_property_input_node(m, P.MP_BASE_COLOR))
            params = [str(n) for n in mel.get_texture_parameter_names(m)] + [str(n) for n in mel.get_vector_parameter_names(m)] + [str(n) for n in mel.get_scalar_parameter_names(m)]
            entry['parameters'] = sorted(params)
            stats = mel.get_statistics(m) if hasattr(mel, 'get_statistics') else None
            if stats:
                entry['pixel_instructions'] = stats.num_pixel_shader_instructions
                entry['samplers'] = stats.num_samplers
                ok = ok and stats.num_pixel_shader_instructions > 0
        ok = ok and entry['loaded']
        res['materials'][name] = entry
    res['ok'] = ok
    with open(path, 'w') as f:
        json.dump(res, f, indent=1)
    log('verify: %s -> %s' % ('ok' if ok else 'FAILED', path))


args = sys.argv[1:]
if '--verify' in args:
    verify(args[args.index('--verify') + 1])
else:
    build()
