"""The atmosphere's materials (World/TCAtmosphere.h, ATCAtmosphere). Generated, never hand-edited: run inside the editor.

  UnrealEditor-Cmd <project> -run=pythonscript -EnablePlugins=PythonScriptPlugin -unattended -nop4 -nosplash -nullrhi \
    -script="unreal/Tools/atmosphere/make-atmosphere-assets.py [--verify REPORT.json]"

Each is the reference shader of client/atmosphere.js (pin e4e579a) as a Custom node, in the reference's axes and metres
(three's x is Unreal X, three's z is Unreal Y, three's y is Unreal Z; Unreal positions are cm). three's scene fog (the
FOG_FADE lines) is left out of the mist, fall and bird layers: the haze is mixed by M_TCLookPost over everything,
these layers included; the ground layer (cover and cloud shade) keeps it (round 14), because there the fade lets the
hazed ground show through and the haze over the composite alone does not. The fog of war multiplies their colour by the cell's texel (FogMap, as the world materials do), which is
what the reference's fog overlay drawn over them does. Each layer blends as three blends it, in display space
(DISPLAY_HLSL below).

Output under /Game/Atmosphere (unreal/Content/Atmosphere):
  Materials/M_TCAtmosGround  coverMaterial (wet ground, mud by the roads, puddles catching the sky, lying snow) and
                             cloudMaterial (the cloud shade) as one transparent layer in the ground's shape, cover first
                             (render orders 0.58 and 0.6)
  Materials/M_TCAtmosMist    mistMaterial: the wisp sheets of river mist and ground fog banks
  Materials/M_TCAtmosFall    weatherPoints: rain streaks, snow and dust dots as camera-facing quads moved by World
                             Position Offset exactly as the reference's vertex shader moves its points
  Materials/M_TCAtmosBird    birdMesh's fragment shader (the flapping is done on the CPU, TCAtmosCore::FlapLift)
  AtmosphereMaterials.json   material -> HLSL sha256, uasset sha256, reference lines
"""
import argparse
import hashlib
import json
import os
import sys

try:
    import unreal
except ImportError:
    unreal = None

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = '/Game/Atmosphere'
TOOL_VERSION = 4

FOG_HLSL = '''
float fogK = 1.0;
if (FogOn > 0.5) {
	// one texel per cell (the world materials' FogMap): 1 in sight, 0.55 explored, 0.32 never seen
	fogK = FogMap.SampleLevel(FogMapSampler, xz / max(FogSize.xy, float2(1.0, 1.0)), 0).r;
}
'''

# three renders straight to the canvas on Graphics Low: every transparent layer is tone mapped in its own shader
# (tonemapping_fragment, colorspace_fragment) and alpha-blended in display space (on High the composer blends in linear
# light: LinearBlend 1, round 14). The engine blends translucency in linear light before
# M_TCLookPost tone maps. DISPLAY_HLSL turns the reference's alpha (A, for colour Col) into the linear alpha that gives
# the same display-space result over the opaque scene behind (SceneColor), on luminance: T(x) = sRGB(ACES fit(x
# exposure / 0.6)) for grey (three's input and output matrices keep grey), and T's inverse. Layers stacked over other
# translucent layers see only the opaque scene, a small error where rain falls over the wet ground.
DISPLAY_HLSL = '''
float3 lumW = float3(0.2126, 0.7152, 0.0722);
float dY = max(dot(Scene.rgb, lumW) / max(View.PreExposure, 1e-6), 0.0), cY = max(dot(Col, lumW), 0.0);
float ek = Exposure / 0.6;
float tv[2] = { dY * ek, cY * ek };
float td[2];
[unroll] for (int k = 0; k < 2; k++) {
	float v = tv[k];
	float f = saturate((v * (v + 0.0245786) - 0.000090537) / (v * (0.983729 * v + 0.4329510) + 0.238081));
	td[k] = f <= 0.0031308 ? 12.92 * f : 1.055 * pow(f, 1.0 / 2.4) - 0.055;
}
float want = td[0] * (1.0 - A) + td[1] * A;                       // the reference's blend, display space
float y = want <= 0.04045 ? want / 12.92 : pow((want + 0.055) / 1.055, 2.4);
y = min(y, 0.999);
float qa = 1.0 - 0.983729 * y, qb = 0.0245786 - 0.4329510 * y, qc = -(0.000090537 + 0.238081 * y);
float vWant = (-qb + sqrt(max(qb * qb - 4.0 * qa * qc, 0.0))) / (2.0 * qa);
float xWant = vWant / ek;                                         // the linear luminance that displays as want
float alphaLin = abs(cY - dY) > 1e-5 ? saturate((xWant - dY) / (cY - dY)) : A;
// round 14: that holds for Graphics Low only. On High light.js draws through its composer into a linear half-float
// target, where three blends every transparent layer in linear light before OutputPass tone-maps: the engine's own
// blend, so the reference's alpha is used as it is (LinearBlend 1, the game; ATCLighting -TCHazeAfterTone sets 0)
if (LinearBlend > 0.5) alphaLin = A;
'''

