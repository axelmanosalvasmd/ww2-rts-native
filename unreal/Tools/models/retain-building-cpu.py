"""Retain original building buffers for cooked runtime breaches. Run with Unreal Python."""
import json
import os
import re
import unreal

PIN = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d'
KINDS = {'hq', 'depot', 'barracks', 'motorpool', 'airfield', 'shipyard', 'flakpos',
         'worldbase', 'armory', 'pillbox', 'tower', 'wall', 'gate', 'supplycache'}
content = unreal.Paths.convert_relative_path_to_full(unreal.Paths.project_content_dir())
reference = os.environ.get('TC_CPU_REFERENCE', os.path.abspath(os.path.join(unreal.Paths.project_dir(), '..', 'reference', 'ww2-rts')))
with open(os.path.join(reference, 'shared', 'sim.js'), encoding='utf-8') as f:
    declared = set(re.findall(r'UNITS\.(\w+)\s*=\s*building\(', f.read()))
if declared != KINDS:
    raise RuntimeError('Pinned building types differ: %r' % sorted(declared ^ KINDS))
with open(os.path.join(content, 'Models', 'ModelCatalog.json'), encoding='utf-8') as f:
    catalog = json.load(f)
if catalog.get('reference_commit') != PIN:
    raise RuntimeError('Model catalog pin does not match '+PIN)
present = {m['type'] for m in catalog['models'] if m['type'] in KINDS}
if present != KINDS:
    raise RuntimeError('Missing source building models: %r' % sorted(KINDS - present))
paths = sorted({p['mesh'] for m in catalog['models'] if m['type'] in KINDS for p in m['parts']})
report = []
for path in paths:
    mesh = unreal.load_asset(path)
    if not isinstance(mesh, unreal.StaticMesh):
        raise RuntimeError('Missing source static mesh '+path)
    before = bool(mesh.get_editor_property('allow_cpu_access'))
    if not before:
        mesh.set_editor_property('allow_cpu_access', True)
        if not unreal.EditorAssetLibrary.save_loaded_asset(mesh, only_if_is_dirty=False):
            raise RuntimeError('Cannot save CPU buffers for '+path)
    after = bool(mesh.get_editor_property('allow_cpu_access'))
    if not after:
        raise RuntimeError('CPU buffers not retained for '+path)
    report.append({'mesh':path, 'before':before, 'cpuAccess':after})
report_path = os.environ.get('TC_CPU_REPORT', os.path.join(unreal.Paths.project_saved_dir(), 'building-cpu-assets.json'))
with open(report_path, 'w', encoding='utf-8') as f:
    json.dump({'reference':PIN, 'assets':report}, f, indent=2)
unreal.log('Retained source building CPU buffers: '+str(len(report)))
