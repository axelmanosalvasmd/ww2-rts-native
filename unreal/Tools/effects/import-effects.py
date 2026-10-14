"""Imports the combat effect textures and builds the effect materials (Content/Effects), inside an Unreal editor:

    UnrealEditor-Cmd <host>.uproject -run=pythonscript -unattended -nop4 -nosplash -nullrhi \
        -script="<repo>/unreal/Tools/effects/import-effects.py --export <repo>/unreal/Saved/EffectsExport"
    ... -script="<repo>/unreal/Tools/effects/import-effects.py --verify <file.json>"

<host> is any project whose Content/Effects is (or links to) the repo's unreal/Content/Effects, with the Python
plugin on (run.sh makes a content-only one in scratch). The PNGs come from convert-fx-textures.mjs.

What it makes, all ported from the pinned reference (reference/ww2-rts at e4e579a):
  Textures/T_FxAtlas   client/textures/fx-atlas.webp, premultiplied (as client/fx.js uploads it), linear, BC7
  Textures/T_FxCrater  client/textures/fx-crater.webp, sRGB, BC7
  Textures/T_FxRubble  client/textures/fx-rubble-materials-v1.webp, sRGB, BC7
  Materials/M_TCFxSprite  client/fx.js's particle ShaderMaterial (FRAG): unlit, premultiplied alpha (Unreal's
                          AlphaComposite is fx.js's One / OneMinusSrcAlpha blend), lit smoke from the atlas normals
                          under the sun and sky, glowing fire with its additive share. The vertex half (billboards,
                          stretched streaks, anchors, pull, the soft ground edge) is computed per view by
                          ATCEffects' sprite component (Client/TCEffects.cpp), which hands the pixel half its inputs in
                          eight texture coordinates (see SPRITE_HLSL).
  Materials/M_TCFxCrater  client/fx.js's crater decal (dmat): Modulate, mix(1, m, alpha * fade), the burn mark and the
                          stain kinds, fogged toward 1.
  Materials/M_TCFxRubble  client/rubble-decals.js: a Lambert surface (rough, no specular) cut out where alpha x 0.9
                          is below 0.08 (three's alphaTest), opaque elsewhere.
  EffectsProvenance.json  sources, their sha256 and each material's HLSL sha256.
"""
import argparse
import hashlib
import json
import os
import sys

import unreal

ROOT = '/Game/Effects'