GROUND_HLSL = '''// client/atmosphere.js coverMaterial then cloudMaterial, composited in that order (render orders 0.58, 0.6).
float2 xz = WorldPos.xy * 0.01;           // reference x, z in metres
float ny = Normal.z;                      // the normal's up component (vN.y)
float vy = normalize(CamPos - WorldPos).z; // normalize(vView).y
float4 acc = float4(0.0, 0.0, 0.0, 0.0);
float a_;
// round 14: FOG_FADE. In the reference each layer's alpha fades with the scene fog (cover: a = acc.a * (1 - fogF);
// cloud: alpha * (1 - fogF)), so the hazed ground shows through where the haze grows; M_TCLookPost's haze over the
// composite alone differs by a f (1 - f) (haze - ground). FogNear and FogFar are light.js followView's (ATCLighting).
float fogF = FogFar > FogNear ? smoothstep(FogNear, FogFar, Depth * 0.01) : 0.0;
if (CoverOn > 0.5) {
	float4 m = Mask.Sample(MaskSampler, xz / MapSize.xy);
	float dry = 1.0 - m.g, flat_ = smoothstep(0.82, 0.97, ny);
	float big = Noise.Sample(NoiseSampler, xz / 41.0).r, small = Noise.Sample(NoiseSampler, xz / 9.0 + 0.37).r;
	a_ = Wet * 0.3 * dry;
	acc = float4(float3(0.05, 0.045, 0.04) * a_ + acc.rgb * (1.0 - a_), a_ + acc.a * (1.0 - a_));
	float mud = Mud * smoothstep(0.1, 0.75, m.r) * smoothstep(0.3, 0.55, big * 0.65 + small * 0.35 + m.b * 0.2) * dry;
	a_ = mud * 0.62;
	acc = float4(MudColor * a_ + acc.rgb * (1.0 - a_), a_ + acc.a * (1.0 - a_));
	// puddles: in the ruts by the roads and in low patches, shining more when seen at a slant
	float pud = Sheen * smoothstep(0.6, 0.66, big * 0.55 + small * 0.3 + m.r * 0.22) * dry * flat_;
	float glance = pow(1.0 - clamp(vy, 0.0, 1.0), 2.0);
	float ripple = 0.85 + 0.15 * sin(Time * 3.1 + small * 40.0);
	a_ = pud * (0.3 + 0.55 * glance);
	acc = float4(lerp(Sky, float3(1.0, 1.0, 1.0), 0.25) * ripple * a_ + acc.rgb * (1.0 - a_), a_ + acc.a * (1.0 - a_));
	float snow = Cover * dry * smoothstep(0.55, 0.9, ny) * (0.72 + 0.28 * small) * (1.0 - 0.45 * m.b);
	a_ = snow * 0.82;
	acc = float4(SnowColor * a_ + acc.rgb * (1.0 - a_), a_ + acc.a * (1.0 - a_));
}
acc *= 1.0 - fogF; // the cover's own FOG_FADE (coverMaterial: a = acc.a * (1.0 - fogF))
if (CloudOn > 0.5) {
	// CLOUD_SCALE 260 m per noise tile, the second layer at 0.53 of it
	float a = Noise.Sample(NoiseSampler, xz / 260.0 + DriftA.xy).r;
	float b = Noise.Sample(NoiseSampler, xz / 137.8 + DriftB.xy).r;
	float cover = smoothstep(0.46, 0.7, a * 0.72 + b * 0.28);
	a_ = cover * Strength * (1.0 - smoothstep(FadeRadii.x, FadeRadii.y, length(xz - FadeCentre.xy))) * (1.0 - fogF);
	acc = float4(Tint * a_ + acc.rgb * (1.0 - a_), a_ + acc.a * (1.0 - a_));
}
''' + FOG_HLSL + '''
// round 14: past the map's edge (the apron's copy of this layer) the reference's apron fog overlay (apron.js fogMesh,
// surfaces.js fogOverlayShader) lies over the cover and the cloud shade too: the edge cell's look eases to never seen
// between 6 and 32 m out, as M_TCApron dims the apron's own ground
if (FogOn > 0.5) fogK = lerp(fogK, 82.0 / 255.0, smoothstep(6.0, 32.0, length(xz - clamp(xz, float2(0.0, 0.0), FogSize.xy))));
float3 Col = acc.rgb / max(acc.a, 1e-4) * fogK;
float A = acc.a < 0.004 ? 0.0 : acc.a;
''' + DISPLAY_HLSL + '''
return float4(Col, alphaLin);
'''

