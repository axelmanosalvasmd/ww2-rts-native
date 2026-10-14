"""Renders the native painted ground through the generated materials, in the editor with offscreen rendering, so the
material interface is seen working on real native output (capture-prep.mjs makes the scene): the harness's Native
canvas on M_TCGroundPaint over the map (flat: the relief is not this tool's), its fog mask bilinear (and, for the
comparison picture, the same mask nearest), the rubble decals on M_TCRubbleDecal and World craters on M_TCCraterDecal
from the harness's decal vertices. A sun and a sky light of fixed strength; this checks the paint and the materials,
not the light (the look tools calibrate that).

  UnrealEditor <host project with ProceduralMeshComponent and PythonScriptPlugin> -RenderOffscreen -unattended -nop4 \
    -nosplash -NoSound -ExecutePythonScript="unreal/Tools/terrain/capture-ground.py --scene <scene.json> --out <dir> --prefix <name>"

The editor (not the commandlet) runs it: shaders compile in the background and only an engine tick takes their results,
so the script waits on a tick callback until a probe capture stops changing (three probes after it changed from the
first, which shows the fallback material while shaders compile, or eight when it never changed because the shaders came
from the cache), then takes every shot and quits the editor.
"""
import argparse
import json
import math
import os
import sys

import unreal

eal = unreal.EditorAssetLibrary
mel = unreal.MaterialEditingLibrary
TMP = '/Game/TerrainCapture'


def log(msg):
    unreal.log_warning('[terrain-capture] ' + msg)


def import_png(path, name, srgb, nearest=False, gray=False):
    task = unreal.AssetImportTask()
    task.set_editor_property('filename', path)
    task.set_editor_property('destination_path', TMP)
    task.set_editor_property('destination_name', name)
    task.set_editor_property('automated', True)
    task.set_editor_property('replace_existing', True)
    task.set_editor_property('save', False)
    unreal.AssetToolsHelpers.get_asset_tools().import_asset_tasks([task])
    tex = eal.load_asset('%s/%s' % (TMP, name))
    tex.set_editor_property('srgb', srgb)
    tex.set_editor_property('address_x', unreal.TextureAddress.TA_CLAMP)
    tex.set_editor_property('address_y', unreal.TextureAddress.TA_CLAMP)
    if gray:
        tex.set_editor_property('compression_settings', unreal.TextureCompressionSettings.TC_GRAYSCALE)
        tex.set_editor_property('mip_gen_settings', unreal.TextureMipGenSettings.TMGS_NO_MIPMAPS)
        tex.set_editor_property('filter', unreal.TextureFilter.TF_NEAREST if nearest else unreal.TextureFilter.TF_BILINEAR)
    else:
        tex.set_editor_property('lod_group', unreal.TextureGroup.TEXTUREGROUP_WORLD)
        tex.set_editor_property('compression_settings', unreal.TextureCompressionSettings.TC_VECTOR_DISPLACEMENTMAP)  # the canvas's bytes, uncompressed
        tex.set_editor_property('never_stream', True)
    return tex


def mic(name, parent, scalars, vectors, textures):
    tools = unreal.AssetToolsHelpers.get_asset_tools()
    path = '%s/%s' % (TMP, name)
    if eal.does_asset_exist(path):
        eal.delete_asset(path)
    m = tools.create_asset(name, TMP, unreal.MaterialInstanceConstant, unreal.MaterialInstanceConstantFactoryNew())
    mel.set_material_instance_parent(m, parent)
    for k, v in scalars.items():
        mel.set_material_instance_scalar_parameter_value(m, k, v)
    for k, v in vectors.items():
        mel.set_material_instance_vector_parameter_value(m, k, unreal.LinearColor(v[0], v[1], v[2], 1.0))
    for k, v in textures.items():
        mel.set_material_instance_texture_parameter_value(m, k, v)
    mel.update_material_instance(m)
    return m


def proc_mesh_actor(name):
    actor = unreal.EditorLevelLibrary.spawn_actor_from_class(unreal.Actor, unreal.Vector(0, 0, 0))
    actor.set_actor_label(name)
    sds = unreal.get_engine_subsystem(unreal.SubobjectDataSubsystem)
    root = sds.k2_gather_subobject_data_for_instance(actor)[0]
    params = unreal.AddNewSubobjectParams(parent_handle=root, new_class=unreal.ProceduralMeshComponent, blueprint_context=None)
    handle, fail = sds.add_new_subobject(params)
    comp = unreal.SubobjectDataBlueprintFunctionLibrary.get_object(unreal.SubobjectDataBlueprintFunctionLibrary.get_data(handle))
    return actor, comp


