"""The light and color assets of the Unreal client (README.md), generated from the pinned reference and its three.js.

Run in the editor (UnrealEditor-Cmd <project> -run=pythonscript -script="make-look-assets.py --look <dir> --three <dir>")
after unreal/Tools/models/import-models.py (which rewrites the model masters this patches). Idempotent.
  --only M_TCLookRing[,...]   make only those simple materials (no --look or --three needed) and record them in the
                              existing LookProvenance.json; the other assets are left as they are. M_TCLookPost may be
                              named too: its defaults come from the light row LookProvenance.json recorded.

  Content/Models/Materials/Look/
    T_TCLookEnv          three's own PMREM atlas of client/light.js outdoorEnvironment (look.mjs reads it back texel
                         for texel), float, no mips, bilinear, clamped: the texels three's textureCubeUV samples
    MPC_TCLook           HemiTop, HemiBottom (hemisphere light colors times intensity, linear), Env (x environment
                         intensity, y environment yaw in radians, z 1 = the model masters add the reference's indirect light;
                         0 by default, so the masters look as imported until ATCLighting turns it on)
    M_TCLookPost         post process, replacing the tonemapper: three r186 ACESFilmicToneMapping with the mood's
                         exposure, the sRGB output transfer, the distance haze and the haze as background where nothing
                         was drawn (WebGLBackground's clear color). HazeBeforeTone 1 (Graphics High, light.js's
                         composer): the linear haze (HazeLinear) mixes before the tone curve; 0 (Low, the direct path):
                         the haze in output space (Haze) mixes after it, as fog_fragment does on the canvas
                         Neutral 1 (round 16): three's NeutralToneMapping in place of ACES filmic (the lobby backdrop)
    M_TCLookEmissive     unlit, Color: flat patches for the tone curve calibration
    M_TCLookLambert      lit, Color, specular 0, roughness 1: a MeshLambertMaterial (the comparison ground)
    M_TCLookLine         unlit translucent, Color, Opacity: three's line helpers (the comparison grid)
    M_TCGroundLambert    lit, vertex color, specular 0, roughness 1: a MeshLambertMaterial with vertex colors
    M_TCLookRing         unlit translucent, two-sided: client/overlay.js makeOverlay for the unit rings (markers.js
                         ownerRing, selectionRing). Its vertex half keeps a line's half-width in metres but never under
                         its pixel count (world position offset along the ribbon's edge direction); its pixel half fades
                         the band's edge over one pixel, draws the thin dark edge outside the core and the fill inside a
                         ring. Per vertex (the client's run-time ring mesh, Client/TCUnitModels.cpp): UV0 (side -1..1
                         across the band, -3 at the fill's hub; where the dark edge starts), UV1 the edge direction (unit
                         vector, local x y), UV2 (the half-width the mesh is built at in cm, the minimum half-width in
                         pixels). Parameters Color, Rim (linear), Opacity, Fill.
  Content/Models/Materials/M_TCModel{,TwoSided,Faded,FadedTwoSided}: one Custom node 'TCLookIndirect' added after
  'TCModel' (its emissive goes through it) and Ambient Occlusion 0, so the engine's sky light leaves these pixels and the
  node adds what three's MeshStandardMaterial and MeshLambertMaterial gather instead (README.md, "Indirect light").
  Content/Models/Materials/LookProvenance.json: sources, hashes and the generated HLSL's hash.
"""
import argparse
import hashlib
import json
import math
import os
import re
import struct
import sys
import zlib

import unreal

ROOT = '/Game/Models/Materials'
LOOK = ROOT + '/Look'
MASTERS = ('M_TCModel', 'M_TCModelTwoSided', 'M_TCModelFaded', 'M_TCModelFadedTwoSided')
ENV_SCALE = 4.0  # the atlas is stored as 16-bit PNG of value / ENV_SCALE (its largest texel is under 2)
MARK = 'TCLookIndirect'

tools = unreal.AssetToolsHelpers.get_asset_tools()
eal = unreal.EditorAssetLibrary
mel = unreal.MaterialEditingLibrary
P = unreal.MaterialProperty
T = unreal.CustomMaterialOutputType


def log(msg):
    unreal.log('[look] ' + msg)
    print('[look] ' + msg)


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def srgb_to_linear(c):
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def hex_linear(h):
    return [srgb_to_linear(((h >> s) & 255) / 255.0) for s in (16, 8, 0)]


# ---------------------------------------------------------------------------------------------------- inputs

def half_to_float(h):
    return struct.unpack('<e', struct.pack('<H', h))[0]


def dfg_table(three):
    """three r186 src/renderers/shaders/DFGLUTData.js: 16 x 16 RG half floats (scale, bias), row = dot(N, V)."""
    path = os.path.join(three, 'node_modules/three/src/renderers/shaders/DFGLUTData.js')
    text = open(path, encoding='utf-8').read()
    body = text[text.index('new Uint16Array( [') + len('new Uint16Array( ['):]
    body = body[:body.index(']')]
    values = [half_to_float(int(v, 16)) for v in re.findall(r'0x[0-9a-fA-F]+', body)]
    if len(values) != 512:
        raise RuntimeError('DFG LUT: expected 512 halves, found %d' % len(values))
    return values, path


