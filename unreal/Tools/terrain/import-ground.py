"""The painted ground's assets: textures from the pinned reference's client/textures and the materials the ground and
its decals draw with. (The material tiles the reference's own buildTiles cuts are not assets: make-ground-tiles.mjs
writes them into Source/ThreeCrossroads/World/TCGroundPaint.Tiles.inl.) Generated, never hand-edited: run inside the editor (a content host whose Content/Terrain is this repo's, or the project itself).

  UnrealEditor-Cmd <project> -run=pythonscript -EnablePlugins=PythonScriptPlugin -unattended -nop4 -nosplash -nullrhi \
    -script="unreal/Tools/terrain/import-ground.py --work <scratch dir> [--verify REPORT.json]"

Output under /Game/Terrain (unreal/Content/Terrain):
  Textures/T_TCGround_<name>      grass, dirt, field, road, mud, earth, shelled, rubble, water (sRGB) and detail (linear,
                                  as structures.js loads it), wrapping, mipmapped, the World texture group: the ground's
                                  pictures for the materials that sample them in the GPU (relief, apron)
  Textures/T_TCCraterDecal        client/textures/fx-crater.webp (decoded to PNG by ImageMagick)
  Textures/T_TCRubbleAtlas        client/textures/fx-rubble-materials-v1.webp (likewise), clamped (rubble-decals.js)
  Textures/T_TCGroundPaintDefault, T_TCGroundFogDefault  the parameters' defaults (grey ground, no fog)
  Materials/MF_TCGroundPaint      the material interface the ground mesh uses: outputs Paint, FogDim, PaintFogged;
                                  parameters GroundPaint, GroundFog, GroundMapSize (metres), GroundFogOn, GroundFogTime,
                                  GroundMist (World/TCGroundPaint.h, FTCGroundPaint::Bind)
  Materials/M_TCGroundPaint       lit, specular 0, roughness 1 (MeshLambertMaterial): base colour PaintFogged
  Materials/M_TCCraterDecal       crater-decals.js: modulate (2 x picture x ground), unlit, the picture over its ref
  Materials/M_TCRubbleDecal       rubble-decals.js: translucent Lambert, opacity 0.9, alpha test 0.08, fogged like a piece
  TerrainProvenance.json          sources and their sha256, HLSL sha256, asset sha256
"""
import argparse
import hashlib
import json
import os
import struct
import subprocess
import sys
import zlib

import unreal

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, '..', '..', '..'))
REF = os.path.join(REPO, 'reference', 'ww2-rts')
TEXTURES = os.path.join(REF, 'client', 'textures')
ROOT = '/Game/Terrain'
TOOL_VERSION = 1
GROUND = ['grass', 'dirt', 'field', 'road', 'mud', 'earth', 'shelled', 'rubble', 'water', 'detail']

PAINT_HLSL = '''// client/ground.js: one canvas covers the map, row 0 at its north edge (Unreal X = reference x, Y = reference z, cm)
float2 uv = WorldPos.xy * 0.01 / max(MapSize.xy, float2(1.0, 1.0));
return Paint.Sample(PaintSampler, uv).rgb;
'''

FOG_HLSL = '''// The fog of war as a multiplier (World/TCGroundPaint.h): 1 seen, 0.55 explored, 0.32 never seen, one texel per cell,
// bilinear between cell centres (client/fog.js LinearFilter), so a fog edge is a ramp, not a stair.
if (FogOn < 0.5) return 1.0;
float2 m = WorldPos.xy * 0.01;
float2 uv = m / max(MapSize.xy, float2(1.0, 1.0));
float dim = Fog.SampleLevel(FogSampler, uv, 0).r;
if (Mist < 0.5) return dim;
// client/surfaces.js fowMist on the overlay's alpha (1 - dim): two layers of value noise in metres, sliding slowly.
// Clear ground stays clear; partly drawn fog drifts.
float a = 1.0 - dim;
float mm = 0.0;
[unroll] for (int o = 0; o < 2; o++)
{
	float2 q = o == 0 ? m * 0.05 + Time * float2(0.04, 0.015) : m * 0.13 - Time * float2(0.02, 0.05);
	float2 i = floor(q), f = frac(q);
	f = f * f * (3.0 - 2.0 * f);
	float h[4];
	[unroll] for (int k = 0; k < 4; k++)
	{
		float2 c = i + float2(k & 1, k >> 1);
		c = frac(c * float2(0.1031, 0.1030));
		c += dot(c, c.yx + 33.33);
		h[k] = frac((c.x + c.y) * c.x);
	}
	mm += lerp(lerp(h[0], h[1], f.x), lerp(h[2], h[3], f.x), f.y) * (o == 0 ? 0.65 : 0.35);
}
a = saturate(a + (mm - 0.5) * 1.4 * a * (1.0 - a));
return 1.0 - a;
'''