def decal_section(comp, index, positions, uvs, count, material, lift):
    verts, tris, normals, uv0, colors, tangents = [], [], [], [], [], []
    for s in range(count):
        base = len(verts)
        for v in range(25):
            p = positions[(s * 25 + v) * 3:(s * 25 + v) * 3 + 3]
            # flat ground here: the decal sits its own lift over z = 0 (the reference's lift: 2.5 cm rubble, 5 cm craters)
            verts.append(unreal.Vector(p[0] * 100.0, p[2] * 100.0, lift * 100.0))
            normals.append(unreal.Vector(0, 0, 1))
            uv0.append(unreal.Vector2D(uvs[(s * 25 + v) * 2], uvs[(s * 25 + v) * 2 + 1]) if uvs else unreal.Vector2D((v % 5) / 4.0, (v // 5) / 4.0))
        for z in range(4):
            for x in range(4):
                a = base + z * 5 + x
                tris += [a, a + 5, a + 1, a + 1, a + 5, a + 6]
    comp.create_mesh_section_linear_color(index, verts, tris, normals, uv0, [], [], [], colors, tangents, False)
    comp.set_material(index, material)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--scene', required=True)
    ap.add_argument('--out', required=True)
    ap.add_argument('--prefix', required=True)
    ap.add_argument('--width', type=int, default=1280)
    ap.add_argument('--height', type=int, default=720)
    args = ap.parse_args(sys.argv[1:])
    scene = json.load(open(args.scene))
    os.makedirs(args.out, exist_ok=True)
    W, H = scene['w'], scene['h']
    world = unreal.EditorLoadingAndSavingUtils.new_blank_map(False)
    world = unreal.EditorLevelLibrary.get_editor_world()
    canvas = import_png(scene['canvas'], 'T_Canvas', True)
    fog = import_png(scene['fogPng'], 'T_Fog', False, gray=True)
    fog_n = import_png(scene['fogPng'], 'T_FogNearest', False, nearest=True, gray=True)
    ground = eal.load_asset('/Game/Terrain/Materials/M_TCGroundPaint')
    rubble = eal.load_asset('/Game/Terrain/Materials/M_TCRubbleDecal')
    crater = eal.load_asset('/Game/Terrain/Materials/M_TCCraterDecal')
    size = (W * 2.0, H * 2.0, 0.0)
    common = dict(scalars={'GroundFogOn': 1.0, 'GroundMist': 0.0, 'GroundFogTime': 0.0}, vectors={'GroundMapSize': size})
    m_ground = mic('MI_Ground', ground, common['scalars'], common['vectors'], {'GroundPaint': canvas, 'GroundFog': fog})
    m_ground_n = mic('MI_GroundNearest', ground, common['scalars'], common['vectors'], {'GroundPaint': canvas, 'GroundFog': fog_n})
    m_ground_mist = mic('MI_GroundMist', ground, dict(common['scalars'], GroundMist=1.0, GroundFogTime=37.0), common['vectors'], {'GroundPaint': canvas, 'GroundFog': fog})
    m_ground_clear = mic('MI_GroundNoFog', ground, dict(common['scalars'], GroundFogOn=0.0), common['vectors'], {'GroundPaint': canvas, 'GroundFog': fog})
    m_rubble = mic('MI_Rubble', rubble, common['scalars'], common['vectors'], {'GroundFog': fog})
    m_crater = mic('MI_Crater', crater, {}, {}, {})
    # the ground: one quad over the map (Unreal X = reference x, Y = reference z, cm)
    g_actor, g_comp = proc_mesh_actor('Ground')
    X, Y = W * 200.0, H * 200.0
    g_comp.create_mesh_section_linear_color(0, [unreal.Vector(0, 0, 0), unreal.Vector(X, 0, 0), unreal.Vector(X, Y, 0), unreal.Vector(0, Y, 0)], [0, 3, 1, 1, 3, 2],
                                            [unreal.Vector(0, 0, 1)] * 4, [unreal.Vector2D(0, 0), unreal.Vector2D(1, 0), unreal.Vector2D(1, 1), unreal.Vector2D(0, 1)], [], [], [], [], [], False)
    g_comp.set_material(0, m_ground)
    d_actor, d_comp = proc_mesh_actor('Decals')
    dec = scene['decals']
    if dec['rubble']['count']:
        decal_section(d_comp, 0, dec['rubble']['positions'], dec['rubble']['uvs'], dec['rubble']['count'], m_rubble, 0.025)
    if dec.get('craters') and dec['craters']['count']:
        decal_section(d_comp, 1, dec['craters']['positions'], None, dec['craters']['count'], m_crater, 0.05)
    d_comp.set_editor_property('cast_shadow', False)
    # light: a sun from the south-east, high, and a sky light
    sun = unreal.EditorLevelLibrary.spawn_actor_from_class(unreal.DirectionalLight, unreal.Vector(0, 0, 10000), unreal.Rotator(0, -50, 135))
    sun.light_component.set_editor_property('intensity', 7.0)
    sky = unreal.EditorLevelLibrary.spawn_actor_from_class(unreal.SkyLight, unreal.Vector(0, 0, 5000))
    cube = unreal.load_asset('/Engine/MapTemplates/Sky/DaylightAmbientCubemap')
    if cube:
        sky.light_component.set_editor_property('source_type', unreal.SkyLightSourceType.SLS_SPECIFIED_CUBEMAP)
        sky.light_component.set_editor_property('cubemap', cube)
    sky.light_component.set_editor_property('intensity', 1.0)
    sky.light_component.recapture_sky()
    rt = unreal.RenderingLibrary.create_render_target2d(world, args.width, args.height, unreal.TextureRenderTargetFormat.RTF_RGBA8_SRGB)
    cap = unreal.EditorLevelLibrary.spawn_actor_from_class(unreal.SceneCapture2D, unreal.Vector(0, 0, 0))
    cc = cap.capture_component2d
    cc.set_editor_property('texture_target', rt)
    cc.set_editor_property('capture_source', unreal.SceneCaptureSource.SCS_FINAL_COLOR_LDR)
    cc.set_editor_property('capture_every_frame', False)
    pp = cc.get_editor_property('post_process_settings')
    pp.set_editor_property('override_auto_exposure_method', True)
    pp.set_editor_property('auto_exposure_method', unreal.AutoExposureMethod.AEM_MANUAL)
    pp.set_editor_property('override_auto_exposure_apply_physical_camera_exposure', True)
    pp.set_editor_property('auto_exposure_apply_physical_camera_exposure', False)
    pp.set_editor_property('override_auto_exposure_bias', True)
    pp.set_editor_property('auto_exposure_bias', 0.0)
    cc.set_editor_property('post_process_settings', pp)
    cc.set_editor_property('post_process_blend_weight', 1.0)
    aspect = float(args.width) / args.height
    shots = []

    close = scene['cameras'][0]
    plan = [('cam%d-%dm' % (i, cam['dist']), cam, m_ground) for i, cam in enumerate(scene['cameras'])]
    plan += [('fog-bilinear', dict(close, dist=70, pitch=1.35), m_ground), ('fog-nearest', dict(close, dist=70, pitch=1.35), m_ground_n),
             ('fog-mist', dict(close, dist=70, pitch=1.35), m_ground_mist), ('nofog', close, m_ground_clear)]

    def probe():
        # one material of each kind in view, then a few pixels read back
        g_comp.set_material(0, m_ground)
        cam = scene['cameras'][1]
        place(cam)
        cc.capture_scene()
        px = []
        for fx, fy in ((0.5, 0.5), (0.25, 0.3), (0.75, 0.7), (0.3, 0.8), (0.7, 0.2)):
            c = unreal.RenderingLibrary.read_render_target_pixel(world, rt, int(args.width * fx), int(args.height * fy))
            px.append((c.r, c.g, c.b))
        return px

    def place(cam):
        cc.set_editor_property('fov_angle', math.degrees(2 * math.atan(math.tan(math.radians(cam['fov'] / 2)) * aspect)))
        tx, tz, d, yaw, pitch = cam['x'], cam['z'], cam['dist'], cam['yaw'], cam['pitch']
        ex, ez, ey = tx - math.cos(yaw) * math.cos(pitch) * d, tz - math.sin(yaw) * math.cos(pitch) * d, math.sin(pitch) * d
        loc = unreal.Vector(ex * 100, ez * 100, ey * 100)
        rot = unreal.MathLibrary.find_look_at_rotation(loc, unreal.Vector(tx * 100, tz * 100, 0))
        cap.set_actor_location_and_rotation(loc, rot, False, False)

    state = dict(t=0.0, next=0.0, first=None, last=None, stable=0, done=False, handle=None)

    def finish():
        for label, cam, material in plan:
            g_comp.set_material(0, material)
            place(cam)
            cc.capture_scene()
            unreal.RenderingLibrary.export_render_target(world, rt, args.out, '%s-%s.png' % (args.prefix, label))
            shots.append('%s-%s' % (args.prefix, label))
        json.dump(dict(shots=shots, scene=args.scene, waited_seconds=state['t']), open(os.path.join(args.out, args.prefix + '-shots.json'), 'w'))
        log('captured %d shots after %.0f s' % (len(shots), state['t']))

    def tick(dt):
        if state['done']:
            return
        state['t'] += dt
        if state['t'] < state['next']:
            return
        state['next'] = state['t'] + 4.0
        px = probe()
        if state['first'] is None:
            state['first'] = px
        changed = px != state['first']
        state['stable'] = state['stable'] + 1 if px == state['last'] else 0
        state['last'] = px
        log('probe at %.0f s: %s, stable %d' % (state['t'], 'changed' if changed else 'as at first', state['stable']))
        # changed from the first probe and steady, or steady for half a minute (the shaders were cached already)
        if (changed and state['stable'] >= 3) or state['stable'] >= 8 or state['t'] > 600:
            state['done'] = True
            finish()
            unreal.unregister_slate_post_tick_callback(state['handle'])
            unreal.SystemLibrary.quit_editor()

    state['handle'] = unreal.register_slate_post_tick_callback(tick)


main()