def png16(path, width, height, rows):
    """A 16-bit RGBA PNG; rows: lists of width * 4 ints 0..65535, first row at the top."""
    raw = bytearray()
    for row in rows:
        raw.append(0)
        raw.extend(struct.pack('>%dH' % len(row), *row))

    def chunk(tag, data):
        c = struct.pack('>I', len(data)) + tag + data
        return c + struct.pack('>I', zlib.crc32(tag + data) & 0xffffffff)
    data = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 16, 6, 0, 0, 0))
    data += chunk(b'IDAT', zlib.compress(bytes(raw), 9)) + chunk(b'IEND', b'')
    with open(path, 'wb') as f:
        f.write(data)


def env_png(look_dir, work):
    meta = json.load(open(os.path.join(look_dir, 'cameras.json'), encoding='utf-8'))['env']
    raw = open(os.path.join(look_dir, 'env-pmrem.f32'), 'rb').read()
    w, h = meta['width'], meta['height']
    px = struct.unpack('<%df' % (w * h * 4), raw)
    peak = max(px[i] for i in range(len(px)) if i % 4 != 3)
    if peak >= ENV_SCALE:
        raise RuntimeError('environment texel %g does not fit the %g scale' % (peak, ENV_SCALE))
    rows = []
    for y in range(h):  # row 0 of the read-back is GL v = 0; it becomes the top row, Unreal's v = 0
        row = []
        for x in range(w):
            i = (y * w + x) * 4
            row.extend(int(round(max(0.0, px[i + c]) / ENV_SCALE * 65535)) for c in range(3))
            row.append(65535)
        rows.append(row)
    os.makedirs(work, exist_ok=True)
    out = os.path.join(work, 'T_TCLookEnv.png')
    png16(out, w, h, rows)
    return out, meta, peak, sha256(raw)


# ---------------------------------------------------------------------------------------------------- HLSL

def cube_hlsl(meta):
    """client three r186 cube_uv_reflection_fragment.glsl.js, as HLSL (a struct, so it can live in a Custom node)."""
    return '''struct TCLookCube {
	float Face(float3 d) {
		float3 a = abs(d);
		if (a.x > a.z) return a.x > a.y ? (d.x > 0.0 ? 0.0 : 3.0) : (d.y > 0.0 ? 1.0 : 4.0);
		return a.z > a.y ? (d.z > 0.0 ? 2.0 : 5.0) : (d.y > 0.0 ? 1.0 : 4.0);
	}
	float2 FaceUV(float3 d, float face) {
		float2 uv;
		if (face == 0.0) uv = float2(d.z, d.y) / abs(d.x);
		else if (face == 1.0) uv = float2(-d.x, -d.z) / abs(d.y);
		else if (face == 2.0) uv = float2(-d.x, d.y) / abs(d.z);
		else if (face == 3.0) uv = float2(-d.z, d.y) / abs(d.x);
		else if (face == 4.0) uv = float2(-d.x, d.z) / abs(d.y);
		else uv = float2(d.x, d.y) / abs(d.z);
		return 0.5 * (uv + 1.0);
	}
	float3 Bilinear(Texture2D T, SamplerState S, float3 d, float mipInt) {
		float face = Face(d);
		float filterInt = max(4.0 - mipInt, 0.0);
		mipInt = max(mipInt, 4.0);
		float faceSize = exp2(mipInt);
		float2 uv = FaceUV(d, face) * (faceSize - 2.0) + 1.0;
		if (face > 2.0) { uv.y += faceSize; face -= 3.0; }
		uv.x += face * faceSize;
		uv.x += filterInt * 3.0 * 16.0;
		uv.y += 4.0 * (exp2(%(maxmip)s) - faceSize);
		uv.x *= %(tw)s;
		uv.y *= %(th)s;
		return T.SampleLevel(S, uv, 0.0).rgb * %(scale)s;
	}
	float Mip(float r) {
		if (r >= 0.8) return (1.0 - r) * (-1.0 - -2.0) / (1.0 - 0.8) + -2.0;
		if (r >= 0.4) return (0.8 - r) * (2.0 - -1.0) / (0.8 - 0.4) + -1.0;
		if (r >= 0.305) return (0.4 - r) * (3.0 - 2.0) / (0.4 - 0.305) + 2.0;
		if (r >= 0.21) return (0.305 - r) * (4.0 - 3.0) / (0.305 - 0.21) + 3.0;
		return -2.0 * log2(1.16 * r);
	}
	float3 Sample(Texture2D T, SamplerState S, float3 d, float r) {
		float mip = clamp(Mip(r), -2.0, %(maxmip)s);
		float mipF = frac(mip), mipInt = floor(mip);
		float3 c0 = Bilinear(T, S, d, mipInt);
		if (mipF == 0.0) return c0;
		return lerp(c0, Bilinear(T, S, d, mipInt + 1.0), mipF);
	}
};
''' % dict(maxmip='%.1f' % meta['maxMip'], tw='%.12g' % meta['texelWidth'], th='%.12g' % meta['texelHeight'], scale='%.1f' % ENV_SCALE)