CRATER_HLSL = '''// client/crater-decals.js: the picture over the colour that leaves the ground as it is (ref), so 2 x picture x ground
// darkens the bowl and lightens its rim; a three.js texture is flipped on load, so v runs up the picture.
float4 t = Map.Sample(MapSampler, float2(UV0.x, 1.0 - UV0.y));
return lerp(float3(1.0, 1.0, 1.0), clamp(pow(max(t.rgb / Ref, float3(0.0, 0.0, 0.0)), 0.85), 0.0, 2.0), t.a * 0.85);
'''

RUBBLE_COLOR_HLSL = '''// client/rubble-decals.js: MeshLambertMaterial with the atlas (flipped on load), darkened in the fog like every piece
return Map.Sample(MapSampler, float2(UV0.x, 1.0 - UV0.y)).rgb * Dim;
'''

RUBBLE_OPACITY_HLSL = '''// opacity 0.9 times the picture's alpha; alphaTest 0.08 discards what is left under it
float a = Map.Sample(MapSampler, float2(UV0.x, 1.0 - UV0.y)).a * 0.9;
return a < 0.08 ? 0.0 : a;
'''


def sha_text(text):
    return hashlib.sha256(text.encode('utf-8')).hexdigest()


def sha_file(path):
    return hashlib.sha256(open(path, 'rb').read()).hexdigest()


def log(msg):
    unreal.log('[terrain] ' + msg)


def write_png(path, w, h, rgba, gray=False):
    """A plain PNG (8-bit RGBA, or 8-bit grey) from raw bytes, so the editor imports exactly these bytes."""
    stride = w if gray else w * 4
    raw = b''.join(b'\x00' + rgba[y * stride:(y + 1) * stride] for y in range(h))

    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data) & 0xffffffff)
    png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 0 if gray else 6, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(raw, 9)) + chunk(b'IEND', b'')
    with open(path, 'wb') as f:
        f.write(png)


