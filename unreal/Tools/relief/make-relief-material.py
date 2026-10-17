"""Generates /Game/Relief/M_TCRelief (Content/Relief): the reference's client/relief-material.js for the native relief.

relief-material.js is a MeshLambertMaterial over the painted ground with three injected shader chunks:
  FRAG_PAINT   after <color_fragment>: height tint, meadow variation (High), dry earth on slopes, damp soil, plateau
               brightening per level, a warm crest along cliff lips, and on rock faces (reliefPaint.x > 0) weathered
               stone with a pale lip band, soil along the foot and grass held on upward ledges; the contact shadow of the
               foot paint on flat ground
  FRAG_NORMAL  after <normal_fragment_maps>: crater floors (reliefScar > 0.5) shaded almost flat; small paint bumps (High)
  FRAG_FILL    after <emissivemap_fragment>: a terrain bounce on rock walls and ramps, added as emissive
This material is the same code in three Custom nodes (HLSL transcribed line for line from the GLSL at pin e4e579a), on a
lit, specular 0, roughness 1 surface (a Lambert, as M_TCGroundLambert). Inputs per vertex, as ATCTerrain uploads the
relief (World/TCTerrain.cpp, TCRelief.h):
  vertex colour   the painted ground's colour (diffuseColor before FRAG_PAINT; until the painted ground lands it is the
                  minimap colour per cell, dimmed by fog)
  UV1             reliefPaint.xy: rock (1 wall band, 0.98 rock model, 0 ground), lip (cliff-lip paint, or a face's top + 2 m)
  UV2             reliefPaint.zw: foot (foot paint, or a face's foot height), damp (wet)
  UV3.x           reliefScar
  world position  vReliefPosition, in three's axes and metres: (X, Z, Y) / 100
Scalar parameter Low (1: RELIEF_LOW, the browser's Low graphics and every World map, client/main.js; 0: High). The
uReliefStep uniform is CFG.levelHeight, 2.5.

Run in an editor whose Content is this repo's (a commandlet is enough):
  UnrealEditor-Cmd <project> -run=pythonscript -unattended -nop4 -nosplash -nullrhi -script="<this file>"
It writes Content/Relief/M_TCRelief.uasset and Content/Relief/ReliefMaterial.json (sources, hashes, the HLSL's hash).
"""
import hashlib
import json
import os

import unreal

PATH = '/Game/Relief'
NAME = 'M_TCRelief'
VERSION = 1

tools = unreal.AssetToolsHelpers.get_asset_tools()
eal = unreal.EditorAssetLibrary
mel = unreal.MaterialEditingLibrary
P = unreal.MaterialProperty
T = unreal.CustomMaterialOutputType

# relief-material.js FRAG_PARS: reliefHash, reliefNoise2, reliefNoise (as methods of a struct: Custom nodes take no free
# functions)
HELPERS = r'''
struct FTCReliefNoise
{
	float Hash(float3 p) { p = frac(p * 0.1031); p += dot(p, p.yzx + 33.33); return frac((p.x + p.y) * p.z); }
	float Noise2(float2 p)
	{
		float2 i = floor(p), f = frac(p);
		f = f * f * (3.0 - 2.0 * f);
		return lerp(lerp(Hash(float3(i, 0.0)), Hash(float3(i + float2(1.0, 0.0), 0.0)), f.x),
			lerp(Hash(float3(i + float2(0.0, 1.0), 0.0)), Hash(float3(i + float2(1.0, 1.0), 0.0)), f.x), f.y);
	}
	float Noise(float3 p)
	{
		float3 i = floor(p), f = frac(p);
		f = f * f * (3.0 - 2.0 * f);
		return lerp(
			lerp(lerp(Hash(i), Hash(i + float3(1.0, 0.0, 0.0)), f.x), lerp(Hash(i + float3(0.0, 1.0, 0.0)), Hash(i + float3(1.0, 1.0, 0.0)), f.x), f.y),
			lerp(lerp(Hash(i + float3(0.0, 0.0, 1.0)), Hash(i + float3(1.0, 0.0, 1.0)), f.x), lerp(Hash(i + float3(0.0, 1.0, 1.0)), Hash(i + float3(1.0, 1.0, 1.0)), f.x), f.y), f.z);
	}
};
FTCReliefNoise R;
float3 Pos = float3(WP.x, WP.z, WP.y) / 100.0;   // vReliefPosition (three's axes, metres)
float3 reliefN = normalize(float3(VN.x, VN.z, VN.y));
float4 Paint = float4(RL.x, RL.y, FD.x, FD.y);  // vReliefPaint
float uReliefStep = 2.5;
float reliefSteep = 1.0 - abs(reliefN.y);
float reliefRock = clamp(Paint.x, 0.0, 1.0);
float reliefLip = clamp(Paint.y, 0.0, 1.0);
float reliefFoot = clamp(Paint.z, 0.0, 1.0);
float reliefDamp = clamp(Paint.w, 0.0, 1.0);
float reliefSlope = smoothstep(0.05, 0.2929, reliefSteep);
'''

