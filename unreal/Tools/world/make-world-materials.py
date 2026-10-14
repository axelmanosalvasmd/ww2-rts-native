"""The world materials: what the map pieces, scenery props and foliage draw with (World/TCWorldObjects.h,
FTCWorldObjectsView). Generated, never hand-edited: run inside the editor, after unreal/Tools/models/import-models.py.

  UnrealEditor-Cmd <a project whose Content/Models and Content/World are this repo's> -run=pythonscript \
    -EnablePlugins=PythonScriptPlugin -unattended -nop4 -nosplash -nullrhi \
    -script="unreal/Tools/world/make-world-materials.py [--verify REPORT.json]"

Why they exist. The imported materials (Content/Models/Materials, import-models.py) reproduce each reference material,
but read no per-instance colour: client/structures.js put() and client/props.js place() give every instance its own
linear tint, which multiplies the material (an InstancedMesh's instanceColor). Nor do they sample the fog: the
reference mixes every piece toward the fog overlay's shade per pixel (client/surfaces.js fogShader). And the roof's
tiles keep their real size on any roof (client/structures.js roofShader). These masters are the importer's own formulas
plus those three things; each instance copies its parameters from the imported instance it stands for, so the look
follows the import.

Output under /Game/World (unreal/Content/World):
  Materials/M_TCWorldSurface   client/surfaces.js planar(): world-planar texture times Tint, top faces tinted, a
                               'painted' surface times its vertex colours (the importer's SURFACE_HLSL, unchanged)
  Materials/M_TCWorldLambert   MeshLambertMaterial: colour, vertex colours, an optional map (with the roof's tile scale)
  Materials/M_TCWorldLeaf      foliage.js leafMaterial(): the same, alpha-tested and two-sided, both faces lit by the
                               crown normal, its alpha raised with the mip level (the reference's onBeforeCompile)
  Materials/Instances/MI_TCWorld_<slot>  one per imported material a piece, prop or foliage model uses
  WorldMaterials.json          slot -> instance, master, copied parameters; master HLSL sha256; uasset sha256

Per-instance custom data (TCWorldObjects.cpp WorldFloats): 0..2 tint (linear), 3 bark grey (props.js c.bark: the
bark material reads it instead of the tint), 4..5 the roof's tile scale (ridge length / 1.6, slope length / 1.6).
Fog: FogMap (one texel per cell, linear 0..1, nearest), FogSize (map metres), FogOn; the colour is multiplied by the
cell's texel, as ATCTerrain dims the ground (1 in sight, 0.55 explored, 0.32 never seen).
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
ROOT = '/Game/World'
TOOL_VERSION = 2

FOG_HLSL = '''
float fog = 1.0;
if (FogOn > 0.5) {
	// one texel per cell: Unreal X is the reference x, Unreal Y the reference z (cm)
	float2 fuv = float2(WorldPos.x, WorldPos.y) * 0.01 / max(FogSize.xy, float2(1.0, 1.0));
	fog = FogMap.SampleLevel(FogMapSampler, fuv, 0).r;
}
'''

SURFACE_HLSL = '''// client/surfaces.js planar(): world-planar coordinates in metres (three axes), the texture times tint, top faces tinted.
float3 gw = float3(WorldPos.x, WorldPos.z, WorldPos.y) * 0.01;
float3 gn = normalize(float3(WorldNormal.x, WorldNormal.z, WorldNormal.y));
float3 an = abs(gn);
float2 uv = (an.y > 0.5 ? gw.xz : an.x > an.z ? gw.zy : gw.xy) * UVScale;
float top = smoothstep(0.5, 0.9, gn.y);
float3 c = Tex.Sample(TexSampler, float2(uv.x, -uv.y)).rgb * Tint;
if (Painted > 0.5) {
	float3 s = saturate(VertexColor.rgb);
	c *= s * Scale.x;
}
c *= lerp(float3(1.0, 1.0, 1.0), TopTint, top);
// structures.js put() / props.js place(): the instance's own tint
c *= Inst;
''' + FOG_HLSL + '''
return c * fog;
'''

LAMBERT_HLSL = '''// MeshLambertMaterial: colour times vertex colours (linear as read; UV3.x scales colours above 1) times the map
float3 c = MaterialColor * saturate(VertexColor.rgb) * Scale.x;
if (MapOn > 0.5) {
	float2 u = UV0;
	// structures.js roofShader: uv times the instance's ridge and slope lengths, in 1.6 m tiles (custom data 4, 5)
	if (RoofUV > 0.5) u *= Roof;
	c *= Map.Sample(MapSampler, float2(u.x, MapFlipY > 0.5 ? 1.0 - u.y : u.y)).rgb;
}
// props.js: bark takes the grey c.bark (custom data 3), everything else the instance's colour (0..2)
c *= UseGrey > 0.5 ? float3(Grey, Grey, Grey) : Inst;
''' + FOG_HLSL + '''
return c * fog;
'''

LEAF_OPACITY_HLSL = '''// foliage.js leafMaterial: alpha-tested; mipmaps average leaf alpha toward nothing, so it is raised with the mip level
float2 u = float2(UV0.x, MapFlipY > 0.5 ? 1.0 - UV0.y : UV0.y);
float a = Map.Sample(MapSampler, u).a;
float2 dx = ddx(UV0 * 1024.0), dy = ddy(UV0 * 1024.0);
a *= 1.0 + 0.3 * max(0.0, 0.5 * log2(max(max(dot(dx, dx), dot(dy, dy)), 1e-8)));
return a;
'''


def sha(text):
    return hashlib.sha256(text.encode('utf-8')).hexdigest()


class Maker(object):
    def __init__(self):
        self.tools = unreal.AssetToolsHelpers.get_asset_tools()
        self.eal = unreal.EditorAssetLibrary
        self.mel = unreal.MaterialEditingLibrary
        self.report = dict(format='tc-world-materials-v1', tool_version=TOOL_VERSION, masters={}, instances={})

    def create(self, sub, name, cls, factory):
        path = '%s/%s/%s' % (ROOT, sub, name) if sub else '%s/%s' % (ROOT, name)
        if self.eal.does_asset_exist(path):
            asset = self.eal.load_asset(path)
            if asset and isinstance(asset, cls):
                return asset
            self.eal.delete_asset(path)
        return self.tools.create_asset(name, '%s/%s' % (ROOT, sub) if sub else ROOT, cls, factory)

    def expr(self, mat, cls, x, y, **props):
        e = self.mel.create_material_expression(mat, cls, x, y)
        for k, v in props.items():
            e.set_editor_property(k, v)
        return e

    def custom(self, mat, code, inputs, ret, x, y, desc):
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
        for n, src in inputs:
            if not self.mel.connect_material_expressions(src, '', c, n):
                raise RuntimeError('could not connect %s' % n)
        return c

    def master(self, name, kind):
        mat = self.create('Materials', name, unreal.Material, unreal.MaterialFactoryNew())
        self.mel.delete_all_material_expressions(mat)
        leaf = kind == 'leaf'
        mat.set_editor_property('blend_mode', unreal.BlendMode.BLEND_MASKED if leaf else unreal.BlendMode.BLEND_OPAQUE)
        mat.set_editor_property('two_sided', leaf)
        mat.set_editor_property('tangent_space_normal', False)
        mat.set_editor_property('shading_model', unreal.MaterialShadingModel.MSM_DEFAULT_LIT)
        T = unreal.CustomMaterialOutputType
        y = [0]
        X = -1400

        def row():
            y[0] += 100
            return y[0]

        def scalar(n, v):
            return self.expr(mat, unreal.MaterialExpressionScalarParameter, X, row(), parameter_name=n, default_value=v)

        def vector(n, v):
            return self.expr(mat, unreal.MaterialExpressionVectorParameter, X, row(), parameter_name=n, default_value=unreal.LinearColor(v[0], v[1], v[2], 1.0))

        def texobj(n, t):
            return self.expr(mat, unreal.MaterialExpressionTextureObjectParameter, X, row(), parameter_name=n, texture=t)

        def uv(i):
            return self.expr(mat, unreal.MaterialExpressionTextureCoordinate, X, row(), coordinate_index=i)

        def cpd3(index, default):
            e = self.expr(mat, unreal.MaterialExpressionPerInstanceCustomData3Vector, X, row(), data_index=index)
            e.set_editor_property('const_default_value', unreal.LinearColor(default[0], default[1], default[2], 1.0))
            return e

        def cpd(index, default):
            e = self.expr(mat, unreal.MaterialExpressionPerInstanceCustomData, X, row(), data_index=index)
            e.set_editor_property('const_default_value', default)
            return e

        white = unreal.load_asset('/Engine/EngineResources/WhiteSquareTexture')
        world_pos = self.expr(mat, unreal.MaterialExpressionWorldPosition, X, row())
        fog = [('WorldPos', world_pos), ('FogMap', texobj('FogMap', white)), ('FogSize', vector('FogSize', (1.0, 1.0, 0.0))), ('FogOn', scalar('FogOn', 0.0))]
        inst = ('Inst', cpd3(0, (1.0, 1.0, 1.0)))
        if kind == 'surface':
            tex = unreal.load_asset('/Game/Models/Textures/Surfaces/T_Surface_plaster')
            inputs = fog + [inst, ('WorldNormal', self.expr(mat, unreal.MaterialExpressionVertexNormalWS, X, row())),
                            ('VertexColor', self.expr(mat, unreal.MaterialExpressionVertexColor, X, row())), ('Scale', uv(3)),
                            ('Tex', texobj('Surface', tex or white)), ('UVScale', scalar('UVScale', 1.0)), ('Tint', vector('Tint', (1.0, 1.0, 1.0))),
                            ('TopTint', vector('TopTint', (1.0, 1.0, 1.0))), ('Painted', scalar('Painted', 0.0))]
            code = SURFACE_HLSL
        else:
            roof = self.expr(mat, unreal.MaterialExpressionAppendVector, X + 250, y[0])
            self.mel.connect_material_expressions(cpd(4, 1.0), '', roof, 'A')
            self.mel.connect_material_expressions(cpd(5, 1.0), '', roof, 'B')
            self.map_uv0 = uv(0)
            self.map_tex = texobj('Map', white)
            self.map_flip = scalar('MapFlipY', 1.0)
            inputs = fog + [inst, ('Grey', cpd(3, 1.0)), ('Roof', roof), ('VertexColor', self.expr(mat, unreal.MaterialExpressionVertexColor, X, row())),
                            ('Scale', uv(3)), ('UV0', self.map_uv0), ('Map', self.map_tex), ('MapOn', scalar('MapOn', 0.0)), ('MapFlipY', self.map_flip),
                            ('RoofUV', scalar('RoofUV', 0.0)), ('UseGrey', scalar('UseGrey', 0.0)), ('MaterialColor', vector('MaterialColor', (1.0, 1.0, 1.0)))]
            code = LAMBERT_HLSL
        c = self.custom(mat, code, inputs, T.CMOT_FLOAT3, -600, 0, name)
        P = unreal.MaterialProperty
        if not self.mel.connect_material_property(c, '', P.MP_BASE_COLOR):
            raise RuntimeError('base colour')
        # Lambert: fully rough, no specular, not metal
        self.mel.connect_material_property(self.expr(mat, unreal.MaterialExpressionConstant, -300, 200, r=1.0), '', P.MP_ROUGHNESS)
        self.mel.connect_material_property(self.expr(mat, unreal.MaterialExpressionConstant, -300, 300, r=0.0), '', P.MP_SPECULAR)
        self.mel.connect_material_property(self.expr(mat, unreal.MaterialExpressionConstant, -300, 400, r=0.0), '', P.MP_METALLIC)
        hlsl = code
        if leaf:
            # both faces take the crown normal (the reference replaces three's flipped back-face normal with vNormal).
            # With a world-space normal (tangent_space_normal off) Unreal flips neither the vertex normal nor the
            # material's normal on back faces (MaterialTemplate.ush flips only tangent-space normals), so the vertex
            # normal goes in as it is. Multiplying it by TwoSidedSign turned every back-facing card's normal inside out:
            # pines, grass and the back half of every crown went unlit.
            n = self.expr(mat, unreal.MaterialExpressionVertexNormalWS, -500, 500)
            self.mel.connect_material_property(n, '', P.MP_NORMAL)
            oc = self.custom(mat, LEAF_OPACITY_HLSL, [('UV0', self.map_uv0), ('Map', self.map_tex), ('MapFlipY', self.map_flip)], T.CMOT_FLOAT1, -600, 700, name + 'Opacity')
            self.mel.connect_material_property(oc, '', P.MP_OPACITY_MASK)
            mat.set_editor_property('opacity_mask_clip_value', 0.5)
            hlsl += LEAF_OPACITY_HLSL
        if hasattr(self.mel, 'set_base_material_usage'):
            self.mel.set_base_material_usage(mat, unreal.MaterialUsage.MATUSAGE_INSTANCED_STATIC_MESHES)
        else:
            self.mel.set_material_usage(mat, unreal.MaterialUsage.MATUSAGE_INSTANCED_STATIC_MESHES)
        self.mel.recompile_material(mat)
        self.eal.save_loaded_asset(mat, only_if_is_dirty=False)
        self.report['masters'][name] = dict(asset=mat.get_path_name().split('.')[0], kind=kind, hlsl_sha256=sha(hlsl))
        return mat

    def instance(self, name, parent, scalars, vectors, textures):
        mi = self.create('Materials/Instances', name, unreal.MaterialInstanceConstant, unreal.MaterialInstanceConstantFactoryNew())
        self.mel.set_material_instance_parent(mi, parent)
        self.mel.clear_all_material_instance_parameters(mi)
        for k, v in sorted(scalars.items()):
            self.mel.set_material_instance_scalar_parameter_value(mi, k, float(v))
        for k, v in sorted(vectors.items()):
            self.mel.set_material_instance_vector_parameter_value(mi, k, unreal.LinearColor(v[0], v[1], v[2], 1.0))
        for k, v in sorted(textures.items()):
            self.mel.set_material_instance_texture_parameter_value(mi, k, v)
        self.mel.update_material_instance(mi)
        self.eal.save_loaded_asset(mi, only_if_is_dirty=False)
        return mi


def catalog_slots(project_content):
    """The imported materials a piece, prop or foliage model uses, with the roles the world masters need."""
    cat = json.load(open(os.path.join(project_content, 'Models', 'ModelCatalog.json')))
    slots = {}
    for m in cat['models']:
        if m.get('category') not in ('piece', 'prop', 'foliage'):
            continue
        for part in m['parts']:
            for path in part['materials']:
                s = slots.setdefault(path, dict(path=path, users=[], roof=False))
                s['users'].append(m['key'])
                if m['key'] == 'piece/roof|-1':
                    s['roof'] = True
    return cat.get('reference_commit'), slots


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--verify', help='only check the generated assets against WorldMaterials.json; write a report here')
    args = ap.parse_args(sys.argv[1:] if len(sys.argv) > 1 else [])
    content = unreal.Paths.convert_relative_path_to_full(unreal.Paths.project_content_dir())
    reference, slots = catalog_slots(content)
    mel = unreal.MaterialEditingLibrary
    eal = unreal.EditorAssetLibrary
    if args.verify:
        rep = json.load(open(os.path.join(content, 'World', 'WorldMaterials.json')))
        problems = []
        for path in sorted(slots):
            name = 'MI_TCWorld_' + path.split('.')[-1][len('MI_TC'):]
            asset = '%s/Materials/Instances/%s' % (ROOT, name)
            if not eal.does_asset_exist(asset):
                problems.append('missing ' + asset)
                continue
            mi = eal.load_asset(asset)
            parent = mi.get_editor_property('parent')
            want = rep['instances'].get(name, {}).get('master')
            if not parent or parent.get_path_name().split('.')[0] != want:
                problems.append('%s: parent %s, want %s' % (name, parent.get_path_name() if parent else None, want))
        out = dict(ok=not problems, problems=problems, instances=len(slots))
        json.dump(out, open(args.verify, 'w'), indent=1)
        unreal.log('world materials verify: %s' % json.dumps(out))
        return
    mk = Maker()
    masters = {k: mk.master('M_TCWorld' + k.capitalize(), k) for k in ('surface', 'lambert', 'leaf')}
    for path, s in sorted(slots.items()):
        src = unreal.load_asset(path.split('.')[0])
        if not src:
            raise RuntimeError('imported material missing: %s (run import-models.py first)' % path)
        parent = src.get_editor_property('parent')
        pname = parent.get_name() if parent else ''
        name = 'MI_TCWorld_' + path.split('.')[-1][len('MI_TC'):]
        sv = lambda k: mel.get_material_instance_scalar_parameter_value(src, k)
        vv = lambda k: mel.get_material_instance_vector_parameter_value(src, k)
        tv = lambda k: mel.get_material_instance_texture_parameter_value(src, k)
        if pname == 'M_TCSurface':
            kind = 'surface'
            scalars = dict(UVScale=sv('UVScale'), Painted=sv('Painted'))
            vectors = dict(Tint=vv('Tint'), TopTint=vv('TopTint'))
            textures = dict(Surface=tv('Surface'))
        else:
            kind = 'leaf' if sv('AlphaTest') > 0 and sv('MapOn') > 0.5 else 'lambert'
            scalars = dict(MapOn=sv('MapOn'), MapFlipY=sv('MapFlipY'), RoofUV=1.0 if s['roof'] else 0.0, UseGrey=1.0 if 'barkMaterial' in name else 0.0)
            vectors = dict(MaterialColor=vv('MaterialColor'))
            textures = {}
            if sv('MapOn') > 0.5 and tv('Map'):
                textures['Map'] = tv('Map')
        vec = {k: (v.r, v.g, v.b) for k, v in vectors.items()}
        mi = mk.instance(name, masters[kind], scalars, vec, textures)
        mk.report['instances'][name] = dict(asset=mi.get_path_name().split('.')[0], source=path, source_parent=pname, master=masters[kind].get_path_name().split('.')[0],
                                            scalars=scalars, vectors=vec, textures={k: v.get_path_name() for k, v in textures.items()}, users=sorted(s['users']))
    mk.report['reference_commit'] = reference
    # uasset hashes, after every save
    for group in ('masters', 'instances'):
        for name, e in mk.report[group].items():
            f = os.path.join(content, e['asset'][len('/Game/'):] + '.uasset')
            e['uasset_sha256'] = hashlib.sha256(open(f, 'rb').read()).hexdigest() if os.path.exists(f) else None
    with open(os.path.join(content, 'World', 'WorldMaterials.json'), 'w') as fh:
        json.dump(mk.report, fh, indent=1, sort_keys=True)
    unreal.log('world materials: %d masters, %d instances' % (len(mk.report['masters']), len(mk.report['instances'])))


if __name__ == '__main__':
    if unreal is None:
        print('run inside the editor (see the docstring)')
        sys.exit(2)
    main()