def indirect_hlsl(meta, dfg):
    table = ', '.join('float2(%.9g, %.9g)' % (dfg[2 * i], dfg[2 * i + 1]) for i in range(256))
    return '''// Generated by unreal/Tools/look/make-look-assets.py from three r186 (lights_physical_pars_fragment,
// lights_lambert_pars_fragment, lights_fragment_begin/maps/end, envmap_physical_pars_fragment, cube_uv_reflection_fragment,
// DFGLUTData) and client/light.js. Edit the generator, not this.
// The light three gathers besides the sun, which the engine draws with its own directional light: the hemisphere light
// (HemiTop, HemiBottom) and the PMREM environment (Env: its atlas, Look.x its intensity, Look.y its yaw). Lambert
// (Specular 0: Lambert materials, the wreck) takes both as diffuse; Standard (Specular 0.5) adds the environment's
// reflection and three's energy terms (Fdez-Aguera multiple scattering, dielectric and metal mixed by metalness).
// Ambient Occlusion is 0 on these masters, so the engine's sky light adds nothing here.
%(cube)s
static const float2 DFG[256] = { %(table)s };
struct TCLookDFG {
	float2 At(float r, float nv) {
		float2 p = float2(saturate(r), saturate(nv)) * 16.0 - 0.5;
		float2 f = frac(p), i0 = floor(p);
		int x0 = (int)clamp(i0.x, 0.0, 15.0), x1 = (int)clamp(i0.x + 1.0, 0.0, 15.0);
		int y0 = (int)clamp(i0.y, 0.0, 15.0), y1 = (int)clamp(i0.y + 1.0, 0.0, 15.0);
		return lerp(lerp(DFG[y0 * 16 + x0], DFG[y0 * 16 + x1], f.x), lerp(DFG[y1 * 16 + x0], DFG[y1 * 16 + x1], f.x), f.y);
	}
};
TCLookCube cube;
TCLookDFG lut;
if (Look.z < 0.5) return Emissive;
// three's axes: x, y up, z (Unreal X, Z, Y); the environment turned by -yaw about y (WebGLMaterials envMapRotation)
float3 wn = normalize(NormalWS), wv = normalize(CameraVector);
float3 N = float3(wn.x, wn.z, wn.y), V = float3(wv.x, wv.z, wv.y);
float ec = cos(-Look.y), es = sin(-Look.y);
float3 Ne = float3(ec * N.x + es * N.z, N.y, -es * N.x + ec * N.z);
float3 hemi = lerp(HemiBottom.rgb, HemiTop.rgb, 0.5 * N.y + 0.5);           // getHemisphereLightIrradiance
float3 envIrr = cube.Sample(Env, EnvSampler, Ne, 1.0) * Look.x;             // getIBLIrradiance / PI
float3 albedo = max(Color, 0.0);
float3 lambert = albedo * (hemi / PI + envIrr);                              // RE_IndirectDiffuse_Lambert
float standard = saturate(Specular / 0.5);
float3 indirect = lambert;
if (standard > 0.0) {
	float3 vn = normalize(VertexNormal);
	float3 dxy = max(abs(ddx(vn)), abs(ddy(vn)));
	float r = min(max(Roughness, 0.0525) + max(max(dxy.x, dxy.y), dxy.z), 1.0);
	float nv = saturate(dot(N, V));
	float2 fab = lut.At(r, nv);
	float Ess = fab.x + fab.y, Ems = 1.0 - Ess;
	float3 F0 = float3(0.04, 0.04, 0.04);
	float3 ssD = F0 * fab.x + fab.y;
	float3 avD = F0 + (1.0 - F0) * 0.047619;
	float3 msD = ssD * avD / (1.0 - Ems * avD) * Ems;
	float3 ssM = albedo * fab.x + fab.y;
	float3 avM = albedo + (1.0 - albedo) * 0.047619;
	float3 msM = ssM * avM / (1.0 - Ems * avM) * Ems;
	float m = saturate(Metallic);
	float3 single = lerp(ssD, ssM, m), multi = lerp(msD, msM, m);
	float3 keep = 1.0 - (ssD + msD);
	float3 R = reflect(-V, N);
	float r2 = r * r;
	R = normalize(lerp(R, N, r2 * r2));
	float3 Re = float3(ec * R.x + es * R.z, R.y, -es * R.x + ec * R.z);
	float3 radiance = cube.Sample(Env, EnvSampler, Re, r) * Look.x;           // getIBLRadiance
	float3 diffuse = albedo * (1.0 - m) * keep * (hemi / PI + envIrr);      // RE_IndirectDiffuse_Physical + RE_IndirectSpecular_Physical
	float3 standardLight = diffuse + radiance * single + multi * envIrr;
	indirect = lerp(lambert, standardLight, standard);
}
return Emissive + indirect;
''' % dict(cube=cube_hlsl(meta), table=table)