# FRAG_PAINT (returns diffuseColor.rgb)
PAINT = HELPERS + r'''
float3 diffuseColor = Base;
float reliefPatch = 0.5;
if (Low < 0.5) reliefPatch = R.Noise2(Pos.xz * 0.52);
float reliefHeight = smoothstep(-5.0, 10.0, Pos.y);
float3 reliefTint = lerp(float3(1.01, 0.98, 0.92), float3(1.12, 1.05, 0.98), reliefHeight);
diffuseColor *= reliefTint;
if (Low < 0.5)
{
	float3 c = diffuseColor;
	float reliefGrass = smoothstep(0.45, 0.65, (min(c.r, c.g) - c.b) / max(max(c.r, c.g), 1e-4));
	float reliefMeadow = 0.65 * R.Noise2(Pos.xz * 0.021 + 17.3) + 0.35 * R.Noise2(Pos.xz * 0.075 - 4.1);
	reliefMeadow = smoothstep(0.2, 0.8, reliefMeadow);
	float3 reliefLush = float3(0.74, 0.9, 0.62), reliefParched = float3(1.14, 1.04, 0.8);
	diffuseColor *= lerp(float3(1.0, 1.0, 1.0), lerp(reliefLush, reliefParched, reliefMeadow), reliefGrass);
}
float reliefScarGate = 1.0 - clamp(Scar, 0.0, 1.0);
float reliefEarth = reliefSlope * 0.42 * reliefScarGate;
float3 reliefDry = float3(0.32, 0.24, 0.12);
if (Low < 0.5)
{
	reliefEarth *= 0.5 + 0.5 * smoothstep(0.20, 0.68, reliefPatch);
	reliefDry *= 0.72 + 0.34 * R.Noise(Pos * 2.7) + 0.18 * R.Noise(Pos * 9.0);
}
float reliefWetEarth = reliefDamp * 0.24;
float3 reliefSoil = lerp(float3(0.16, 0.115, 0.065), float3(0.14, 0.14, 0.075), reliefDamp);
diffuseColor = lerp(diffuseColor, reliefDry, reliefEarth);
diffuseColor = lerp(diffuseColor, reliefSoil, reliefWetEarth);
diffuseColor *= 1.0 - 0.16 * reliefSlope * (1.0 - reliefRock);
float reliefLevel = clamp(Pos.y / uReliefStep, 0.0, 3.0);
float reliefPlateau = reliefLevel * (1.0 - reliefSlope) * (1.0 - reliefRock) * (1.0 - reliefFoot) * (1.0 - reliefDamp) * (1.0 - clamp(Scar, 0.0, 1.0));
diffuseColor *= float3(1.0, 1.0, 1.0) + float3(0.13, 0.12, 0.10) * reliefPlateau;
float reliefCrest = smoothstep(0.20, 0.38, reliefLip) * (1.0 - smoothstep(0.38, 0.62, reliefLip))
	* (1.0 - reliefSlope) * (1.0 - reliefRock) * (1.0 - reliefFoot) * (1.0 - reliefDamp);
diffuseColor *= float3(1.0, 1.0, 1.0) + float3(0.06, 0.05, 0.035) * reliefCrest;
if (reliefRock > 0.001)
{
	float reliefEdgeNoise = 0.5;
	float reliefDetail = 1.0;
	float reliefPatchTone = 0.5;
	if (Low < 0.5)
	{
		float3 reliefStonePoint = Pos * float3(0.42, 0.55, 0.42);
		float reliefWeather = R.Noise(reliefStonePoint);
		float reliefGrain = R.Noise(Pos * 3.1);
		float reliefFine = R.Noise(Pos * 13.0);
		reliefPatchTone = smoothstep(0.25, 0.75, reliefWeather);
		float reliefPockets = 1.0 - smoothstep(0.24, 0.38, reliefGrain + (reliefWeather - 0.5) * 0.30);
		reliefDetail = (0.91 + 0.18 * reliefFine) * (1.0 - 0.24 * reliefPockets);
		reliefEdgeNoise = reliefWeather * 0.65 + reliefGrain * 0.35;
	}
	float3 reliefStone = lerp(float3(0.08, 0.085, 0.075), float3(0.19, 0.16, 0.115), reliefPatchTone) * reliefDetail;
	float reliefLipWidth = 0.30 + (reliefEdgeNoise - 0.5) * 0.16;
	float reliefFootWidth = 0.40 + (reliefEdgeNoise - 0.5) * 0.16;
	float reliefWallLip = 1.0 - smoothstep(reliefLipWidth - 0.05, reliefLipWidth + 0.02, Paint.y - Pos.y);
	float reliefWallFoot = 1.0 - smoothstep(reliefFootWidth - 0.10, reliefFootWidth + 0.03, Pos.y - Paint.z);
	float reliefWallDamp = max(reliefDamp, 1.0 - smoothstep(-2.5, 0.75, Paint.z));
	float3 reliefWallSoil = lerp(float3(0.16, 0.115, 0.065), float3(0.14, 0.14, 0.075), reliefWallDamp);
	reliefStone = lerp(reliefStone, reliefWallSoil * (0.93 + 0.14 * reliefEdgeNoise), reliefWallFoot * 0.90);
	reliefStone = lerp(reliefStone, float3(0.43, 0.39, 0.30), reliefWallLip * 0.35);
	float reliefLedge = smoothstep(0.55, 0.85, reliefN.y) * (1.0 - reliefWallFoot);
	reliefStone = lerp(reliefStone, diffuseColor * 0.82, reliefLedge * 0.8);
	diffuseColor = lerp(diffuseColor, reliefStone, reliefRock);
}
else
{
	float reliefRim = smoothstep(0.38, 0.62, reliefLip);
	diffuseColor = lerp(diffuseColor, float3(0.43, 0.39, 0.30), reliefRim * 0.35);
	diffuseColor = lerp(diffuseColor, reliefSoil, reliefFoot * 1.3);
	diffuseColor *= 1.0 - 0.20 * smoothstep(0.0, 0.35, reliefFoot);
}
return diffuseColor;
'''

