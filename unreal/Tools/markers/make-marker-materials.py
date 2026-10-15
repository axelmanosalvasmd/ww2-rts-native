"""Generate the reference marker layers. Run as an Unreal Python commandlet.
Ground: translucent, depth tested, depth writes off. Through: same, depth test off.
Vertex colours are linear. Both blend in scene linear light before the tone curve.
"""
import unreal
root = '/Game/Markers'
tools = unreal.AssetToolsHelpers.get_asset_tools()
mel = unreal.MaterialEditingLibrary
for name, through in [('M_TCMarkerGround', False), ('M_TCMarkerThrough', True)]:
    path = root + '/' + name
    mat = unreal.load_asset(path) or tools.create_asset(name, root, unreal.Material, unreal.MaterialFactoryNew())
    mel.delete_all_material_expressions(mat)
    mat.set_editor_property('shading_model', unreal.MaterialShadingModel.MSM_UNLIT)
    mat.set_editor_property('blend_mode', unreal.BlendMode.BLEND_TRANSLUCENT)
    mat.set_editor_property('two_sided', True)
    mat.set_editor_property('disable_depth_test', through)
    mat.set_editor_property('translucency_pass', unreal.MaterialTranslucencyPass.MTP_BEFORE_DOF)
    vertex = mel.create_material_expression(mat, unreal.MaterialExpressionVertexColor, -300, 0)
    assert mel.connect_material_property(vertex, '', unreal.MaterialProperty.MP_EMISSIVE_COLOR)
    assert mel.connect_material_property(vertex, 'A', unreal.MaterialProperty.MP_OPACITY)
    mel.recompile_material(mat)
    unreal.EditorAssetLibrary.save_loaded_asset(mat)
    assert mat.get_editor_property('disable_depth_test') == through
    unreal.log('MARKERS material %s depth_test=%s linear_vertex_colour=true' % (name, not through))
# Pass TC_BADGE_PNG=<export-badges.mjs output>/badges.png in the environment.
import os
import json
import hashlib
source = os.environ.get('TC_BADGE_PNG')
if source:
    task = unreal.AssetImportTask()
    task.filename = source
    task.destination_path = root
    task.destination_name = 'T_TCUnitBadges'
    task.automated = True
    task.replace_existing = True
    task.save = True
    tools.import_asset_tasks([task])
    tex = unreal.load_asset(root + '/T_TCUnitBadges')
    tex.set_editor_property('srgb', True)
    tex.set_editor_property('compression_settings', unreal.TextureCompressionSettings.TC_EDITOR_ICON)
    tex.set_editor_property('mip_gen_settings', unreal.TextureMipGenSettings.TMGS_NO_MIPMAPS)
    tex.set_editor_property('filter', unreal.TextureFilter.TF_BILINEAR)
    tex.set_editor_property('address_x', unreal.TextureAddress.TA_CLAMP)
    tex.set_editor_property('address_y', unreal.TextureAddress.TA_CLAMP)
    unreal.EditorAssetLibrary.save_loaded_asset(tex)
    unreal.log('MARKERS imported exact reference badge atlas')

# HQ camp-only painted surfaces were absent from the world scenery catalog. Copy the
# imported builder parameters onto the existing world surface master, including fog.
# The world generator's catalog_slots change keeps them in future full regenerations.
content = unreal.Paths.convert_relative_path_to_full(unreal.Paths.project_content_dir())
catalog = json.load(open(os.path.join(content, 'Models', 'ModelCatalog.json')))
slots = sorted({slot for model in catalog['models'] if model['key'].startswith('hq-')
                for part in model['parts'] for slot in part['materials']})
parent = unreal.load_asset('/Game/World/Materials/M_TCWorldSurface')
assert parent, 'generate the world surface master first'
report = dict(reference=catalog['reference_commit'], instances={})
for slot in slots:
    src = unreal.load_asset(slot.split('.')[0])
    assert src and src.get_editor_property('parent').get_name() == 'M_TCSurface', slot
    name = 'MI_TCWorld_' + src.get_name()[len('MI_TC'):]
    folder = '/Game/World/Materials/Instances'
    mi = unreal.load_asset(folder + '/' + name) or tools.create_asset(name, folder, unreal.MaterialInstanceConstant, unreal.MaterialInstanceConstantFactoryNew())
    mel.set_material_instance_parent(mi, parent)
    mel.clear_all_material_instance_parameters(mi)
    scalars = {key: mel.get_material_instance_scalar_parameter_value(src, key) for key in ('UVScale', 'Painted')}
    vectors = {key: mel.get_material_instance_vector_parameter_value(src, key) for key in ('Tint', 'TopTint')}
    texture = mel.get_material_instance_texture_parameter_value(src, 'Surface')
    assert texture, slot
    for key, value in scalars.items():
        mel.set_material_instance_scalar_parameter_value(mi, key, value)
    for key, value in vectors.items():
        mel.set_material_instance_vector_parameter_value(mi, key, value)
    mel.set_material_instance_texture_parameter_value(mi, 'Surface', texture)
    mel.update_material_instance(mi)
    unreal.EditorAssetLibrary.save_loaded_asset(mi)
    report['instances'][name] = dict(source=slot, parent=parent.get_path_name(), scalars=scalars,
                                    vectors={key: [v.r, v.g, v.b] for key, v in vectors.items()}, texture=texture.get_path_name())
    unreal.log('MARKERS camp surface %s copied from %s' % (name, slot))
if source:
    report['badge_png_sha256'] = hashlib.sha256(open(source, 'rb').read()).hexdigest()
with open(os.path.join(content, 'Markers', 'MarkerAssets.json'), 'w') as handle:
    json.dump(report, handle, indent=2, sort_keys=True)
    handle.write('\n')