POST_HLSL = '''// Generated by unreal/Tools/look/make-look-assets.py. three r186: ACESFilmicToneMapping (tonemapping_pars_fragment),
// sRGBTransferOETF (colorspace_pars_fragment), fog_fragment's mix with the haze and the clear color (scene.background,
// the mood's haze) where nothing was drawn. Where the haze mixes is the browser's graphics level (round 14): on High
// light.js draws through its composer into a linear half-float target, where three gives the materials no tone mapping
// and a linear output and the fog colour is linear, so the haze mixes in linear light and OutputPass tone-maps it with
// the rest (HazeBeforeTone 1, HazeLinear); on Low the scene draws straight to the canvas and the haze (its sRGB
// bytes, Haze) mixes after the tone curve (HazeBeforeTone 0). Exposure is the mood's toneMappingExposure; the
// engine's own exposure is locked at 1 (manual, bias 0), and its pre-exposure is taken off here.
// Neutral 1 (round 16): three's NeutralToneMapping instead of ACES filmic (client/lobby-view.js, the lobby backdrop).
float3 c = max(Scene.rgb, 0.0) * View.OneOverPreExposure;
float depth = Depth * 0.01; // metres along the view axis (three's vFogDepth)
float fog = FogFar > FogNear ? smoothstep(FogNear, FogFar, depth) : 0.0;
if (HazeBeforeTone > 0.5) c = depth > SkyMetres ? HazeLinear.rgb : lerp(c, HazeLinear.rgb, fog);
if (Neutral > 0.5)
{
	// round 16 (hudfit): three r186 NeutralToneMapping, client/lobby-view.js's renderer (the lobby backdrop)
	c *= Exposure;
	float x = min(c.r, min(c.g, c.b));
	float offset = x < 0.08 ? x - 6.25 * x * x : 0.04;
	c -= offset;
	float peak = max(c.r, max(c.g, c.b));
	if (peak >= 0.76)
	{
		float dd = 1.0 - 0.76;
		float newPeak = 1.0 - dd * dd / (peak + dd - 0.76);
		c *= newPeak / peak;
		float g = 1.0 - 1.0 / (0.15 * (peak - newPeak) + 1.0);
		c = lerp(c, float3(newPeak, newPeak, newPeak), g);
	}
}
else
{
	c *= Exposure / 0.6;
	c = float3(dot(float3(0.59719, 0.35458, 0.04823), c), dot(float3(0.07600, 0.90834, 0.01566), c), dot(float3(0.02840, 0.13383, 0.83777), c));
	float3 a = c * (c + 0.0245786) - 0.000090537;
	float3 b = c * (0.983729 * c + 0.4329510) + 0.238081;
	c = a / b;
	c = float3(dot(float3(1.60475, -0.53108, -0.07367), c), dot(float3(-0.10208, 1.10813, -0.00605), c), dot(float3(-0.00327, -0.07276, 1.07602), c));
}
c = saturate(c);
float3 lo = c * 12.92, hi = pow(max(c, 1e-8), 0.41666) * 1.055 - 0.055;
float3 o = float3(c.r <= 0.0031308 ? lo.r : hi.r, c.g <= 0.0031308 ? lo.g : hi.g, c.b <= 0.0031308 ? lo.b : hi.b);
float3 d = HazeBeforeTone > 0.5 ? o : (depth > SkyMetres ? Haze.rgb : lerp(o, Haze.rgb, fog));
// OutputLinear 1: a target that sRGB-encodes in hardware gets the linear value of the same display color
if (OutputLinear > 0.5) d = float3(d.r <= 0.04045 ? d.r * 0.0773993808 : pow(d.r * 0.9478672986 + 0.0521327014, 2.4), d.g <= 0.04045 ? d.g * 0.0773993808 : pow(d.g * 0.9478672986 + 0.0521327014, 2.4), d.b <= 0.04045 ? d.b * 0.0773993808 : pow(d.b * 0.9478672986 + 0.0521327014, 2.4));
return d;
'''


# ---------------------------------------------------------------------------------------------------- assets

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


def connect_prop(src, out, prop):
    if not mel.connect_material_property(src, out, prop):
        raise RuntimeError('could not connect %s' % prop)


def save(asset):
    eal.save_loaded_asset(asset, only_if_is_dirty=False)


def enum(owner, *names):
    for n in names:
        if hasattr(owner, n):
            return getattr(owner, n)
    raise RuntimeError('none of %s on %s' % (names, owner))