class Maker(object):
    def __init__(self, work):
        self.work = work
        self.tools = unreal.AssetToolsHelpers.get_asset_tools()
        self.eal = unreal.EditorAssetLibrary
        self.mel = unreal.MaterialEditingLibrary
        self.report = dict(format='tc-terrain-assets-v1', tool_version=TOOL_VERSION, reference=self.reference_pin(), textures={}, materials={})

    @staticmethod
    def reference_pin():
        try:
            return subprocess.check_output(['git', '-C', REF, 'rev-parse', 'HEAD']).decode().strip()
        except Exception:
            return 'unknown'

    def import_texture(self, src, folder, name):
        task = unreal.AssetImportTask()
        task.set_editor_property('filename', src)
        task.set_editor_property('destination_path', '%s/%s' % (ROOT, folder))
        task.set_editor_property('destination_name', name)
        task.set_editor_property('automated', True)
        task.set_editor_property('replace_existing', True)
        task.set_editor_property('save', False)
        self.tools.import_asset_tasks([task])
        tex = self.eal.load_asset('%s/%s/%s' % (ROOT, folder, name))
        if not tex:
            raise RuntimeError('import failed: ' + src)
        return tex

    def finish_texture(self, tex, **props):
        for k, v in props.items():
            tex.set_editor_property(k, v)
        self.eal.save_loaded_asset(tex, only_if_is_dirty=False)

    # ---------- textures ----------
    def ground_textures(self):
        for name in GROUND:
            src = os.path.join(TEXTURES, name + '.jpg')
            tex = self.import_texture(src, 'Textures', 'T_TCGround_' + name)
            linear = name == 'detail'  # structures.js loads detail.jpg with NoColorSpace (a packed data texture)
            self.finish_texture(tex, srgb=not linear, lod_group=unreal.TextureGroup.TEXTUREGROUP_WORLD,
                                address_x=unreal.TextureAddress.TA_WRAP, address_y=unreal.TextureAddress.TA_WRAP,
                                mip_gen_settings=unreal.TextureMipGenSettings.TMGS_FROM_TEXTURE_GROUP)
            self.report['textures']['T_TCGround_' + name] = dict(source='client/textures/%s.jpg' % name, sha256=sha_file(src), srgb=not linear)

    def decal_textures(self):
        magick = subprocess.check_output(['magick', '-version']).decode().splitlines()[0]
        for src_name, name, clamp in (('fx-crater.webp', 'T_TCCraterDecal', False), ('fx-rubble-materials-v1.webp', 'T_TCRubbleAtlas', True)):
            src = os.path.join(TEXTURES, src_name)
            png = os.path.join(self.work, name + '.png')
            subprocess.check_call(['magick', src, 'PNG32:' + png])
            tex = self.import_texture(png, 'Textures', name)
            addr = unreal.TextureAddress.TA_CLAMP if clamp else unreal.TextureAddress.TA_WRAP
            self.finish_texture(tex, srgb=True, lod_group=unreal.TextureGroup.TEXTUREGROUP_WORLD, address_x=addr, address_y=addr)
            self.report['textures'][name] = dict(source='client/textures/' + src_name, sha256=sha_file(src), decoded_png_sha256=sha_file(png), decoder=magick)
        # the parameters' defaults: mid grey ground, fog off (white)
        grey = os.path.join(self.work, 'paint-default.png')
        write_png(grey, 4, 4, bytes([128, 128, 128, 255] * 16))
        tex = self.import_texture(grey, 'Textures', 'T_TCGroundPaintDefault')
        self.finish_texture(tex, srgb=True)
        white = os.path.join(self.work, 'fog-default.png')
        write_png(white, 4, 4, bytes([255] * 16), gray=True)
        tex = self.import_texture(white, 'Textures', 'T_TCGroundFogDefault')
        self.finish_texture(tex, srgb=False, compression_settings=unreal.TextureCompressionSettings.TC_GRAYSCALE)

    # ---------- materials ----------
    def create(self, sub, name, cls, factory):
        path = '%s/%s/%s' % (ROOT, sub, name)
        if self.eal.does_asset_exist(path):
            asset = self.eal.load_asset(path)
            if asset and isinstance(asset, cls):
                return asset
            self.eal.delete_asset(path)
        return self.tools.create_asset(name, '%s/%s' % (ROOT, sub), cls, factory)

    def custom(self, make, connect, code, inputs, ret, x, y, desc):
        c = make(unreal.MaterialExpressionCustom, x, y)
        c.set_editor_property('code', code)
        c.set_editor_property('description', desc)
        c.set_editor_property('output_type', ret)
        ins = []
        for n, _ in inputs:
            ci = unreal.CustomInput()
            ci.set_editor_property('input_name', n)
            ins.append(ci)
        c.set_editor_property('inputs', ins)
        for n, src in inputs:
            if not connect(src, '', c, n):
                raise RuntimeError('%s: could not connect %s' % (desc, n))
        return c

    def ground_function(self):
        fn = self.create('Materials', 'MF_TCGroundPaint', unreal.MaterialFunction, unreal.MaterialFunctionFactoryNew())
        self.mel.delete_all_material_expressions_in_function(fn)
        fn.set_editor_property('description', 'The painted ground (World/TCGroundPaint.h): Paint, FogDim, PaintFogged. Generated by unreal/Tools/terrain/import-ground.py.')
        fn.set_editor_property('expose_to_library', True)

        def make(cls, x, y, **props):
            e = self.mel.create_material_expression_in_function(fn, cls, x, y)
            for k, v in props.items():
                e.set_editor_property(k, v)
            return e
        connect = self.mel.connect_material_expressions
        T = unreal.CustomMaterialOutputType
        paint_default = self.eal.load_asset(ROOT + '/Textures/T_TCGroundPaintDefault')
        fog_default = self.eal.load_asset(ROOT + '/Textures/T_TCGroundFogDefault')
        wp = make(unreal.MaterialExpressionWorldPosition, -1200, 0)
        paint = make(unreal.MaterialExpressionTextureObjectParameter, -1200, 120, parameter_name='GroundPaint', texture=paint_default, sampler_type=unreal.MaterialSamplerType.SAMPLERTYPE_COLOR)
        fog = make(unreal.MaterialExpressionTextureObjectParameter, -1200, 320, parameter_name='GroundFog', texture=fog_default, sampler_type=unreal.MaterialSamplerType.SAMPLERTYPE_LINEAR_GRAYSCALE)
        size = make(unreal.MaterialExpressionVectorParameter, -1200, 520, parameter_name='GroundMapSize', default_value=unreal.LinearColor(100.0, 100.0, 0.0, 1.0))
        on = make(unreal.MaterialExpressionScalarParameter, -1200, 640, parameter_name='GroundFogOn', default_value=0.0)
        time = make(unreal.MaterialExpressionScalarParameter, -1200, 740, parameter_name='GroundFogTime', default_value=0.0)
        mist = make(unreal.MaterialExpressionScalarParameter, -1200, 840, parameter_name='GroundMist', default_value=1.0)
        pc = self.custom(make, connect, PAINT_HLSL, [('WorldPos', wp), ('MapSize', size), ('Paint', paint)], T.CMOT_FLOAT3, -700, 100, 'TCGroundPaint')
        fc = self.custom(make, connect, FOG_HLSL, [('WorldPos', wp), ('MapSize', size), ('Fog', fog), ('FogOn', on), ('Time', time), ('Mist', mist)], T.CMOT_FLOAT1, -700, 400, 'TCGroundFog')
        mul = make(unreal.MaterialExpressionMultiply, -350, 250)
        connect(pc, '', mul, 'A')
        connect(fc, '', mul, 'B')
        for i, (name, src) in enumerate((('Paint', pc), ('FogDim', fc), ('PaintFogged', mul))):
            out = make(unreal.MaterialExpressionFunctionOutput, 0, i * 150, output_name=name, sort_priority=i)
            if not connect(src, '', out, ''):
                raise RuntimeError('output ' + name)
        self.mel.update_material_function(fn)
        self.eal.save_loaded_asset(fn, only_if_is_dirty=False)
        self.report['materials']['MF_TCGroundPaint'] = dict(paint_hlsl_sha256=sha_text(PAINT_HLSL), fog_hlsl_sha256=sha_text(FOG_HLSL))
        return fn

    def material(self, name, blend, shading):
        mat = self.create('Materials', name, unreal.Material, unreal.MaterialFactoryNew())
        self.mel.delete_all_material_expressions(mat)
        mat.set_editor_property('blend_mode', blend)
        mat.set_editor_property('shading_model', shading)
        mat.set_editor_property('two_sided', False)
        return mat

    def call(self, mat, fn, x, y):
        c = self.mel.create_material_expression(mat, unreal.MaterialExpressionMaterialFunctionCall, x, y)
        c.set_material_function(fn)
        return c

    def lambert(self, mat):
        P = unreal.MaterialProperty
        for prop, v, y in ((P.MP_SPECULAR, 0.0, 200), (P.MP_ROUGHNESS, 1.0, 300), (P.MP_METALLIC, 0.0, 400)):
            c = self.mel.create_material_expression(mat, unreal.MaterialExpressionConstant, -300, y)
            c.set_editor_property('r', v)
            self.mel.connect_material_property(c, '', prop)

    def finish(self, mat, report):
        self.mel.recompile_material(mat)
        self.eal.save_loaded_asset(mat, only_if_is_dirty=False)
        self.report['materials'][mat.get_name()] = report

    def materials(self, fn):
        P = unreal.MaterialProperty
        T = unreal.CustomMaterialOutputType
        connect = self.mel.connect_material_expressions
        # the ground
        mat = self.material('M_TCGroundPaint', unreal.BlendMode.BLEND_OPAQUE, unreal.MaterialShadingModel.MSM_DEFAULT_LIT)
        c = self.call(mat, fn, -600, 0)
        if not self.mel.connect_material_property(c, 'PaintFogged', P.MP_BASE_COLOR):
            raise RuntimeError('M_TCGroundPaint base colour')
        self.lambert(mat)
        self.finish(mat, dict(function='MF_TCGroundPaint', output='PaintFogged'))

        def make_in(m):
            def make(cls, x, y, **props):
                e = self.mel.create_material_expression(m, cls, x, y)
                for k, v in props.items():
                    e.set_editor_property(k, v)
                return e
            return make
        # crater decals: modulate
        mat = self.material('M_TCCraterDecal', unreal.BlendMode.BLEND_MODULATE, unreal.MaterialShadingModel.MSM_UNLIT)
        make = make_in(mat)
        crater = self.eal.load_asset(ROOT + '/Textures/T_TCCraterDecal')
        uv = make(unreal.MaterialExpressionTextureCoordinate, -900, 0, coordinate_index=0)
        tex = make(unreal.MaterialExpressionTextureObjectParameter, -900, 100, parameter_name='Map', texture=crater)
        ref = make(unreal.MaterialExpressionVectorParameter, -900, 300, parameter_name='Ref', default_value=unreal.LinearColor(0.146, 0.107, 0.06, 1.0))
        cc = self.custom(make, connect, CRATER_HLSL, [('UV0', uv), ('Map', tex), ('Ref', ref)], T.CMOT_FLOAT3, -500, 0, 'TCCraterDecal')
        self.mel.connect_material_property(cc, '', P.MP_EMISSIVE_COLOR)
        self.finish(mat, dict(blend='modulate', hlsl_sha256=sha_text(CRATER_HLSL)))
        # rubble decals: translucent Lambert, fogged
        mat = self.material('M_TCRubbleDecal', unreal.BlendMode.BLEND_TRANSLUCENT, unreal.MaterialShadingModel.MSM_DEFAULT_LIT)
        mat.set_editor_property('translucency_lighting_mode', unreal.TranslucencyLightingMode.TLM_SURFACE_PER_PIXEL_LIGHTING)
        make = make_in(mat)
        atlas = self.eal.load_asset(ROOT + '/Textures/T_TCRubbleAtlas')
        uv = make(unreal.MaterialExpressionTextureCoordinate, -900, 0, coordinate_index=0)
        tex = make(unreal.MaterialExpressionTextureObjectParameter, -900, 100, parameter_name='Map', texture=atlas)
        g = self.call(mat, fn, -900, 400)
        col = self.custom(make, connect, RUBBLE_COLOR_HLSL, [('UV0', uv), ('Map', tex), ('Dim', g)], T.CMOT_FLOAT3, -500, 0, 'TCRubbleColor')
        # the call's FogDim output feeds Dim
        if not connect(g, 'FogDim', col, 'Dim'):
            raise RuntimeError('rubble Dim')
        op = self.custom(make, connect, RUBBLE_OPACITY_HLSL, [('UV0', uv), ('Map', tex)], T.CMOT_FLOAT1, -500, 300, 'TCRubbleOpacity')
        self.mel.connect_material_property(col, '', P.MP_BASE_COLOR)
        self.mel.connect_material_property(op, '', P.MP_OPACITY)
        self.lambert(mat)
        self.finish(mat, dict(blend='translucent', lighting='surface per pixel', color_hlsl_sha256=sha_text(RUBBLE_COLOR_HLSL), opacity_hlsl_sha256=sha_text(RUBBLE_OPACITY_HLSL)))

    def asset_hashes(self, content_dir):
        out = {}
        for base, _, files in os.walk(content_dir):
            for f in sorted(files):
                if f.endswith('.uasset'):
                    p = os.path.join(base, f)
                    out[os.path.relpath(p, content_dir)] = sha_file(p)
        return out


