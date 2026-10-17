"""Refresh the pinned infantry RGB normal formula in existing model materials.

Run with UnrealEditor <project> -run=pythonscript -script=<this file> -nullrhi.
LOOK_REFERENCE overrides the reference checkout, LOOK_REFRESH_LOG the manifest path.
This preserves each graph, motion deformation and indirect-light wiring. Full imports
already receive the same formula through import-models.py.
"""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess

import unreal

tools_dir = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('tc_model_import', tools_dir / 'models/import-models.py')
generator = importlib.util.module_from_spec(spec)
spec.loader.exec_module(generator)
reference = Path(os.environ.get('LOOK_REFERENCE', generator.REFERENCE))
pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d'
actual_pin = subprocess.check_output(['git', '-C', str(reference), 'rev-parse', 'HEAD'], text=True).strip()
if actual_pin != pin:
    raise RuntimeError('normal refresh requires reference ' + pin)
differences = generator.reference_parity(str(reference))
if differences:
    raise RuntimeError('reference table differences: ' + str(differences))

mel, eal = unreal.MaterialEditingLibrary, unreal.EditorAssetLibrary
roots = ['/Game/Models/Materials', '/Game/Models/Motion']
rows = []
for path in sorted({p for root in roots for p in eal.list_assets(root, recursive=True, include_folder=False)}):
    if not path.rsplit('/', 1)[-1].startswith('M_'):
        continue
    material = eal.load_asset(path)
    if not isinstance(material, unreal.Material):
        continue
    changed = False
    for node in mel.get_material_expressions(material):
        if not isinstance(node, unreal.MaterialExpressionCustom) or str(node.get_editor_property('description')) != 'TCModel':
            continue
        code = node.get_editor_property('code')
        start = code.index('if (atlas && InfantryDetail > 0.5) {')
        end = code.index('\nif (ModelTextures > 0.5) {', start)
        updated = code[:start] + generator.INFANTRY_NORMAL_HLSL + code[end:]
        changed = updated != code
        node.set_editor_property('code', updated)
        rows.append({'asset': path, 'changed': changed, 'model_hlsl_sha256': hashlib.sha256(updated.encode()).hexdigest()})
    if changed:
        errors = mel.recompile_material(material)
        if errors:
            raise RuntimeError(path + ' shader errors: ' + str(errors))
        if not eal.save_loaded_asset(material, only_if_is_dirty=False):
            raise RuntimeError('could not save ' + path)

texture_path = '/Game/Models/Textures/T_InfantryNormal'
texture = eal.load_asset(texture_path)
if not texture:
    raise RuntimeError('missing ' + texture_path)
texture.set_editor_property('compression_settings', unreal.TextureCompressionSettings.TC_VECTOR_DISPLACEMENTMAP)
texture.set_editor_property('srgb', False)
if not eal.save_loaded_asset(texture, only_if_is_dirty=False):
    raise RuntimeError('could not save ' + texture_path)
if len(rows) < 10:
    raise RuntimeError('missing model/crowd/far/track masters: ' + str(len(rows)))

content = Path(unreal.Paths.convert_relative_path_to_full(unreal.Paths.project_content_dir()))
for row in rows + [{'asset': texture_path}]:
    relative = row['asset'].removeprefix('/Game/').split('.')[0] + '.uasset'
    asset_file = content / relative
    row['file'] = 'Content/' + relative
    row['uasset_sha256'] = hashlib.sha256(asset_file.read_bytes()).hexdigest()
    row['bytes'] = asset_file.stat().st_size
    if row not in rows:
        texture_row = row
out = Path(os.environ.get('LOOK_REFRESH_LOG', str(content / 'Models/NormalAtlasProvenance.json')))
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(json.dumps({
    'reference_commit': pin,
    'generator': 'unreal/Tools/models/import-models.py',
    'generator_sha256': hashlib.sha256(Path(generator.__file__).read_bytes()).hexdigest(),
    'refresh': 'unreal/Tools/look/refresh-normal-atlas.py',
    'formula_sha256': hashlib.sha256(generator.INFANTRY_NORMAL_HLSL.encode()).hexdigest(),
    'source': 'client/infantry-material.js',
    'source_sha256': hashlib.sha256((reference / 'client/infantry-material.js').read_bytes()).hexdigest(),
    'materials': rows,
    'texture': texture_row,
    'normal_compression': 'TC_VECTOR_DISPLACEMENTMAP',
    'normal_srgb': False,
}, indent=2) + '\n')
unreal.log('normal atlas refresh: %d masters and RGB texture saved; %s' % (len(rows), out))