def env_texture(png):
    task = unreal.AssetImportTask()
    task.set_editor_property('filename', png)
    task.set_editor_property('destination_path', LOOK)
    task.set_editor_property('destination_name', 'T_TCLookEnv')
    task.set_editor_property('automated', True)
    task.set_editor_property('replace_existing', True)
    task.set_editor_property('save', False)
    tools.import_asset_tasks([task])
    tex = eal.load_asset(LOOK + '/T_TCLookEnv')
    if not tex:
        raise RuntimeError('import failed: ' + png)
    tex.set_editor_property('srgb', False)
    tex.set_editor_property('compression_settings', enum(unreal.TextureCompressionSettings, 'TC_HDR_F32', 'TC_HDR'))
    tex.set_editor_property('mip_gen_settings', unreal.TextureMipGenSettings.TMGS_NO_MIPMAPS)
    tex.set_editor_property('filter', unreal.TextureFilter.TF_BILINEAR)
    tex.set_editor_property('address_x', unreal.TextureAddress.TA_CLAMP)
    tex.set_editor_property('address_y', unreal.TextureAddress.TA_CLAMP)
    tex.set_editor_property('never_stream', True)
    tex.set_editor_property('lod_group', unreal.TextureGroup.TEXTUREGROUP_PIXELS2D)
    save(tex)
    return tex


def collection(row):
    mpc = fresh(LOOK + '/MPC_TCLook', unreal.MaterialParameterCollection, unreal.MaterialParameterCollectionFactoryNew())
    k = row['hemiI']
    vectors = []
    for name, value in (('HemiTop', [c * k for c in hex_linear(row['top'])] + [1.0]), ('HemiBottom', [c * k for c in hex_linear(row['bottom'])] + [1.0]),
                        # z 0 by default: the masters' node is inert until ATCLighting (or TCLookCompare) turns it on, which it
                        # does only without static lighting (TCLook::IndirectInMaterials)
                        ('Env', [0.38 * (0.7 + 0.3 * min(row['sunI'] / 3.2, 1.0)), 0.0, 0.0, 0.0])):
        p = unreal.CollectionVectorParameter()
        p.set_editor_property('parameter_name', name)
        p.set_editor_property('default_value', unreal.LinearColor(*value))
        vectors.append(p)
    mpc.set_editor_property('vector_parameters', vectors)
    save(mpc)
    return mpc


def new_material(name):
    mat = fresh(LOOK + '/' + name, unreal.Material, unreal.MaterialFactoryNew())
    mel.delete_all_material_expressions(mat)
    return mat


def post_material(row):
    mat = new_material('M_TCLookPost')
    mat.set_editor_property('material_domain', unreal.MaterialDomain.MD_POST_PROCESS)
    mat.set_editor_property('shading_model', unreal.MaterialShadingModel.MSM_UNLIT)  # the engine refuses lit post process
    mat.set_editor_property('blendable_location', enum(unreal.BlendableLocation, 'BL_REPLACING_TONEMAPPER', 'BL_ReplacingTonemapper'))
    scene = expr(mat, unreal.MaterialExpressionSceneTexture, -900, -200, scene_texture_id=unreal.SceneTextureId.PPI_POST_PROCESS_INPUT0)
    depth = expr(mat, unreal.MaterialExpressionSceneTexture, -900, 0, scene_texture_id=unreal.SceneTextureId.PPI_SCENE_DEPTH)
    haze = [((row['haze'] >> s) & 255) / 255.0 for s in (16, 8, 0)]
    ins = [('Scene', scene, 'Color'), ('Depth', depth, 'Color'),
           ('Exposure', expr(mat, unreal.MaterialExpressionScalarParameter, -900, 200, parameter_name='Exposure', default_value=row['exposure']), ''),
           ('FogNear', expr(mat, unreal.MaterialExpressionScalarParameter, -900, 300, parameter_name='FogNear', default_value=1e4), ''),
           ('FogFar', expr(mat, unreal.MaterialExpressionScalarParameter, -900, 400, parameter_name='FogFar', default_value=2e4), ''),
           ('SkyMetres', expr(mat, unreal.MaterialExpressionScalarParameter, -900, 500, parameter_name='SkyMetres', default_value=1e5), ''),
           ('OutputLinear', expr(mat, unreal.MaterialExpressionScalarParameter, -900, 550, parameter_name='OutputLinear', default_value=0.0), ''),
           ('Haze', expr(mat, unreal.MaterialExpressionVectorParameter, -900, 600, parameter_name='Haze', default_value=unreal.LinearColor(haze[0], haze[1], haze[2], 1.0)), ''),
           # round 14: Graphics High's order (the composer): the linear haze, mixed before the tone curve (ATCLighting sets both)
           ('HazeBeforeTone', expr(mat, unreal.MaterialExpressionScalarParameter, -900, 700, parameter_name='HazeBeforeTone', default_value=0.0), ''),
           # round 16 (hudfit): 1 = three's NeutralToneMapping (the lobby backdrop, client/lobby-view.js); 0 = ACES filmic (a match)
           ('Neutral', expr(mat, unreal.MaterialExpressionScalarParameter, -900, 900, parameter_name='Neutral', default_value=0.0), ''),
           ('HazeLinear', expr(mat, unreal.MaterialExpressionVectorParameter, -900, 800, parameter_name='HazeLinear', default_value=unreal.LinearColor(*[(v / 12.92 if v <= 0.04045 else ((v + 0.055) / 1.055) ** 2.4) for v in haze], 1.0)), '')]
    c = custom(mat, POST_HLSL, ins, T.CMOT_FLOAT3, -400, 0, 'TCLookPost')
    connect_prop(c, '', P.MP_EMISSIVE_COLOR)
    mel.recompile_material(mat)
    save(mat)
    return mat


