"""Run with UnrealEditor-Cmd -run=pythonscript -script=<this file>.

fx.js sectionMesh is MeshStandardMaterial({roughness:1}), with per-instance colour.
Use the established three.js indirect lighting shader from the model master. No textures or invented art.
"""
import unreal
E = unreal.EditorAssetLibrary
M = unreal.MaterialEditingLibrary
P = unreal.MaterialProperty
root = '/Game/Effects/Materials'
name = 'M_TCFxSection'
path = root + '/' + name
# Replace only this generated asset. Reusing a partially generated graph can retain old expressions and shader IDs.
if E.does_asset_exist(path):
    assert E.delete_asset(path), 'Cannot replace generated section material'
material = unreal.AssetToolsHelpers.get_asset_tools().create_asset(name, root, unreal.Material, unreal.MaterialFactoryNew())
assert material
material.set_editor_property('shading_model', unreal.MaterialShadingModel.MSM_DEFAULT_LIT)
material.set_editor_property('two_sided', False)

def expr(cls, **kw):
    e = M.create_material_expression(material, cls)
    for k,v in kw.items():
        e.set_editor_property(k,v)
    return e

def constant(v):
    return expr(unreal.MaterialExpressionConstant, r=v)

color = expr(unreal.MaterialExpressionVectorParameter, parameter_name='Color', default_value=unreal.LinearColor(1,1,1,1))
normal = expr(unreal.MaterialExpressionPixelNormalWS)
vertex_normal = expr(unreal.MaterialExpressionVertexNormalWS)
zero, one, spec = constant(0), constant(1), constant(0.5)
master = E.load_asset('/Game/Models/Materials/M_TCModel')
indirect = M.get_material_property_input_node(master, P.MP_EMISSIVE_COLOR)
assert indirect.get_editor_property('description') == 'TCLookIndirect', 'Generate look assets first'
node = expr(unreal.MaterialExpressionCustom, code=indirect.get_editor_property('code'), output_type=unreal.CustomMaterialOutputType.CMOT_FLOAT3, description='TCLookIndirect')
mpc = E.load_asset('/Game/Models/Materials/Look/MPC_TCLook')
inputs = [('Color',color),('Roughness',one),('Metallic',zero),('Specular',spec),('NormalWS',normal),('Emissive',zero),('VertexNormal',vertex_normal),
          ('CameraVector',expr(unreal.MaterialExpressionCameraVectorWS)),
          ('Env',expr(unreal.MaterialExpressionTextureObjectParameter, parameter_name='LookEnv', texture=E.load_asset('/Game/Models/Materials/Look/T_TCLookEnv')))]
inputs += [(k,expr(unreal.MaterialExpressionCollectionParameter,collection=mpc,parameter_name=p)) for k,p in [('HemiTop','HemiTop'),('HemiBottom','HemiBottom'),('Look','Env')]]
custom_inputs = []
for k,v in inputs:
    ci = unreal.CustomInput()
    ci.set_editor_property('input_name', k)
    custom_inputs.append(ci)
node.set_editor_property('inputs', custom_inputs)
for k,v in inputs:
    assert M.connect_material_expressions(v,'',node,k)
for e,p in [(color,P.MP_BASE_COLOR),(one,P.MP_ROUGHNESS),(spec,P.MP_SPECULAR),(zero,P.MP_METALLIC),(zero,P.MP_AMBIENT_OCCLUSION),(node,P.MP_EMISSIVE_COLOR)]:
    assert M.connect_material_property(e,'',p)
if hasattr(M, 'set_base_material_usage'):
    M.set_base_material_usage(material,unreal.MaterialUsage.MATUSAGE_INSTANCED_STATIC_MESHES)
else:
    M.set_material_usage(material,unreal.MaterialUsage.MATUSAGE_INSTANCED_STATIC_MESHES)
errors = M.recompile_material(material)
assert not errors, str(errors)
E.save_loaded_asset(material)
unreal.log('bridgefx: generated '+path)