MIST_HLSL = '''// client/atmosphere.js mistMaterial: a small distortion inside the wisp texture, faded near the eye
float2 xz = WorldPos.xy * 0.01;
float2 uv = UV0 + 0.018 * float2(sin(xz.y * 0.2 + Time * 0.13), sin(xz.x * 0.17 - Time * 0.11));
float vNear = smoothstep(6.0, 18.0, Depth * 0.01);
// three's CanvasTexture is flipped (v = 1 is the canvas's top row); this texture's row 0 is the canvas's top
float a = Wisp.Sample(WispSampler, float2(uv.x, 1.0 - uv.y)).a * Opacity * vNear;
''' + FOG_HLSL + '''
float3 Col = Tint * fogK;
float A = a < 0.004 ? 0.0 : a;
''' + DISPLAY_HLSL + '''
return float4(Col, alphaLin);
'''

# The point: its place in the box (the mesh's local position, cm, Unreal axes; three's position x uBox), aRand in UV0
# and UV1. Everything below is weatherPoints' vertex shader in reference metres.
FALL_POINT = '''float4 r = float4(UV0.x, UV0.y, UV1.x, UV1.y);
float3 p = float3(Local.x, Local.z, Local.y) * 0.01 + Vel * Time * (0.75 + 0.5 * r.x);
p.x += sin(Time * (0.5 + r.y) + r.z * 6.283) * Sway;
p.z += cos(Time * (0.4 + r.x) + r.w * 6.283) * Sway;
float3 lp = p - BoxMin;
float3 local = lp - Box * floor(lp / Box);          // GLSL mod
float3 wp = BoxMin + local;                         // reference metres
float3 wu = float3(wp.x, wp.z, wp.y) * 100.0;       // Unreal cm
float depth = max(dot(wu - CamPos, CamFwd) * 0.01, 1e-3); // -mvPosition.z
'''

FALL_WPO_HLSL = '''// client/atmosphere.js weatherPoints vertex shader: the point wraps inside a box that follows the camera target and is
// drawn camera-facing, gl_PointSize = clamp(size x uScale / depth, uPx.x, uPx.y) pixels
''' + FALL_POINT + '''
float h = local.y / Box.y;
OutAlpha = Alpha * smoothstep(0.0, 0.12, h) * (1.0 - smoothstep(0.8, 1.0, h))
	* (1.0 - smoothstep(FadeRadii.x, FadeRadii.y, length(wp.xz - FadeCentre.xy)))
	* smoothstep(4.0, 14.0, depth);
float px = clamp(lerp(SizeM.x, SizeM.y, r.w) * Scale / depth, Px.x, Px.y);
float size = px * depth / Scale * 100.0;            // the sprite's side in cm at that depth
float2 corner = Corner.rg;                          // gl_PointCoord at this vertex (y grows down the screen)
float3 offs = (CamRight * (corner.x - 0.5) - CamUp * (corner.y - 0.5)) * size;
// the component stands at the box's corner: its local position is relative to it
float3 target = (float3(local.x, local.z, local.y) * 100.0) + offs;
return target - Local;
'''