RING_WPO_HLSL = '''// client/overlay.js VERT_BODY: float hw = max(aEdge.z, aEdge.w * depth * 2.0 / (projectionMatrix[1][1] * viewH)).
// The run-time mesh stands its edges at Edge.x (cm) from the line; this moves them out to the half-width in pixels
// when that is wider. ViewPos: the vertex in view space (cm), Fov: the view's field of view (radians, x horizontal,
// y vertical), ViewSize: the view in pixels.
float depth = max(abs(ViewPos.z), 1.0);
float pxcm = depth * 2.0 * tan(Fov.y * 0.5) / max(ViewSize.y, 1.0);
float hw = max(Edge.x, Edge.y * pxcm);
return float3(Dir.x * (hw - Edge.x), Dir.y * (hw - Edge.x), 0.0);
'''

RING_HLSL = '''// client/overlay.js FRAG_BODY and FRAG_END: Line.x is the side across the band (-1 to 1; the fill's hub -3), Line.y
// where the dark edge starts. The edge fades over one pixel (fwidth), the dark rim (0.012, 0.016, 0.014) shows outside
// the core at 0.55 of the opacity, the inside of a filled ring takes Fill.
float lv = abs(Line.x);
float aa = max(fwidth(Line.x), 1e-3);
float band = 1.0 - smoothstep(1.0 - aa, 1.0, lv);
float body = max(1.0 - smoothstep(Line.y - aa, Line.y, lv), step(1.0, lv));
float inside = step(1.0, -Line.x);
float3 rgb = lerp(Rim.rgb, Color.rgb, body);
return float4(rgb, saturate(Opacity * (band * lerp(0.55, 1.0, body) + inside * Fill)));
'''


def ring_material():
    m = new_material('M_TCLookRing')
    m.set_editor_property('shading_model', unreal.MaterialShadingModel.MSM_UNLIT)
    m.set_editor_property('blend_mode', unreal.BlendMode.BLEND_TRANSLUCENT)
    m.set_editor_property('two_sided', True)
    X = -1200
    uv = [expr(m, unreal.MaterialExpressionTextureCoordinate, X, 100 * i, coordinate_index=i) for i in range(3)]
    world = expr(m, unreal.MaterialExpressionWorldPosition, X - 300, 400,
                 world_position_shader_offset=enum(unreal.WorldPositionIncludedOffsets, 'WPT_EXCLUDE_ALL_SHADER_OFFSETS'))
    view_pos = expr(m, unreal.MaterialExpressionTransformPosition, X, 400)
    view_pos.set_editor_property('transform_source_type', enum(unreal.MaterialPositionTransformSource, 'TRANSFORMPOSSOURCE_WORLD'))
    view_pos.set_editor_property('transform_type', enum(unreal.MaterialPositionTransformSource, 'TRANSFORMPOSSOURCE_VIEW'))
    mel.connect_material_expressions(world, '', view_pos, '')
    fov = expr(m, unreal.MaterialExpressionViewProperty, X, 500, property=enum(unreal.MaterialExposedViewProperty, 'MEVP_FIELD_OF_VIEW'))
    size = expr(m, unreal.MaterialExpressionViewProperty, X, 600, property=enum(unreal.MaterialExposedViewProperty, 'MEVP_VIEW_SIZE'))
    wpo = custom(m, RING_WPO_HLSL, [('ViewPos', view_pos, ''), ('Fov', fov, ''), ('ViewSize', size, ''), ('Dir', uv[1], ''), ('Edge', uv[2], '')], T.CMOT_FLOAT3, -700, 400, 'TCLookRingWidth')
    connect_prop(wpo, '', P.MP_WORLD_POSITION_OFFSET)
    ins = [('Line', uv[0], ''),
           ('Color', expr(m, unreal.MaterialExpressionVectorParameter, X, 800, parameter_name='Color', default_value=unreal.LinearColor(1, 1, 1, 1)), ''),
           ('Rim', expr(m, unreal.MaterialExpressionVectorParameter, X, 900, parameter_name='Rim', default_value=unreal.LinearColor(0.012, 0.016, 0.014, 1)), ''),
           ('Opacity', expr(m, unreal.MaterialExpressionScalarParameter, X, 1000, parameter_name='Opacity', default_value=0.9), ''),
           ('Fill', expr(m, unreal.MaterialExpressionScalarParameter, X, 1100, parameter_name='Fill', default_value=0.0), '')]
    look = custom(m, RING_HLSL, ins, T.CMOT_FLOAT4, -700, 0, 'TCLookRing')
    rgb = expr(m, unreal.MaterialExpressionComponentMask, -400, 0, r=True, g=True, b=True, a=False)
    alpha = expr(m, unreal.MaterialExpressionComponentMask, -400, 150, r=False, g=False, b=False, a=True)
    mel.connect_material_expressions(look, '', rgb, '')
    mel.connect_material_expressions(look, '', alpha, '')
    connect_prop(rgb, '', P.MP_EMISSIVE_COLOR)
    connect_prop(alpha, '', P.MP_OPACITY)
    return m