# ---------------------------------------------------------------------------------------------------------- the HLSL
# client/fx.js FRAG, the fragment shader of the particle material, line by line. Its varyings arrive as texture
# coordinates (ATCEffects writes them per vertex):
#   UV0 vUv0 (frame A), UV1 vUv1 (frame B), UV2 vShape.zw (the quad corner c), UV3 vShape.xy (the sprite's turn),
#   UV4 (vInfo.x frame blend, vInfo.y: -1 lit or the additive share), UV5.x vInfo.z / vInfo.w (height above the ground
#   over the soft-edge distance; the vertex computes both, so their ratio interpolates exactly), UV6 vCol.rg, UV7 vCol.b
#   and vCol.a.
# The uniforms: SunV and UpV are the view-space sun and up (three's view space: z toward the eye, so Unreal's view
# z is negated), SunCol, SkyCol, GroundCol as client/fx.js light() derives them from the world's lights, Glow the hot
# part's scale (the ratio of this scene's sun to the reference sun; see ATCEffects::UpdateLight). fx.js lights smoke
# "like the world": with SkyFromScene the sky and ground fill is the scene's own sky light (its irradiance at the
# puff's normal, as Unreal lights the ground), else the hemisphere of SkyCol and GroundCol. Output: premultiplied colour, Opacity = the
# share that covers what is behind (fx.js: gl_FragColor = vec4(rgb * a, a * (1 - add))).
SPRITE_HLSL = r'''
float3 sunV = float3(SunView.x, SunView.y, -SunView.z);
float3 upV = float3(UpView.x, UpView.y, -UpView.z);
float4 t = Texture2DSample(Atlas, AtlasSampler, UV0);
if (UV4.x > 0.0) t = lerp(t, Texture2DSample(Atlas, AtlasSampler, UV1), UV4.x);
float ratio = UV5.x;
float a = t.a * UV7.y * saturate(ratio);
OutOpacity = 0.0;
if (a < 0.004) return float3(0.0, 0.0, 0.0);
float ta = max(t.a, 0.004);
float add;
float3 vcol = float3(UV6.x, UV6.y, UV7.x);
float3 rgb;
if (UV4.y < 0.0)
{
    float2 r = UV3;
    float2 ns = t.rb / ta * 2.0 - 1.0;
    float2 nv = float2(ns.x * r.x - ns.y * r.y, ns.x * r.y + ns.y * r.x);
    float2 sp = float2(UV2.x * r.x - UV2.y * r.y, UV2.x * r.y + UV2.y * r.x);
    float3 n = normalize(float3(nv + sp * 0.6, 0.75));
    float diff = saturate(dot(n, sunV) * 0.55 + 0.45);
    float3 amb = lerp(GroundCol, SkyCol, saturate(dot(n, upV) * 0.5 + 0.5));
    if (SkyFromScene > 0.5)
    {
        // the scene's sky light at this normal (ReflectionEnvironmentShared.ush GetSkySHDiffuse, inlined: that file is
        // included after the material code), as Unreal lights the ground with it
        float4 nw = float4(normalize(mul(float4(n.x, n.y, -n.z, 0.0), ResolvedView.ViewToTranslatedWorld).xyz), 1.0);
        float4 vb = nw.xyzz * nw.yzzx;
        float vc = nw.x * nw.x - nw.y * nw.y;
        float3 i0 = float3(dot(View.SkyIrradianceEnvironmentMap[0], nw), dot(View.SkyIrradianceEnvironmentMap[1], nw), dot(View.SkyIrradianceEnvironmentMap[2], nw));
        float3 i1 = float3(dot(View.SkyIrradianceEnvironmentMap[3], vb), dot(View.SkyIrradianceEnvironmentMap[4], vb), dot(View.SkyIrradianceEnvironmentMap[5], vb));
        amb = max(0.0, i0 + i1 + View.SkyIrradianceEnvironmentMap[6].xyz * vc) * View.SkyLightColor.rgb;
    }
    float low = saturate(ratio / 6.0);
    float back = saturate(-sunV.z);
    float thin = 1.0 - saturate(t.a * 1.3);
    rgb = vcol * ((t.g / ta * 2.0) * (amb + SunCol * diff) * (0.72 + 0.28 * low) + SunCol * back * back * thin * 1.5);
    add = 0.0;
}
else
{
    float3 c = t.rgb / ta;
    c *= c;
    float hot = smoothstep(0.06, 0.45, max(c.r, max(c.g, c.b)));
    float3 fill = lerp(GroundCol, SkyCol, 0.8);
    if (SkyFromScene > 0.5)
    {
        // fx.js mix(ground, sky, 0.8): the scene's sky light from straight up and straight down, mixed the same way
        float4 up4 = float4(0.0, 0.0, 1.0, 1.0), dn4 = float4(0.0, 0.0, -1.0, 1.0);
        float4 ub = up4.xyzz * up4.yzzx, db = dn4.xyzz * dn4.yzzx;
        float3 su = float3(dot(View.SkyIrradianceEnvironmentMap[0], up4), dot(View.SkyIrradianceEnvironmentMap[1], up4), dot(View.SkyIrradianceEnvironmentMap[2], up4))
            + float3(dot(View.SkyIrradianceEnvironmentMap[3], ub), dot(View.SkyIrradianceEnvironmentMap[4], ub), dot(View.SkyIrradianceEnvironmentMap[5], ub));
        float3 sd = float3(dot(View.SkyIrradianceEnvironmentMap[0], dn4), dot(View.SkyIrradianceEnvironmentMap[1], dn4), dot(View.SkyIrradianceEnvironmentMap[2], dn4))
            + float3(dot(View.SkyIrradianceEnvironmentMap[3], db), dot(View.SkyIrradianceEnvironmentMap[4], db), dot(View.SkyIrradianceEnvironmentMap[5], db));
        fill = lerp(max(0.0, sd), max(0.0, su), 0.8) * View.SkyLightColor.rgb;
    }
    float3 cold = c * 2.2 * (fill + SunCol * 0.6);
    rgb = lerp(cold, c * vcol * Glow, hot);
    add = hot * UV4.y;
}
OutOpacity = saturate(a * (1.0 - add));
return rgb * a;
'''

