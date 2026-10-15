"""Generate the HQ cloth from the pinned browser's flagMat canvas.

LOOKFX_CLOTH names capture-browser.mjs --cloth-out's PNG. Run only on a scratch project, then hand off the assets.
The flag is Lambert, double sided, no environment reflection, and has no vertex wave at e4e579a.
"""
import hashlib
import json
import os
import unreal

E = unreal.EditorAssetLibrary
M = unreal.MaterialEditingLibrary
P = unreal.MaterialProperty
root = '/Game/Effects'
cloth = os.environ['LOOKFX_CLOTH']
task = unreal.AssetImportTask()
for key, value in dict(filename=cloth, destination_path=root + '/Textures', destination_name='T_HQCloth',
                       automated=True, replace_existing=True, save=False).items():
    task.set_editor_property(key, value)
unreal.AssetToolsHelpers.get_asset_tools().import_asset_tasks([task])
texture = E.load_asset(root + '/Textures/T_HQCloth')
assert texture
texture.set_editor_property('srgb', True)  # CanvasTexture.colorSpace = SRGBColorSpace
texture.set_editor_property('compression_settings', unreal.TextureCompressionSettings.TC_DEFAULT)
texture.set_editor_property('address_x', unreal.TextureAddress.TA_CLAMP)
texture.set_editor_property('address_y', unreal.TextureAddress.TA_CLAMP)
texture.set_editor_property('never_stream', True)
E.save_loaded_asset(texture)
path = root + '/Materials/M_TCHQCloth'
if E.does_asset_exist(path):
    assert E.delete_asset(path)
mat = unreal.AssetToolsHelpers.get_asset_tools().create_asset('M_TCHQCloth', root + '/Materials', unreal.Material, unreal.MaterialFactoryNew())
mat.set_editor_property('two_sided', True)
mat.set_editor_property('shading_model', unreal.MaterialShadingModel.MSM_DEFAULT_LIT)


def expr(cls, **props):
    node = M.create_material_expression(mat, cls)
    for k, v in props.items():
        node.set_editor_property(k, v)
    return node


def custom(code, inputs, description, output=unreal.CustomMaterialOutputType.CMOT_FLOAT3):
    node = expr(unreal.MaterialExpressionCustom, code=code, description=description,
                output_type=output)
    ports = []
    for name, source, output in inputs:
        port = unreal.CustomInput()
        port.set_editor_property('input_name', name)
        ports.append(port)
    node.set_editor_property('inputs', ports)
    for name, source, output in inputs:
        assert M.connect_material_expressions(source, output, node, name)
    return node


color = expr(unreal.MaterialExpressionVectorParameter, parameter_name='Color', default_value=unreal.LinearColor(1, 1, 1, 1))
sample = expr(unreal.MaterialExpressionTextureSampleParameter2D, parameter_name='Cloth', texture=texture)
# Unreal V runs down the image. The procedural Three plane's V runs up, and CanvasTexture.flipY is true.
uv = expr(unreal.MaterialExpressionTextureCoordinate, coordinate_index=0, un_mirror_v=False)
flip = custom('return float2(UV.x, 1.0-UV.y);', [('UV', uv, '')], 'flagMat canvas UV',
              unreal.CustomMaterialOutputType.CMOT_FLOAT2)
assert M.connect_material_expressions(flip, '', sample, 'UVs')
albedo = expr(unreal.MaterialExpressionMultiply)
assert M.connect_material_expressions(sample, 'RGB', albedo, 'A')
assert M.connect_material_expressions(color, 'RGB', albedo, 'B')
mpc = E.load_asset('/Game/Models/Materials/Look/MPC_TCLook')
normal = expr(unreal.MaterialExpressionPixelNormalWS)
sky = [expr(unreal.MaterialExpressionCollectionParameter, collection=mpc, parameter_name=p) for p in ('HemiTop', 'HemiBottom')]
ambient = custom('return Color * lerp(Bottom.rgb, Top.rgb, saturate(Normal.z * 0.5 + 0.5)) / 3.141592653589793;',
                 [('Color', albedo, ''), ('Normal', normal, ''), ('Top', sky[0], ''), ('Bottom', sky[1], '')],
                 'flagMat Lambert hemisphere')
zero = expr(unreal.MaterialExpressionConstant, r=0)
one = expr(unreal.MaterialExpressionConstant, r=1)
for node, prop in [(albedo, P.MP_BASE_COLOR), (ambient, P.MP_EMISSIVE_COLOR), (zero, P.MP_SPECULAR),
                   (zero, P.MP_METALLIC), (one, P.MP_ROUGHNESS), (zero, P.MP_AMBIENT_OCCLUSION)]:
    assert M.connect_material_property(node, '', prop)
if hasattr(M, 'set_base_material_usage'):
    M.set_base_material_usage(mat, unreal.MaterialUsage.MATUSAGE_INSTANCED_STATIC_MESHES)
else:
    M.set_material_usage(mat, unreal.MaterialUsage.MATUSAGE_INSTANCED_STATIC_MESHES)
errors = M.recompile_material(mat)
assert not errors, str(errors)
E.save_loaded_asset(mat)
manifest = {'clothSha256': hashlib.sha256(open(cloth, 'rb').read()).hexdigest(),
            'source': 'e4e579a client/markers.js flagMat, exported original canvas',
            'assets': [texture.get_path_name(), mat.get_path_name()], 'srgb': True, 'wave': False}
if os.environ.get('LOOKFX_ASSET_LOG'):
    with open(os.environ['LOOKFX_ASSET_LOG'], 'w') as f:
        json.dump(manifest, f, indent=2)
unreal.log('lookfx cloth ' + json.dumps(manifest))