def simple_materials(only=None):
    made = {}
    if only:
        if 'M_TCLookRing' in only:
            made['M_TCLookRing'] = ring_material()
        unknown = [n for n in only if n not in made]
        if unknown:
            raise RuntimeError('--only: unknown materials %s' % unknown)
        return finish_simple(made)
    m = new_material('M_TCLookEmissive')
    m.set_editor_property('shading_model', unreal.MaterialShadingModel.MSM_UNLIT)
    connect_prop(expr(m, unreal.MaterialExpressionVectorParameter, -400, 0, parameter_name='Color', default_value=unreal.LinearColor(1, 1, 1, 1)), '', P.MP_EMISSIVE_COLOR)
    made['M_TCLookEmissive'] = m
    m = new_material('M_TCLookLambert')
    connect_prop(expr(m, unreal.MaterialExpressionVectorParameter, -400, 0, parameter_name='Color', default_value=unreal.LinearColor(0.5, 0.5, 0.5, 1)), '', P.MP_BASE_COLOR)
    connect_prop(expr(m, unreal.MaterialExpressionConstant, -400, 150, r=0.0), '', P.MP_SPECULAR)
    connect_prop(expr(m, unreal.MaterialExpressionConstant, -400, 250, r=1.0), '', P.MP_ROUGHNESS)
    made['M_TCLookLambert'] = m
    m = new_material('M_TCGroundLambert')
    vc = expr(m, unreal.MaterialExpressionVertexColor, -500, 0)
    connect_prop(vc, '', P.MP_BASE_COLOR)
    connect_prop(expr(m, unreal.MaterialExpressionConstant, -400, 150, r=0.0), '', P.MP_SPECULAR)
    connect_prop(expr(m, unreal.MaterialExpressionConstant, -400, 250, r=1.0), '', P.MP_ROUGHNESS)
    made['M_TCGroundLambert'] = m
    m = new_material('M_TCLookLine')
    m.set_editor_property('shading_model', unreal.MaterialShadingModel.MSM_UNLIT)
    m.set_editor_property('blend_mode', unreal.BlendMode.BLEND_TRANSLUCENT)
    connect_prop(expr(m, unreal.MaterialExpressionVectorParameter, -400, 0, parameter_name='Color', default_value=unreal.LinearColor(1, 1, 1, 1)), '', P.MP_EMISSIVE_COLOR)
    connect_prop(expr(m, unreal.MaterialExpressionScalarParameter, -400, 150, parameter_name='Opacity', default_value=0.45), '', P.MP_OPACITY)
    made['M_TCLookLine'] = m
    made['M_TCLookRing'] = ring_material()
    return finish_simple(made)


def finish_simple(made):
    for name, m in made.items():
        for usage in (unreal.MaterialUsage.MATUSAGE_INSTANCED_STATIC_MESHES,):
            if hasattr(mel, 'set_base_material_usage'):
                mel.set_base_material_usage(m, usage)
        mel.recompile_material(m)
        save(m)
    return made