# FRAG_FILL (the factor: totalEmissiveRadiance += diffuseColor.rgb * factor)
FILL = HELPERS + r'''
float reliefWallFill = 0.30 * reliefRock * smoothstep(0.35, 1.0, reliefSteep);
float reliefRampFill = 0.22 * (1.0 - reliefRock) * reliefSlope * (1.0 - clamp(Scar, 0.0, 1.0));
return reliefWallFill + reliefRampFill;
'''

# FRAG_NORMAL (world-space normal, Unreal axes)
NORMAL = HELPERS + r'''
float3 n = reliefN;
if (Scar > 0.5)
{
	n = normalize(lerp(n, float3(0.0, 1.0, 0.0), 0.93));
	reliefN = normalize(lerp(reliefN, float3(0.0, 1.0, 0.0), 0.93));
}
if (Low < 0.5)
{
	float3 reliefBump = float3(0.054 * cos(Pos.x * 0.80 + Pos.z * 0.36), 0.0, 0.048 * sin(Pos.z * 0.74 - Pos.x * 0.23));
	reliefBump *= smoothstep(0.3, 0.9, abs(reliefN.y)) * (1.0 - reliefRock);
	reliefBump -= reliefN * dot(reliefN, reliefBump);
	n = normalize(n + reliefBump);
}
return float3(n.x, n.z, n.y);
'''


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