FALL_PS_HLSL = '''// client/atmosphere.js weatherPoints fragment shader: rain as a short slanted streak, snow and dust as soft dots.
// The points draw before the fog of war overlay (render order 0.8 under 1), which darkens them with the ground.
float2 c = Corner.rg - 0.5;
float soft = 1.0 - smoothstep(0.06, 0.25, dot(c, c));
float streak = (1.0 - smoothstep(0.02, 0.07, abs(c.x - c.y * 0.18))) * (1.0 - smoothstep(0.2, 0.5, abs(c.y)));
float a = VAlpha * lerp(soft, streak, Streak);
float2 xz = WorldPos.xy * 0.01;
''' + FOG_HLSL + '''
float3 Col = Color * fogK;
float A = a < 0.01 ? 0.0 : a;
''' + DISPLAY_HLSL + '''
return float4(Col, alphaLin);
'''

BIRD_HLSL = '''// client/atmosphere.js birdMesh fragment shader: vAlpha = smoothstep(10, 22, -mvPosition.z); drawn before the fog of
// war overlay (render order 0.9 under 1), which darkens a bird over unexplored ground
float2 xz = WorldPos.xy * 0.01;
''' + FOG_HLSL + '''
float3 Col = Color * fogK;
float A = smoothstep(10.0, 22.0, Depth * 0.01);
''' + DISPLAY_HLSL + '''
return float4(Col, alphaLin);
'''


def sha(text):
    return hashlib.sha256(text.encode('utf-8')).hexdigest()