# client/fx.js dmat's fragment shader: the crater picture against its reference colour (the colour that leaves the
# ground as it is), the burn mark (kind 1) and the stain (kind 2); written as the factor the ground is multiplied by
# (fx.js blends DstColor * Src + Src * Dst with Src = 0.5 c, which is c times the ground: Unreal's Modulate). Fog: the
# Modulate blend already eases the factor toward 1 under fog, as the reference's linear fog does.
#   UV0 the crater's uv, UV1.x fade, UV1.y kind.
CRATER_HLSL = r'''
float4 t = Texture2DSample(Crater, CraterSampler, UV0);
float kind = UV1.y;
float3 m = kind > 1.5 ? float3(0.5, 0.16, 0.13) : (kind > 0.5 ? float3(0.36, 0.33, 0.3) : clamp(pow(max(t.rgb, 0.0) / Ref, 0.85), 0.0, 2.0));
return lerp(float3(1.0, 1.0, 1.0), m, t.a * UV1.x);
'''

# client/rubble-decals.js: MeshLambertMaterial({ map, transparent, opacity: 0.9, alphaTest: 0.08 }): three discards
# where alpha * 0.9 is below 0.08 and blends the rest at alpha * 0.9. Here the same test cuts it out (masked, clip 0.08)
# and what stays is opaque, so it keeps the opaque pass's light and shadows; the 10 % see-through is not ported.
RUBBLE_HLSL = r'''
float4 t = Texture2DSample(Rubble, RubbleSampler, UV0);
OutMask = t.a * 0.9;
return t.rgb;
'''

CRATER_REF = (0.146, 0.107, 0.06)  # fx.js dmat uniform ref


def sha256_file(p):
    h = hashlib.sha256()
    with open(p, 'rb') as f:
        for chunk in iter(lambda: f.read(1 << 20), b''):
            h.update(chunk)
    return h.hexdigest()


def sha(text):
    return hashlib.sha256(text.encode('utf-8')).hexdigest()


