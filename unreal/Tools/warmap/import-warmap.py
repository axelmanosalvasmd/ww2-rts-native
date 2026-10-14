"""The paper war map's assets in /Game/WarMap, from what unreal/Tools/warmap/make-warmap.mjs assets wrote into its work
folder (the reference's own symbols, rasterised by Chromium, and the face Chromium draws map-view.js's text with).
Generated, never hand-edited. Run inside the editor on the project:

  UnrealEditor-Cmd <project> -run=pythonscript -EnablePlugins=PythonScriptPlugin -unattended -nop4 -nosplash -nullrhi \
    -script="unreal/Tools/warmap/import-warmap.py --work <scratch dir>"

Output under /Game/WarMap (unreal/Content/WarMap):
  T_TCWarMapSymbols        symbols.png: white silhouettes (top half) and their halos (bottom half), alpha the coverage;
                           uncompressed RGBA, mipmapped (drawn at 26 px from 128 px cells), clamped, trilinear
  Fonts/F_TCWarMapMono     (made before this script by -run=TCWarMapPaint -ImportFont, see run.sh) the font face (map-view.js "bold ... Courier New": fontconfig's match on the machine that
                           ran the tool), loaded by Client/TCWarMap.cpp into a runtime font; its licence beside it
  WarMapData.json          the atlas layout and the shared/sim.js numbers (read with FFileHelper, staged as a file)
  WarMapProvenance.json    sources, outputs and the assets' sha256
"""
import argparse
import hashlib
import json
import os
import shutil
import sys

import unreal

ROOT = '/Game/WarMap'


def sha_file(p):
    h = hashlib.sha256()
    with open(p, 'rb') as f:
        for b in iter(lambda: f.read(1 << 20), b''):
            h.update(b)
    return h.hexdigest()


def log(msg):
    unreal.log('[warmap] ' + msg)


def import_file(src, folder, name, factory=None):
    task = unreal.AssetImportTask()
    task.set_editor_property('filename', src)
    task.set_editor_property('destination_path', folder)
    task.set_editor_property('destination_name', name)
    task.set_editor_property('automated', True)
    task.set_editor_property('replace_existing', True)
    task.set_editor_property('save', False)
    if factory is not None:
        task.set_editor_property('factory', factory)
    unreal.AssetToolsHelpers.get_asset_tools().import_asset_tasks([task])
    asset = unreal.EditorAssetLibrary.load_asset('%s/%s' % (folder, name))
    if not asset:
        raise RuntimeError('import failed: ' + src)
    return asset


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--work', required=True)
    args = ap.parse_args(sys.argv[1:] if len(sys.argv) > 1 else [])
    work = args.work
    content = os.path.join(unreal.Paths.convert_relative_path_to_full(unreal.Paths.project_content_dir()), 'WarMap')
    os.makedirs(os.path.join(content, 'Fonts'), exist_ok=True)
    data = json.load(open(os.path.join(work, 'WarMapData.json')))

    tex = import_file(os.path.join(work, 'symbols.png'), ROOT, 'T_TCWarMapSymbols')
    for k, v in dict(srgb=True, compression_settings=unreal.TextureCompressionSettings.TC_EDITOR_ICON,
                     mip_gen_settings=unreal.TextureMipGenSettings.TMGS_SIMPLE_AVERAGE, filter=unreal.TextureFilter.TF_TRILINEAR,
                     address_x=unreal.TextureAddress.TA_CLAMP, address_y=unreal.TextureAddress.TA_CLAMP,
                     lod_group=unreal.TextureGroup.TEXTUREGROUP_UI, never_stream=True).items():
        tex.set_editor_property(k, v)
    unreal.EditorAssetLibrary.save_loaded_asset(tex, only_if_is_dirty=False)

    # the font face is made first by the commandlet (run.sh: -run=TCWarMapPaint -ImportFont=...): the editor's font
    # import factory needs Slate, which a commandlet does not have
    font_file = os.path.join(work, 'font', data['font']['file'])
    face = unreal.EditorAssetLibrary.load_asset(ROOT + '/Fonts/F_TCWarMapMono')
    if not isinstance(face, unreal.FontFace):
        raise RuntimeError('no font face at %s/Fonts/F_TCWarMapMono: run the commandlet step first' % ROOT)
    for f in os.listdir(os.path.join(work, 'font')):
        if f.startswith('LICENSE'):
            shutil.copyfile(os.path.join(work, 'font', f), os.path.join(content, 'Fonts', f))

    shutil.copyfile(os.path.join(work, 'WarMapData.json'), os.path.join(content, 'WarMapData.json'))
    assets = {}
    for base, _, files in os.walk(content):
        for f in sorted(files):
            if f.endswith('.uasset'):
                p = os.path.join(base, f)
                assets[os.path.relpath(p, content)] = sha_file(p)
    prov = dict(format='tc-warmap-assets-v1', reference=data.get('reference'), tool_version=data.get('tool_version'),
                inputs={'symbols.png': sha_file(os.path.join(work, 'symbols.png')), data['font']['file']: sha_file(font_file),
                        'WarMapData.json': sha_file(os.path.join(work, 'WarMapData.json'))},
                font=data['font'], sources=data.get('sources'), assets=assets)
    json.dump(prov, open(os.path.join(content, 'WarMapProvenance.json'), 'w'), indent=1, sort_keys=True)
    log('done: %d assets' % len(assets))


main()
