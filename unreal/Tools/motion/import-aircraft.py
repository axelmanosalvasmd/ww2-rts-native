"""Import export-aircraft.mjs output. TC_AIRCRAFT_EXPORT names the source-only export folder.
Run in an isolated content project; never regenerate shared assets during capture lanes.
"""
import hashlib,json,os,shutil
from pathlib import Path
import unreal
source=Path(os.environ['TC_AIRCRAFT_EXPORT'])
meta=json.loads((source/'aircraft.json').read_text())
assert meta['reference']=='e4e579a838aabae3b1d9dadbc17a0a87147bc52d'
root='/Game/Models/Motion/Aircraft'
mel=unreal.MaterialEditingLibrary;eal=unreal.EditorAssetLibrary;tools=unreal.AssetToolsHelpers.get_asset_tools()
material=eal.load_asset(root+'/M_AircraftShadow') if eal.does_asset_exist(root+'/M_AircraftShadow') else tools.create_asset('M_AircraftShadow',root,unreal.Material,unreal.MaterialFactoryNew())
mel.delete_all_material_expressions(material)
material.set_editor_property('blend_mode',unreal.BlendMode.BLEND_TRANSLUCENT)
material.set_editor_property('shading_model',unreal.MaterialShadingModel.MSM_UNLIT)
material.set_editor_property('two_sided',False)
material.set_editor_property('used_with_instanced_static_meshes',True)
tex=mel.create_material_expression(material,unreal.MaterialExpressionTextureSampleParameter2D,-350,0)
tex.set_editor_property('parameter_name','Shadow')
alpha=mel.create_material_expression(material,unreal.MaterialExpressionMultiply,-100,100)
alpha.set_editor_property('const_b',0.6)
mel.connect_material_expressions(tex,'A',alpha,'A')
mel.connect_material_property(tex,'RGB',unreal.MaterialProperty.MP_EMISSIVE_COLOR)
mel.connect_material_property(alpha,'',unreal.MaterialProperty.MP_OPACITY)
for row in meta['planes']:
 file=source/row['texture'];assert hashlib.sha256(file.read_bytes()).hexdigest()==row['sha256']
 task=unreal.AssetImportTask();task.filename=str(file);task.destination_path=root+'/Textures';task.destination_name='T_Shadow_'+row['type']+'_'+str(row['faction']);task.automated=True;task.replace_existing=True;task.save=True
 tools.import_asset_tasks([task]);texture=eal.load_asset(task.imported_object_paths[0]);texture.set_editor_property('srgb',True);texture.set_editor_property('address_x',unreal.TextureAddress.TA_CLAMP);texture.set_editor_property('address_y',unreal.TextureAddress.TA_CLAMP)
 texture.set_editor_property('defer_compression',False)
 texture.set_editor_property('compression_settings',unreal.TextureCompressionSettings.TC_VECTOR_DISPLACEMENTMAP)
 texture.set_editor_property('mip_gen_settings',unreal.TextureMipGenSettings.TMGS_NO_MIPMAPS);texture.set_editor_property('never_stream',True);texture.set_editor_property('filter',unreal.TextureFilter.TF_BILINEAR)
 eal.save_loaded_asset(texture)
 tex.set_editor_property('texture',texture)
 name='MI_Shadow_'+row['type']+'_'+str(row['faction'])
 mi=eal.load_asset(root+'/'+name) if eal.does_asset_exist(root+'/'+name) else tools.create_asset(name,root,unreal.MaterialInstanceConstant,unreal.MaterialInstanceConstantFactoryNew())
 mel.set_material_instance_parent(mi,material);mel.set_material_instance_texture_parameter_value(mi,'Shadow',texture);eal.save_loaded_asset(mi)
mel.recompile_material(material);eal.save_loaded_asset(material)
target=Path(unreal.Paths.project_content_dir())/'Models/Motion/Aircraft/aircraft.json';target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(source/'aircraft.json',target)
print('Imported',len(meta['planes']),'original aircraft shadows and',len(meta['decks']),'source hull point sets')