class Builder:
    def __init__(self):
        self.tools = unreal.AssetToolsHelpers.get_asset_tools()
        self.eal = unreal.EditorAssetLibrary
        self.mel = unreal.MaterialEditingLibrary
        self.provenance = dict(textures=[], materials=[])

    def path(self, sub, name):
        return '%s/%s/%s' % (ROOT, sub, name)

    def save(self, asset):
        self.eal.save_loaded_asset(asset, only_if_is_dirty=False)

    def create(self, sub, name, cls, factory):
        full = self.path(sub, name)
        if self.eal.does_asset_exist(full):
            asset = self.eal.load_asset(full)
            if asset and isinstance(asset, cls):
                return asset
            self.eal.delete_asset(full)
        return self.tools.create_asset(name, '%s/%s' % (ROOT, sub), cls, factory)

    def texture(self, src, name, srgb, source_entry):
        task = unreal.AssetImportTask()
        task.set_editor_property('filename', src)
        task.set_editor_property('destination_path', '%s/Textures' % ROOT)
        task.set_editor_property('destination_name', name)
        task.set_editor_property('automated', True)
        task.set_editor_property('replace_existing', True)
        task.set_editor_property('save', False)
        self.tools.import_asset_tasks([task])
        tex = self.eal.load_asset(self.path('Textures', name))
        if not tex:
            raise RuntimeError('import failed: %s' % src)
        tex.set_editor_property('srgb', srgb)
        tex.set_editor_property('compression_settings', unreal.TextureCompressionSettings.TC_BC7)
        tex.set_editor_property('lod_group', unreal.TextureGroup.TEXTUREGROUP_EFFECTS)
        tex.set_editor_property('never_stream', True)
        tex.set_editor_property('address_x', unreal.TextureAddress.TA_CLAMP)
        tex.set_editor_property('address_y', unreal.TextureAddress.TA_CLAMP)
        self.save(tex)
        self.provenance['textures'].append(dict(asset=self.path('Textures', name), png=os.path.basename(src), png_sha256=sha256_file(src),
                                                source=source_entry['source'], source_sha256=source_entry['sourceSha256'],
                                                premultiplied=source_entry['premultiplied'], srgb=srgb, compression='TC_BC7'))
        return tex

    def expr(self, mat, cls, x, y, **props):
        e = self.mel.create_material_expression(mat, cls, x, y)
        for k, v in props.items():
            e.set_editor_property(k, v)
        return e

    def rgb(self, mat, src, x, y):
        m = self.expr(mat, unreal.MaterialExpressionComponentMask, x, y, r=True, g=True, b=True, a=False)
        self.mel.connect_material_expressions(src, '', m, '')
        return m

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

    def material(self, name):
        mat = self.create('Materials', name, unreal.Material, unreal.MaterialFactoryNew())
        self.mel.delete_all_material_expressions(mat)
        return mat

    def finish(self, mat, code, sources):
        self.mel.recompile_material(mat)
        self.save(mat)
        self.provenance['materials'].append(dict(asset=mat.get_path_name().split('.')[0], hlsl_sha256=sha(code), ported_from=sources))

    def sprite(self, atlas):
        T = unreal.CustomMaterialOutputType
        P = unreal.MaterialProperty
        mat = self.material('M_TCFxSprite')
        mat.set_editor_property('blend_mode', unreal.BlendMode.BLEND_ALPHA_COMPOSITE)
        mat.set_editor_property('shading_model', unreal.MaterialShadingModel.MSM_UNLIT)
        mat.set_editor_property('two_sided', True)
        X = -1400
        rows = iter(range(-600, 2400, 110))
        uv = lambda i: self.expr(mat, unreal.MaterialExpressionTextureCoordinate, X, next(rows), coordinate_index=i)
        vec = lambda n, v: self.rgb(mat, self.expr(mat, unreal.MaterialExpressionVectorParameter, X - 300, next(rows), parameter_name=n,
                                                   default_value=unreal.LinearColor(v[0], v[1], v[2], 1.0)), X, 0)

        def view(n, v):
            src = vec(n, v)
            tr = self.expr(mat, unreal.MaterialExpressionTransform, X + 200, next(rows),
                           transform_source_type=unreal.MaterialVectorCoordTransformSource.TRANSFORMSOURCE_WORLD,
                           transform_type=unreal.MaterialVectorCoordTransform.TRANSFORM_VIEW)
            self.mel.connect_material_expressions(src, '', tr, '')
            return tr
        # defaults: client/moods.js 'day' through fx.js light() (sun 0xffe7c9 x 3.2 / pi, hemisphere 0xc3cddd and 0x8a806b
        # x 1.25 / pi, linear); ATCEffects sets the live values
        inputs = [('Atlas', self.expr(mat, unreal.MaterialExpressionTextureObjectParameter, X, next(rows), parameter_name='Atlas', texture=atlas))]
        inputs += [('UV%d' % i, uv(i)) for i in range(8)]
        inputs += [('SunView', view('SunDir', (0.0, 0.0, 1.0))), ('UpView', view('Up', (0.0, 0.0, 1.0))),
                   ('SunCol', vec('SunCol', (1.019, 0.814, 0.596))), ('SkyCol', vec('SkyCol', (0.217, 0.243, 0.288))),
                   ('GroundCol', vec('GroundCol', (0.101, 0.086, 0.058))),
                   ('Glow', self.expr(mat, unreal.MaterialExpressionScalarParameter, X, next(rows), parameter_name='Glow', default_value=1.0)),
                   ('SkyFromScene', self.expr(mat, unreal.MaterialExpressionScalarParameter, X, next(rows), parameter_name='SkyFromScene', default_value=0.0))]
        c = self.custom(mat, SPRITE_HLSL, inputs, [('OutOpacity', T.CMOT_FLOAT1)], T.CMOT_FLOAT3, -500, 0, 'client/fx.js FRAG')
        if not self.mel.connect_material_property(c, '', P.MP_EMISSIVE_COLOR):
            raise RuntimeError('emissive')
        if not self.mel.connect_material_property(c, 'OutOpacity', P.MP_OPACITY):
            raise RuntimeError('opacity')
        self.finish(mat, SPRITE_HLSL, ['client/fx.js FX, VERT, FRAG'])
        return mat

    def crater(self, tex):
        T = unreal.CustomMaterialOutputType
        P = unreal.MaterialProperty
        mat = self.material('M_TCFxCrater')
        mat.set_editor_property('blend_mode', unreal.BlendMode.BLEND_MODULATE)
        mat.set_editor_property('shading_model', unreal.MaterialShadingModel.MSM_UNLIT)
        mat.set_editor_property('two_sided', True)
        ref = self.rgb(mat, self.expr(mat, unreal.MaterialExpressionVectorParameter, -1500, 300, parameter_name='Ref',
                                      default_value=unreal.LinearColor(CRATER_REF[0], CRATER_REF[1], CRATER_REF[2], 1.0)), -1200, 300)
        inputs = [('Crater', self.expr(mat, unreal.MaterialExpressionTextureObjectParameter, -1200, 0, parameter_name='Crater', texture=tex)),
                  ('UV0', self.expr(mat, unreal.MaterialExpressionTextureCoordinate, -1200, 100, coordinate_index=0)),
                  ('UV1', self.expr(mat, unreal.MaterialExpressionTextureCoordinate, -1200, 200, coordinate_index=1)),
                  ('Ref', ref)]
        c = self.custom(mat, CRATER_HLSL, inputs, [], T.CMOT_FLOAT3, -500, 0, 'client/fx.js dmat')
        if not self.mel.connect_material_property(c, '', P.MP_EMISSIVE_COLOR):
            raise RuntimeError('emissive')
        self.finish(mat, CRATER_HLSL, ['client/fx.js scorch, fitDecal, dmat'])
        return mat

    def rubble(self, tex):
        T = unreal.CustomMaterialOutputType
        P = unreal.MaterialProperty
        mat = self.material('M_TCFxRubble')
        mat.set_editor_property('blend_mode', unreal.BlendMode.BLEND_MASKED)
        mat.set_editor_property('shading_model', unreal.MaterialShadingModel.MSM_DEFAULT_LIT)
        mat.set_editor_property('opacity_mask_clip_value', 0.08)
        inputs = [('Rubble', self.expr(mat, unreal.MaterialExpressionTextureObjectParameter, -1200, 0, parameter_name='Rubble', texture=tex)),
                  ('UV0', self.expr(mat, unreal.MaterialExpressionTextureCoordinate, -1200, 100, coordinate_index=0))]
        c = self.custom(mat, RUBBLE_HLSL, inputs, [('OutMask', T.CMOT_FLOAT1)], T.CMOT_FLOAT3, -700, 0, 'client/rubble-decals.js')
        ok = self.mel.connect_material_property(c, '', P.MP_BASE_COLOR)
        ok &= self.mel.connect_material_property(c, 'OutMask', P.MP_OPACITY_MASK)
        ok &= self.mel.connect_material_property(self.expr(mat, unreal.MaterialExpressionConstant, -300, 300, r=1.0), '', P.MP_ROUGHNESS)
        ok &= self.mel.connect_material_property(self.expr(mat, unreal.MaterialExpressionConstant, -300, 400, r=0.0), '', P.MP_SPECULAR)
        if not ok:
            raise RuntimeError('rubble outputs')
        self.finish(mat, RUBBLE_HLSL, ['client/rubble-decals.js'])
        return mat