def main():
    path = PATH + '/' + NAME
    if eal.does_asset_exist(path):
        mat = eal.load_asset(path)
    else:
        mat = tools.create_asset(NAME, PATH, unreal.Material, unreal.MaterialFactoryNew())
    mel.delete_all_material_expressions(mat)
    mat.set_editor_property('tangent_space_normal', False)  # the normal node returns a world-space normal
    vc = expr(mat, unreal.MaterialExpressionVertexColor, -1200, -300)
    wp = expr(mat, unreal.MaterialExpressionWorldPosition, -1200, -150)
    vn = expr(mat, unreal.MaterialExpressionVertexNormalWS, -1200, 0)
    uv1 = expr(mat, unreal.MaterialExpressionTextureCoordinate, -1200, 150, coordinate_index=1)
    uv2 = expr(mat, unreal.MaterialExpressionTextureCoordinate, -1200, 250, coordinate_index=2)
    uv3 = expr(mat, unreal.MaterialExpressionTextureCoordinate, -1200, 350, coordinate_index=3)
    scar = expr(mat, unreal.MaterialExpressionComponentMask, -1000, 350, r=True, g=False, b=False, a=False)
    mel.connect_material_expressions(uv3, '', scar, '')
    low = expr(mat, unreal.MaterialExpressionScalarParameter, -1200, 470, parameter_name='Low', default_value=0.0)
    common = [('WP', wp, ''), ('VN', vn, ''), ('RL', uv1, ''), ('FD', uv2, ''), ('Scar', scar, ''), ('Low', low, '')]
    # slice 13 (paint): diffuseColor is the painted ground (/Game/Terrain/Materials/MF_TCGroundPaint, its Paint output);
    # without that function the vertex colour stands in, as before
    ground = eal.load_asset('/Game/Terrain/Materials/MF_TCGroundPaint')
    call = None
    if ground:
        call = expr(mat, unreal.MaterialExpressionMaterialFunctionCall, -1500, -300)
        call.set_material_function(ground)
    paint = custom(mat, PAINT, [('Base', call, 'Paint') if call else ('Base', vc, '')] + common, T.CMOT_FLOAT3, -700, -200, 'TCReliefPaint')
    fill = custom(mat, FILL, common, T.CMOT_FLOAT1, -700, 100, 'TCReliefFill')
    normal = custom(mat, NORMAL, common, T.CMOT_FLOAT3, -700, 300, 'TCReliefNormal')
    emissive = expr(mat, unreal.MaterialExpressionMultiply, -400, 50)
    mel.connect_material_expressions(paint, '', emissive, 'A')
    mel.connect_material_expressions(fill, '', emissive, 'B')
    if call:
        # the fog of war over all of it, as the browser's overlay sits over the lit ground (MF_TCGroundPaint FogDim)
        fogged = expr(mat, unreal.MaterialExpressionMultiply, -250, -200)
        mel.connect_material_expressions(paint, '', fogged, 'A')
        mel.connect_material_expressions(call, 'FogDim', fogged, 'B')
        fogged_fill = expr(mat, unreal.MaterialExpressionMultiply, -250, 50)
        mel.connect_material_expressions(emissive, '', fogged_fill, 'A')
        mel.connect_material_expressions(call, 'FogDim', fogged_fill, 'B')
        mel.connect_material_property(fogged, '', P.MP_BASE_COLOR)
        mel.connect_material_property(fogged_fill, '', P.MP_EMISSIVE_COLOR)
    else:
        mel.connect_material_property(paint, '', P.MP_BASE_COLOR)
        mel.connect_material_property(emissive, '', P.MP_EMISSIVE_COLOR)
    mel.connect_material_property(normal, '', P.MP_NORMAL)
    mel.connect_material_property(expr(mat, unreal.MaterialExpressionConstant, -400, 200, r=0.0), '', P.MP_SPECULAR)
    mel.connect_material_property(expr(mat, unreal.MaterialExpressionConstant, -400, 260, r=1.0), '', P.MP_ROUGHNESS)
    mel.recompile_material(mat)
    eal.save_loaded_asset(mat, only_if_is_dirty=False)
    here = os.path.dirname(os.path.abspath(__file__))
    ref = os.environ.get('TC_REFERENCE_RELIEF_MATERIAL') or os.path.normpath(os.path.join(here, '..', '..', '..', 'reference', 'ww2-rts', 'client', 'relief-material.js'))
    sha = lambda b: hashlib.sha256(b).hexdigest()
    prov = {'asset': path, 'version': VERSION, 'reference': 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d',
            'source': 'client/relief-material.js', 'sourceSha256': sha(open(ref, 'rb').read()) if os.path.exists(ref) else None,
            'hlslSha256': sha((PAINT + FILL + NORMAL).encode()), 'tool': 'unreal/Tools/relief/make-relief-material.py'}
    out = os.path.join(unreal.Paths.convert_relative_path_to_full(unreal.Paths.project_content_dir()), 'Relief', 'ReliefMaterial.json')
    os.makedirs(os.path.dirname(out), exist_ok=True)
    with open(out, 'w') as f:
        json.dump(prov, f, indent=1)
    unreal.log('[relief] wrote %s and %s' % (path, out))


main()