def verify(report_path):
    """Loads every asset back and checks what the ground relies on; writes a JSON report."""
    eal = unreal.EditorAssetLibrary
    out = dict(ok=True, checks=[])

    def check(name, ok, detail=''):
        out['checks'].append(dict(name=name, ok=bool(ok), detail=detail))
        if not ok:
            out['ok'] = False
    prov = json.load(open(os.path.join(REPO, 'unreal', 'Content', 'Terrain', 'TerrainProvenance.json')))
    for name, t in sorted(prov['textures'].items()):
        tex = eal.load_asset('%s/Textures/%s' % (ROOT, name))
        check('texture ' + name, tex is not None and ('srgb' not in t or tex.get_editor_property('srgb') == t['srgb']), t.get('source', ''))
    for m in ('M_TCGroundPaint', 'M_TCCraterDecal', 'M_TCRubbleDecal'):
        mat = eal.load_asset('%s/Materials/%s' % (ROOT, m))
        check('material ' + m, mat is not None)
    fn = eal.load_asset(ROOT + '/Materials/MF_TCGroundPaint')
    check('function MF_TCGroundPaint', fn is not None)
    json.dump(out, open(report_path, 'w'), indent=1)
    log('verify: %s (%d checks)' % ('ok' if out['ok'] else 'FAILED', len(out['checks'])))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--work', required=True, help='a scratch folder for the PNGs it imports')
    ap.add_argument('--verify', help='only check the assets; write a report here')
    args = ap.parse_args(sys.argv[1:] if len(sys.argv) > 1 else [])
    if args.verify:
        return verify(args.verify)
    os.makedirs(args.work, exist_ok=True)
    mk = Maker(args.work)
    mk.ground_textures()
    mk.decal_textures()
    fn = mk.ground_function()
    mk.materials(fn)
    content = os.path.join(REPO, 'unreal', 'Content', 'Terrain')
    mk.report['sources'] = {('client/textures/%s.jpg' % n): sha_file(os.path.join(TEXTURES, n + '.jpg')) for n in GROUND}
    for f in ('client/ground.js', 'client/surfaces.js', 'client/crater-decals.js', 'client/rubble-decals.js', 'client/fog.js'):
        mk.report['sources'][f] = sha_file(os.path.join(REF, f))
    mk.report['assets'] = mk.asset_hashes(content)
    json.dump(mk.report, open(os.path.join(content, 'TerrainProvenance.json'), 'w'), indent=1, sort_keys=True)
    log('done: %d textures, %d materials' % (len(mk.report['textures']), len(mk.report['materials'])))


main()