def patch_master(name, code, mpc, env):
    path = ROOT + '/' + name
    mat = eal.load_asset(path)
    if not mat:
        raise RuntimeError('missing ' + path)
    model = mel.get_material_property_input_node(mat, P.MP_BASE_COLOR)
    if not model or model.get_editor_property('description') != 'TCModel':
        raise RuntimeError('%s: base color does not come from the TCModel node' % name)
    current = mel.get_material_property_input_node(mat, P.MP_EMISSIVE_COLOR)
    if current and current.get_editor_property('description') == MARK:
        # a previous run: drop its node and the nodes only it used
        for e in mel.get_inputs_for_material_expression(mat, current):
            if e and e != model and e.get_editor_property('desc') == MARK:
                mel.delete_material_expression(mat, e)
        mel.delete_material_expression(mat, current)
    elif current and current != model:
        raise RuntimeError('%s: emissive comes from %s, not TCModel' % (name, current.get_name()))
    X, Y = -350, 400
    vn = expr(mat, unreal.MaterialExpressionVertexNormalWS, X - 300, Y + 300, desc=MARK)
    cv = expr(mat, unreal.MaterialExpressionCameraVectorWS, X - 300, Y + 380, desc=MARK)
    env_obj = expr(mat, unreal.MaterialExpressionTextureObjectParameter, X - 300, Y + 460, parameter_name='LookEnv', texture=env, desc=MARK)
    params = [expr(mat, unreal.MaterialExpressionCollectionParameter, X - 300, Y + 560 + 80 * i, collection=mpc, parameter_name=n, desc=MARK) for i, n in enumerate(('HemiTop', 'HemiBottom', 'Env'))]
    inputs = [('Color', model, ''), ('Roughness', model, 'Roughness'), ('Metallic', model, 'Metallic'), ('Specular', model, 'Specular'),
              ('NormalWS', model, 'NormalWS'), ('Emissive', model, 'Emissive'), ('VertexNormal', vn, ''), ('CameraVector', cv, ''),
              ('Env', env_obj, ''), ('HemiTop', params[0], ''), ('HemiBottom', params[1], ''), ('Look', params[2], '')]
    node = custom(mat, code, inputs, T.CMOT_FLOAT3, X, Y, MARK)
    connect_prop(node, '', P.MP_EMISSIVE_COLOR)
    ao = mel.get_material_property_input_node(mat, P.MP_AMBIENT_OCCLUSION)
    if ao and ao.get_editor_property('desc') == MARK:
        mel.delete_material_expression(mat, ao)
    zero = expr(mat, unreal.MaterialExpressionConstant, X, Y - 120, r=0.0, desc=MARK)
    connect_prop(zero, '', P.MP_AMBIENT_OCCLUSION)
    mel.recompile_material(mat)
    save(mat)
    return mat


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--look', help='a look.mjs output folder (cameras.json, env-pmrem.f32)')
    ap.add_argument('--three', help='the three deps folder (node_modules/three 0.186.1)')
    ap.add_argument('--work')
    ap.add_argument('--skip-masters', action='store_true')
    ap.add_argument('--only', help='comma list of simple materials to make alone (M_TCLookRing)')
    a = ap.parse_args(sys.argv[1:] if len(sys.argv) > 1 else [])
    if a.only:
        only = [n.strip() for n in a.only.split(',') if n.strip()]
        out = os.path.join(unreal.Paths.convert_relative_path_to_full(unreal.Paths.project_content_dir()), 'Models/Materials/LookProvenance.json')
        prov = json.load(open(out, encoding='utf-8')) if os.path.exists(out) else dict(generator='unreal/Tools/look/make-look-assets.py', assets=[])
        if 'M_TCLookPost' in only:
            # round 14: the post material alone, its defaults from the light row the full run recorded
            row = (prov.get('light') or {}).get('row')
            if not row:
                raise SystemExit('--only M_TCLookPost needs LookProvenance.json from a full run (its light row)')
            post_material(row)
            prov.setdefault('hlsl', {})['post_sha256'] = sha256(POST_HLSL.encode())
            only = [n for n in only if n != 'M_TCLookPost']
            log('made M_TCLookPost')
        simple = simple_materials(only) if only else {}
        prov.setdefault('assets', [])
        for n in sorted(simple):
            if LOOK + '/' + n not in prov['assets']:
                prov['assets'].append(LOOK + '/' + n)
        prov.setdefault('hlsl', {})['ring_sha256'] = sha256((RING_WPO_HLSL + RING_HLSL).encode())
        with open(out, 'w', encoding='utf-8') as f:
            json.dump(prov, f, indent=1, sort_keys=True)
        log('done: %d simple materials (%s)' % (len(simple), ', '.join(sorted(simple))))
        return
    if not (a.look and a.three and a.work):
        raise SystemExit('--look, --three and --work are needed (or --only)')
    cams = json.load(open(os.path.join(a.look, 'cameras.json'), encoding='utf-8'))
    row = cams['light']['row']
    png, meta, peak, env_sha = env_png(a.look, a.work)
    dfg, dfg_path = dfg_table(a.three)
    log('environment atlas %dx%d, max mip %d, peak texel %.4f' % (meta['width'], meta['height'], meta['maxMip'], peak))
    env = env_texture(png)
    mpc = collection(row)
    post = post_material(row)
    simple = simple_materials()
    code = indirect_hlsl(meta, dfg)
    patched = []
    if not a.skip_masters:
        for name in MASTERS:
            patch_master(name, code, mpc, env)
            patched.append(name)
            log('patched ' + name)
    prov = dict(generator='unreal/Tools/look/make-look-assets.py', reference=cams.get('reference'), three=cams.get('three'),
                light=cams['light'], env=dict(meta, scale=ENV_SCALE, source_f32_sha256=env_sha, png_sha256=sha256(open(png, 'rb').read()), peak=peak),
                dfg=dict(source=os.path.relpath(dfg_path, a.three), source_sha256=sha256(open(dfg_path, 'rb').read())),
                hlsl=dict(indirect_sha256=sha256(code.encode()), post_sha256=sha256(POST_HLSL.encode()), ring_sha256=sha256((RING_WPO_HLSL + RING_HLSL).encode())),
                assets=[LOOK + '/' + n for n in ['T_TCLookEnv', 'MPC_TCLook', 'M_TCLookPost'] + sorted(simple)], patched=[ROOT + '/' + n for n in patched])
    out = os.path.join(unreal.Paths.convert_relative_path_to_full(unreal.Paths.project_content_dir()), 'Models/Materials/LookProvenance.json')
    with open(out, 'w', encoding='utf-8') as f:
        json.dump(prov, f, indent=1, sort_keys=True)
    log('done: %d assets, %d masters patched' % (len(prov['assets']), len(patched)))


main()