def file_sha(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        h.update(f.read())
    return h.hexdigest()


class Maker(object):
    def __init__(self):
        self.tools = unreal.AssetToolsHelpers.get_asset_tools()
        self.eal = unreal.EditorAssetLibrary
        self.mel = unreal.MaterialEditingLibrary
        self.report = dict(format='tc-atmosphere-materials-v1', tool_version=TOOL_VERSION, reference='e4e579a838aabae3b1d9dadbc17a0a87147bc52d', materials={})

    def create(self, name):
        path = '%s/Materials/%s' % (ROOT, name)
        if self.eal.does_asset_exist(path):
            asset = self.eal.load_asset(path)
            if asset and isinstance(asset, unreal.Material):
                self.mel.delete_all_material_expressions(asset)
                return asset
            self.eal.delete_asset(path)
        mat = self.tools.create_asset(name, ROOT + '/Materials', unreal.Material, unreal.MaterialFactoryNew())
        return mat

    def expr(self, mat, cls, x, y, **props):
        e = self.mel.create_material_expression(mat, cls, x, y)
        for k, v in props.items():
            e.set_editor_property(k, v)
        return e

    def custom(self, mat, code, inputs, ret, x, y, desc, outputs=()):
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
        for n, (src, pin) in inputs:
            if not self.mel.connect_material_expressions(src, pin, c, n):
                raise RuntimeError('%s: could not connect %s' % (desc, n))
        return c

    def base(self, name, two_sided):
        mat = self.create(name)
        mat.set_editor_property('shading_model', unreal.MaterialShadingModel.MSM_UNLIT)
        mat.set_editor_property('blend_mode', unreal.BlendMode.BLEND_TRANSLUCENT)
        mat.set_editor_property('two_sided', two_sided)
        mat.set_editor_property('tangent_space_normal', False)
        return mat

    def params(self, mat, X):
        y = [-600]

        def row():
            y[0] += 100
            return y[0]

        def scalar(n, v):
            return (self.expr(mat, unreal.MaterialExpressionScalarParameter, X, row(), parameter_name=n, default_value=v), '')

        def vector(n, v):
            return (self.expr(mat, unreal.MaterialExpressionVectorParameter, X, row(), parameter_name=n,
                              default_value=unreal.LinearColor(v[0], v[1], v[2], 1.0)), '')

        def texobj(n, t):
            return (self.expr(mat, unreal.MaterialExpressionTextureObjectParameter, X, row(), parameter_name=n, texture=t), '')

        def node(cls, **kw):
            return (self.expr(mat, cls, X, row(), **kw), '')
        return scalar, vector, texobj, node

    def split(self, mat, src, x, y):
        rgb = self.expr(mat, unreal.MaterialExpressionComponentMask, x, y, r=True, g=True, b=True, a=False)
        alpha = self.expr(mat, unreal.MaterialExpressionComponentMask, x, y + 150, r=False, g=False, b=False, a=True)
        self.mel.connect_material_expressions(src, '', rgb, '')
        self.mel.connect_material_expressions(src, '', alpha, '')
        return rgb, alpha

    def finish(self, mat, name, hlsl, sources):
        self.mel.recompile_material(mat)
        self.eal.save_loaded_asset(mat, only_if_is_dirty=False)
        self.report['materials'][name] = dict(asset='%s/Materials/%s' % (ROOT, name), hlsl_sha256=sha(hlsl), ported_from=sources)

    def fog_inputs(self, scalar, vector, texobj, white):
        return [('FogMap', texobj('FogMap', white)), ('FogSize', vector('FogSize', (1.0, 1.0, 0.0))), ('FogOn', scalar('FogOn', 0.0))]

    def display_inputs(self, scalar, node):
        # the opaque scene behind (linear, pre-exposed) and the mood's exposure (client/moods.js exposure)
        return [('Scene', node(unreal.MaterialExpressionSceneColor)), ('Exposure', scalar('Exposure', 0.9)), ('LinearBlend', scalar('LinearBlend', 1.0))]

    def ground(self, white, grey):
        P = unreal.MaterialProperty
        T = unreal.CustomMaterialOutputType
        mat = self.base('M_TCAtmosGround', False)
        # cover/cloud draw before the shared main.js and apron.js fog overlay.
        mat.set_editor_property('translucency_pass', unreal.MaterialTranslucencyPass.MTP_BEFORE_DOF)
        scalar, vector, texobj, node = self.params(mat, -1600)
        ins = [('WorldPos', node(unreal.MaterialExpressionWorldPosition)), ('Normal', node(unreal.MaterialExpressionVertexNormalWS)),
               ('CamPos', vector('CamPos', (0.0, 0.0, 10000.0))), ('Noise', texobj('Noise', grey)), ('Mask', texobj('Mask', white)),
               ('MapSize', vector('MapSize', (100.0, 100.0, 0.0))), ('CoverOn', scalar('CoverOn', 0.0)), ('CloudOn', scalar('CloudOn', 0.0)),
               ('Wet', scalar('Wet', 0.0)), ('Sheen', scalar('Sheen', 0.0)), ('Mud', scalar('Mud', 0.0)), ('Cover', scalar('Cover', 0.0)),
               ('Time', scalar('Time', 0.0)), ('Sky', vector('Sky', (0.5, 0.5, 0.5))), ('MudColor', vector('MudColor', (0.07, 0.042, 0.019))),
               ('SnowColor', vector('SnowColor', (0.85, 0.88, 0.9))), ('DriftA', vector('DriftA', (0.0, 0.0, 0.0))), ('DriftB', vector('DriftB', (0.37, 0.61, 0.0))),
               ('Strength', scalar('Strength', 0.15)), ('Tint', vector('Tint', (0.011, 0.016, 0.029))),
               ('FadeCentre', vector('FadeCentre', (0.0, 0.0, 0.0))), ('FadeRadii', vector('FadeRadii', (1e4, 2e4, 0.0))),
               # round 14: the reference's FOG_FADE on this layer (light.js followView's fog distances, metres)
               ('Depth', node(unreal.MaterialExpressionPixelDepth)), ('FogNear', scalar('FogNear', 1e4)), ('FogFar', scalar('FogFar', 2e4))]
        ins += self.fog_inputs(scalar, vector, texobj, white) + self.display_inputs(scalar, node)
        c = self.custom(mat, GROUND_HLSL, ins, T.CMOT_FLOAT4, -800, 0, 'client/atmosphere.js coverMaterial + cloudMaterial')
        rgb, alpha = self.split(mat, c, -450, 0)
        self.mel.connect_material_property(rgb, '', P.MP_EMISSIVE_COLOR)
        self.mel.connect_material_property(alpha, '', P.MP_OPACITY)
        self.finish(mat, 'M_TCAtmosGround', GROUND_HLSL, ['client/atmosphere.js noiseTexture, cloudMaterial, groundMask, coverMaterial'])

    def mist(self, white):
        P = unreal.MaterialProperty
        T = unreal.CustomMaterialOutputType
        mat = self.base('M_TCAtmosMist', True)
        scalar, vector, texobj, node = self.params(mat, -1600)
        ins = [('UV0', node(unreal.MaterialExpressionTextureCoordinate, coordinate_index=0)), ('WorldPos', node(unreal.MaterialExpressionWorldPosition)),
               ('Depth', node(unreal.MaterialExpressionPixelDepth)), ('Wisp', texobj('Wisp', white)), ('Time', scalar('Time', 0.0)),
               ('Opacity', scalar('Opacity', 0.28)), ('Tint', vector('Tint', (0.84, 0.86, 0.83)))]
        ins += self.fog_inputs(scalar, vector, texobj, white) + self.display_inputs(scalar, node)
        c = self.custom(mat, MIST_HLSL, ins, T.CMOT_FLOAT4, -800, 0, 'client/atmosphere.js mistMaterial')
        rgb, alpha = self.split(mat, c, -450, 0)
        self.mel.connect_material_property(rgb, '', P.MP_EMISSIVE_COLOR)
        self.mel.connect_material_property(alpha, '', P.MP_OPACITY)
        self.finish(mat, 'M_TCAtmosMist', MIST_HLSL, ['client/atmosphere.js wispTexture, mistMaterial'])

    def fall(self):
        P = unreal.MaterialProperty
        T = unreal.CustomMaterialOutputType
        mat = self.base('M_TCAtmosFall', True)
        scalar, vector, texobj, node = self.params(mat, -1800)
        local_cls = getattr(unreal, 'MaterialExpressionLocalPosition', None) or unreal.MaterialExpressionPreSkinnedPosition
        local = node(local_cls)
        if hasattr(unreal, 'PositionIncludedOffsets'):
            # the point's own place, before this material's offset
            local[0].set_editor_property('included_offsets', unreal.PositionIncludedOffsets.EXCLUDE_OFFSETS)
        corner = node(unreal.MaterialExpressionVertexColor)
        uv0 = node(unreal.MaterialExpressionTextureCoordinate, coordinate_index=0)
        uv1 = node(unreal.MaterialExpressionTextureCoordinate, coordinate_index=1)
        common = [('Local', local), ('Corner', corner), ('UV0', uv0), ('UV1', uv1), ('Time', scalar('Time', 0.0)),
                  ('Box', vector('Box', (150.0, 36.0, 150.0))), ('BoxMin', vector('BoxMin', (0.0, 0.0, 0.0))), ('Vel', vector('Vel', (1.0, -21.0, 0.5))),
                  ('Sway', scalar('Sway', 0.1)), ('SizeM', vector('SizeM', (0.55, 1.0, 0.0))), ('Px', vector('Px', (1.0, 16.0, 0.0))),
                  ('Scale', scalar('Scale', 938.0)), ('CamPos', vector('CamPos', (0.0, 0.0, 0.0))), ('CamFwd', vector('CamFwd', (1.0, 0.0, 0.0))),
                  ('CamRight', vector('CamRight', (0.0, 1.0, 0.0))), ('CamUp', vector('CamUp', (0.0, 0.0, 1.0))),
                  ('FadeCentre', vector('FadeCentre', (0.0, 0.0, 0.0))), ('FadeRadii', vector('FadeRadii', (50.0, 90.0, 0.0))), ('Alpha', scalar('Alpha', 0.42))]
        wpo = self.custom(mat, FALL_WPO_HLSL, common, T.CMOT_FLOAT3, -900, 0, 'client/atmosphere.js weatherPoints vertex', outputs=[('OutAlpha', T.CMOT_FLOAT1)])
        self.mel.connect_material_property(wpo, '', P.MP_WORLD_POSITION_OFFSET)
        vi = self.expr(mat, unreal.MaterialExpressionVertexInterpolator, -600, 400)
        if not (self.mel.connect_material_expressions(wpo, 'OutAlpha', vi, 'VS') or self.mel.connect_material_expressions(wpo, 'OutAlpha', vi, '')):
            raise RuntimeError('vertex interpolator')
        white = unreal.load_asset('/Engine/EngineResources/WhiteSquareTexture')
        ps_ins = [('Corner', corner), ('VAlpha', (vi, '')), ('Streak', scalar('Streak', 1.0)), ('Color', vector('Color', (0.55, 0.6, 0.66))),
                  ('WorldPos', node(unreal.MaterialExpressionWorldPosition))] + self.fog_inputs(scalar, vector, texobj, white) + self.display_inputs(scalar, node)
        ps = self.custom(mat, FALL_PS_HLSL, ps_ins, T.CMOT_FLOAT4, -400, 400, 'client/atmosphere.js weatherPoints fragment')
        rgb, alpha = self.split(mat, ps, -200, 400)
        self.mel.connect_material_property(rgb, '', P.MP_EMISSIVE_COLOR)
        self.mel.connect_material_property(alpha, '', P.MP_OPACITY)
        self.finish(mat, 'M_TCAtmosFall', FALL_WPO_HLSL + FALL_PS_HLSL, ['client/atmosphere.js FALLS, weatherPoints'])

    def bird(self):
        P = unreal.MaterialProperty
        T = unreal.CustomMaterialOutputType
        mat = self.base('M_TCAtmosBird', True)
        scalar, vector, texobj, node = self.params(mat, -1200)
        white = unreal.load_asset('/Engine/EngineResources/WhiteSquareTexture')
        ins = [('Depth', node(unreal.MaterialExpressionPixelDepth)), ('Color', vector('Color', (0.024, 0.019, 0.016))),
               ('WorldPos', node(unreal.MaterialExpressionWorldPosition))] + self.fog_inputs(scalar, vector, texobj, white) + self.display_inputs(scalar, node)
        c = self.custom(mat, BIRD_HLSL, ins, T.CMOT_FLOAT4, -700, 0, 'client/atmosphere.js birdMesh fragment')
        rgb, alpha = self.split(mat, c, -400, 0)
        self.mel.connect_material_property(rgb, '', P.MP_EMISSIVE_COLOR)
        self.mel.connect_material_property(alpha, '', P.MP_OPACITY)
        self.finish(mat, 'M_TCAtmosBird', BIRD_HLSL, ['client/atmosphere.js birdMesh'])


def content_dir():
    return unreal.Paths.convert_relative_path_to_full(unreal.Paths.project_content_dir())


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--verify', default='')
    args = ap.parse_args([a for a in sys.argv[1:] if a])
    if unreal is None:
        raise SystemExit('run inside the editor (UnrealEditor-Cmd -run=pythonscript)')
    out = os.path.join(content_dir(), 'Atmosphere', 'AtmosphereMaterials.json')
    if args.verify:
        report = dict(ok=True, missing=[], sha_mismatch=[])
        with open(out) as f:
            want = json.load(f)
        for name, m in want['materials'].items():
            path = os.path.join(content_dir(), 'Atmosphere', 'Materials', name + '.uasset')
            if not os.path.exists(path) or not unreal.EditorAssetLibrary.load_asset(m['asset']):
                report['missing'].append(name)
            elif file_sha(path) != m.get('uasset_sha256'):
                report['sha_mismatch'].append(name)
        report['ok'] = not report['missing'] and not report['sha_mismatch']
        with open(args.verify, 'w') as f:
            json.dump(report, f, indent=1, sort_keys=True)
        unreal.log('atmosphere materials verify: %s' % json.dumps(report))
        return
    mk = Maker()
    white = unreal.load_asset('/Engine/EngineResources/WhiteSquareTexture')
    grey = unreal.load_asset('/Engine/EngineResources/DefaultTexture') or white
    mk.ground(white, grey)
    mk.mist(white)
    mk.fall()
    mk.bird()
    for name, m in mk.report['materials'].items():
        path = os.path.join(content_dir(), 'Atmosphere', 'Materials', name + '.uasset')
        m['uasset_sha256'] = file_sha(path) if os.path.exists(path) else None
    with open(out, 'w') as f:
        json.dump(mk.report, f, indent=1, sort_keys=True)
        f.write('\n')
    unreal.log('atmosphere materials: %d made, report %s' % (len(mk.report['materials']), out))


main()