def content_dir():
    return os.path.join(unreal.Paths.convert_relative_path_to_full(unreal.Paths.project_content_dir()), 'Effects')


def build(export):
    with open(os.path.join(export, 'manifest.json')) as f:
        manifest = json.load(f)
    by = {e['output']: e for e in manifest['files']}
    b = Builder()
    atlas = b.texture(os.path.join(export, 'fx-atlas.png'), 'T_FxAtlas', False, by['fx-atlas.png'])
    crater = b.texture(os.path.join(export, 'fx-crater.png'), 'T_FxCrater', True, by['fx-crater.png'])
    rubble = b.texture(os.path.join(export, 'fx-rubble-materials-v1.png'), 'T_FxRubble', True, by['fx-rubble-materials-v1.png'])
    b.sprite(atlas)
    b.crater(crater)
    b.rubble(rubble)
    prov = dict(reference=manifest['reference'], converter=manifest['tool'], converter_sha256=manifest['toolSha256'],
                imagemagick=manifest['imagemagick'], importer='unreal/Tools/effects/import-effects.py', importer_sha256=sha256_file(__file__),
                engine=unreal.SystemLibrary.get_engine_version(), **b.provenance)
    with open(os.path.join(content_dir(), 'EffectsProvenance.json'), 'w') as f:
        json.dump(prov, f, indent=2, sort_keys=True)
        f.write('\n')
    unreal.log('import-effects: %d textures, %d materials' % (len(b.provenance['textures']), len(b.provenance['materials'])))


def verify(out):
    eal = unreal.EditorAssetLibrary
    report = dict(ok=True, assets={})
    want = {
        'Textures/T_FxAtlas': dict(cls=unreal.Texture2D, srgb=False, size=2048),
        'Textures/T_FxCrater': dict(cls=unreal.Texture2D, srgb=True, size=512),
        'Textures/T_FxRubble': dict(cls=unreal.Texture2D, srgb=True, size=768),
        'Materials/M_TCFxSprite': dict(cls=unreal.Material, blend=unreal.BlendMode.BLEND_ALPHA_COMPOSITE, shading=unreal.MaterialShadingModel.MSM_UNLIT),
        'Materials/M_TCFxCrater': dict(cls=unreal.Material, blend=unreal.BlendMode.BLEND_MODULATE, shading=unreal.MaterialShadingModel.MSM_UNLIT),
        'Materials/M_TCFxRubble': dict(cls=unreal.Material, blend=unreal.BlendMode.BLEND_MASKED, shading=unreal.MaterialShadingModel.MSM_DEFAULT_LIT),
    }
    for rel, w in sorted(want.items()):
        path = '%s/%s' % (ROOT, rel)
        a = eal.load_asset(path)
        entry = dict(found=bool(a))
        good = bool(a) and isinstance(a, w['cls'])
        if good and w['cls'] is unreal.Texture2D:
            entry.update(srgb=a.get_editor_property('srgb'), width=a.blueprint_get_size_x(), height=a.blueprint_get_size_y(),
                         compression=str(a.get_editor_property('compression_settings')))
            good = entry['srgb'] == w['srgb'] and entry['width'] == w['size'] and entry['height'] == w['size']
        if good and w['cls'] is unreal.Material:
            entry.update(blend=str(a.get_editor_property('blend_mode')), shading=str(a.get_editor_property('shading_model')),
                         expressions=unreal.MaterialEditingLibrary.get_num_material_expressions(a))
            good = a.get_editor_property('blend_mode') == w['blend'] and a.get_editor_property('shading_model') == w['shading']
        entry['ok'] = good
        report['ok'] &= good
        report['assets'][path] = entry
    with open(out, 'w') as f:
        json.dump(report, f, indent=2, sort_keys=True)
        f.write('\n')
    unreal.log('import-effects verify: %s' % ('ok' if report['ok'] else 'FAILED'))
    if not report['ok']:
        raise RuntimeError('verify failed: %s' % out)


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--export')
    p.add_argument('--verify')
    a = p.parse_args(sys.argv[1:] if len(sys.argv) > 1 else [])
    if a.export:
        build(a.export)
    if a.verify:
        verify(a.verify)


main()
