"""Rebuild reference fog compositing after the existing world, relief, water and look generators.

Run in UnrealEditor-Cmd with -run=pythonscript -EnablePlugins=PythonScriptPlugin -nullrhi
-script="<repo>/unreal/Tools/world/make-fog-materials.py". Generated assets are build outputs.
Reads TCFogFrontier.Shader.inl, pinned surfaces.js/fog.js and the existing source-derived
TCLookIndirect shader (three r186 PMREM/DFG), retaining the game's geometry and textures.
"""
from pathlib import Path
import hashlib, json, re
import unreal

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
M = unreal.MaterialEditingLibrary
E = unreal.EditorAssetLibrary
P = unreal.MaterialProperty
T = unreal.CustomMaterialOutputType
MARK = 'TCReferenceFogV1'
SHADER = REPO/'unreal/Source/ThreeCrossroads/World/TCFogFrontier.Shader.inl'
MIST = SHADER.read_text().split('R"TCFOG(', 1)[1].split(')TCFOG"', 1)[0]
REPORT = {'format': 'tc-reference-fog-v1', 'materials': [], 'sources': {}}
for f in [SHADER, REPO/'reference/ww2-rts/client/fog.js', REPO/'reference/ww2-rts/client/surfaces.js', REPO/'reference/ww2-rts/client/water.js']:
    REPORT['sources'][str(f.relative_to(REPO))] = hashlib.sha256(f.read_bytes()).hexdigest()

def asset(path):
    a = E.load_asset(path)
    if not a: raise RuntimeError('Missing source-generated asset: '+path)
    return a

def expr(mat, cls, **props):
    n = M.create_material_expression(mat, cls, -500, 600)
    n.set_editor_property('desc', MARK)
    for k,v in props.items(): n.set_editor_property(k,v)
    return n

def connect(a, out, b, pin):
    if not M.connect_material_expressions(a,out,b,pin): raise RuntimeError('connect '+pin)

def prop(a, out, p):
    if not M.connect_material_property(a,out,p): raise RuntimeError('property '+str(p))

def custom(mat, code, inputs, ret=T.CMOT_FLOAT1):
    n = expr(mat,unreal.MaterialExpressionCustom,code=code,description=MARK,output_type=ret)
    pins=[]
    for name,src,out in inputs:
        p=unreal.CustomInput();p.set_editor_property('input_name',name);pins.append(p)
    n.set_editor_property('inputs',pins)
    for name,src,out in inputs: connect(src,out,n,name)
    return n

def node(mat, desc):
    ns=[n for n in M.get_material_expressions(mat) if isinstance(n,unreal.MaterialExpressionCustom) and str(n.get_editor_property('description'))==desc]
    if len(ns)!=1:
        live=set()
        def visit(n):
            if not n or n in live: return
            live.add(n)
            for i in M.get_inputs_for_material_expression(mat,n): visit(i)
        for p in [P.MP_BASE_COLOR,P.MP_EMISSIVE_COLOR,P.MP_NORMAL,P.MP_ROUGHNESS,P.MP_SPECULAR]: visit(M.get_material_property_input_node(mat,p))
        ns=[n for n in ns if n in live]
    if len(ns)!=1: raise RuntimeError(str((mat.get_name(),desc,len(ns))))
    return ns[0]

def clear_ours(mat):
    for n in M.get_material_expressions(mat):
        if str(n.get_editor_property('desc'))==MARK: M.delete_material_expression(mat,n)

def save(mat):
    M.recompile_material(mat);E.save_loaded_asset(mat,only_if_is_dirty=False)
    REPORT['materials'].append(mat.get_path_name())

def const(mat,x): return expr(mat,unreal.MaterialExpressionConstant,r=x)
def rgb(mat,n):
    a=expr(mat,unreal.MaterialExpressionComponentMask,r=True,g=True,b=True,a=False);connect(n,'',a,'');return a

def component(mat,n,name):
    a=expr(mat,unreal.MaterialExpressionComponentMask,r=name=='r',g=name=='g',b=name=='b',a=name=='a');connect(n,'',a,'');return a

# Shared real-time mist clock, also used by surfaces on instanced meshes.
tools=unreal.AssetToolsHelpers.get_asset_tools()
mpc=(E.load_asset('/Game/Fog/MPC_TCFog') if E.does_asset_exist('/Game/Fog/MPC_TCFog') else None) or tools.create_asset('MPC_TCFog','/Game/Fog',unreal.MaterialParameterCollection,unreal.MaterialParameterCollectionFactoryNew())
s=unreal.CollectionScalarParameter();s.set_editor_property('parameter_name','FogTime');s.set_editor_property('default_value',0.0)
mpc.set_editor_property('scalar_parameters',[s]);E.save_loaded_asset(mpc,only_if_is_dirty=False)

# Reference overlay, before tone mapping, after the translucent water. Raw alpha, clamped bilinear G8.
fog=(E.load_asset('/Game/Fog/M_TCFogOverlay') if E.does_asset_exist('/Game/Fog/M_TCFogOverlay') else None) or tools.create_asset('M_TCFogOverlay','/Game/Fog',unreal.Material,unreal.MaterialFactoryNew())
M.delete_all_material_expressions(fog)
fog.set_editor_property('blend_mode',unreal.BlendMode.BLEND_TRANSLUCENT)
fog.set_editor_property('shading_model',unreal.MaterialShadingModel.MSM_UNLIT)
fog.set_editor_property('two_sided',False)
fog.set_editor_property('disable_depth_test',False)
fog.set_editor_property('translucency_pass',unreal.MaterialTranslucencyPass.MTP_BEFORE_DOF)
wp=expr(fog,unreal.MaterialExpressionWorldPosition)
size=expr(fog,unreal.MaterialExpressionVectorParameter,parameter_name='FogSize',default_value=unreal.LinearColor(1,1,0,0))
tex=expr(fog,unreal.MaterialExpressionTextureObjectParameter,parameter_name='FogAlpha',texture=asset('/Engine/EngineResources/WhiteSquareTexture'))
time=expr(fog,unreal.MaterialExpressionScalarParameter,parameter_name='FogTime',default_value=0.0)
outer=expr(fog,unreal.MaterialExpressionScalarParameter,parameter_name='Outer',default_value=0.0)
code=MIST+'''
float2 p = WPos.xy * .01;
float a = FogAlpha.SampleLevel(FogAlphaSampler, p / max(FogSize.xy, float2(1,1)), 0).r;
if (Outer > .5) {
    float d = length(p - clamp(p, float2(0,0), FogSize.xy));
    a = lerp(a, 185.0 / 255.0, smoothstep(6.0, 32.0, d));
}
return fow.Alpha(a, p, FogTime);
'''
a=custom(fog,code,[('WPos',wp,''),('FogSize',size,''),('FogAlpha',tex,''),('FogTime',time,''),('Outer',outer,'')])
prop(a,'',P.MP_OPACITY)
shade=expr(fog,unreal.MaterialExpressionConstant3Vector,constant=unreal.LinearColor(10/255,10/255,10/255,1))
prop(shade,'',P.MP_EMISSIVE_COLOR)
# A view-directed submillimetre depth nudge is the native equivalent of the source polygon
# offset. A normal offset leaves gaps on the vertical joins at a known/unknown boundary.
cam=expr(fog,unreal.MaterialExpressionCameraPositionWS)
bias=custom(fog,'return normalize(Camera - Position) * .05;', [('Camera',cam,''),('Position',wp,'')],T.CMOT_FLOAT3)
prop(bias,'',P.MP_WORLD_POSITION_OFFSET);save(fog)

# Ground and apron are lit normally; the overlay now performs the blend after lighting.
relief=asset('/Game/Relief/M_TCRelief')
# Resolve live source nodes before removing the previous generated emissive connection.
paint=node(relief,'TCReliefPaint');fill=node(relief,'TCReliefFill');clear_ours(relief)
prop(paint,'',P.MP_BASE_COLOR)
em=expr(relief,unreal.MaterialExpressionMultiply);connect(paint,'',em,'A');connect(fill,'',em,'B');prop(em,'',P.MP_EMISSIVE_COLOR);save(relief)
apron=asset('/Game/Water/M_TCApron')
n=node(apron,'TCApron');c=str(n.get_editor_property('code'))
c=c.replace('return col * fogK;','return col;')
n.set_editor_property('code',c);save(apron)

# Rubble and craters draw before the common overlay. Removing the legacy multiplier avoids double fog.
rubble=asset('/Game/Terrain/Materials/M_TCRubbleDecal')
rubble.set_editor_property('translucency_pass',unreal.MaterialTranslucencyPass.MTP_BEFORE_DOF)
r=node(rubble,'TCRubbleColor')
r.set_editor_property('code',str(r.get_editor_property('code')).replace(' * Dim;', ';'))
save(rubble)

# Structures, props, foliage and railways: fogShader mixes lit radiance toward .0392.
# Lambert's diffuse light scales with base colour, so attenuating base and adding the shade
# as emissive performs the same linear mix while retaining native depth tests and shadows.
for name in ['M_TCWorldSurface','M_TCWorldLambert','M_TCWorldLeaf']:
    mat=asset('/Game/World/Materials/'+name)
    color=node(mat,name);c=str(color.get_editor_property('code'));clear_ours(mat)
    old=c.index('float fog = 1.0;') if 'float fog = 1.0;' in c else -1
    if old>=0: c=c[:old]+'return c;\n'
    color.set_editor_property('code',c)
    wp=expr(mat,unreal.MaterialExpressionWorldPosition)
    tex=expr(mat,unreal.MaterialExpressionTextureObjectParameter,parameter_name='FogMap',texture=asset('/Engine/EngineResources/WhiteSquareTexture'))
    sz=expr(mat,unreal.MaterialExpressionVectorParameter,parameter_name='FogSize',default_value=unreal.LinearColor(1,1,0,0))
    on=expr(mat,unreal.MaterialExpressionScalarParameter,parameter_name='FogOn',default_value=0.0)
    tm=expr(mat,unreal.MaterialExpressionCollectionParameter,collection=mpc,parameter_name='FogTime')
    a=custom(mat,MIST+'''float2 p = WPos.xy * .01;
return FogOn > .5 ? fow.Alpha(FogMap.SampleLevel(FogMapSampler, p / max(FogSize.xy, float2(1,1)), 0).r, p, FogTime) : 0;
''',[('WPos',wp,''),('FogMap',tex,''),('FogSize',sz,''),('FogOn',on,''),('FogTime',tm,'')])
    base=custom(mat,'return Color * (1.0 - Alpha);',[('Color',color,''),('Alpha',a,'')],T.CMOT_FLOAT3)
    em=custom(mat,'return float3(.0392,.0392,.0392) * Alpha;',[('Alpha',a,'')],T.CMOT_FLOAT3)
    prop(base,'',P.MP_BASE_COLOR);prop(em,'',P.MP_EMISSIVE_COLOR);save(mat)

# Water uses the existing source-derived Standard hemisphere/PMREM shader, as the imported models do.
# Previously it retained Unreal's generic skylight while the other Standard materials used three's IBL.
water=asset('/Game/Water/M_TCWater')
water.set_editor_property('translucency_pass',unreal.MaterialTranslucencyPass.MTP_BEFORE_DOF)
outs=[node(water,'TCWater'+str(i)) for i in range(4)];clear_ours(water)
for n in outs:
    code=str(n.get_editor_property('code'))
    code=re.sub(r'float fogK = pow\(Fog.SampleLevel\(FogSampler, xz / Size.xy, 0\).r, FogGamma\);','float fogK = 1.0; // fog is composited after water by the ground/apron overlay',code)
    n.set_editor_property('code',code)
model=asset('/Game/Models/Materials/M_TCModel')
source=M.get_material_property_input_node(model,P.MP_EMISSIVE_COLOR)
if not source or str(source.get_editor_property('description'))!='TCLookIndirect': raise RuntimeError('Generate source look assets before water fog materials')
look=asset('/Game/Models/Materials/Look/MPC_TCLook')
params=[expr(water,unreal.MaterialExpressionCollectionParameter,collection=look,parameter_name=n) for n in ['HemiTop','HemiBottom','Env']]
base=rgb(water,outs[0]);normal=rgb(water,outs[1]);sheen=rgb(water,outs[2])
rough=component(water,outs[3],'r');spec=component(water,outs[3],'g')
# Source Standard lighting is evaluated below. An unlit carrier permits the
# negative intermediate required to invert bed-depth haze without clipping shadows.
water.set_editor_property('shading_model',unreal.MaterialShadingModel.MSM_UNLIT)
water.set_editor_property('allow_negative_emissive_color',True)
code=str(source.get_editor_property('code'))
water_shader=REPO/'unreal/Source/ThreeCrossroads/World/TCWater.Shader.inl'
direct=water_shader.read_text().split('R"TCWATER(',1)[1].split(')TCWATER"',1)[0]
code=code.replace('return Emissive + indirect;',direct+'\nreturn waterRadiance;')
haze=expr(water,unreal.MaterialExpressionVectorParameter,parameter_name='WaterHaze',default_value=unreal.LinearColor(0,0,0,0))
fog_params=expr(water,unreal.MaterialExpressionVectorParameter,parameter_name='WaterFog',default_value=unreal.LinearColor(0,0,0,0))
opaque_depth=expr(water,unreal.MaterialExpressionSceneDepth)
sun_dir=expr(water,unreal.MaterialExpressionVectorParameter,parameter_name='SunDirection',default_value=unreal.LinearColor(0,0,1,0))
sun_colour=expr(water,unreal.MaterialExpressionVectorParameter,parameter_name='SunColour',default_value=unreal.LinearColor(0,0,0,0))
shadow_radius=expr(water,unreal.MaterialExpressionScalarParameter,parameter_name='ShadowRadius',default_value=2.0)
low=expr(water,unreal.MaterialExpressionScalarParameter,parameter_name='Low',default_value=0.0)
# A Custom input's default vector pin is RGB. Append direction alpha explicitly.
sun_rgba=expr(water,unreal.MaterialExpressionAppendVector)
connect(sun_dir,'RGB',sun_rgba,'A');connect(sun_dir,'A',sun_rgba,'B')
REPORT['sources'][str(water_shader.relative_to(REPO))]=hashlib.sha256(water_shader.read_bytes()).hexdigest()
light=custom(water,code,[('Color',base,''),('Roughness',rough,''),('Metallic',const(water,0),''),('Specular',spec,''),('NormalWS',normal,''),('Emissive',sheen,''),('VertexNormal',expr(water,unreal.MaterialExpressionVertexNormalWS),''),('CameraVector',expr(water,unreal.MaterialExpressionCameraVectorWS),''),('Env',expr(water,unreal.MaterialExpressionTextureObjectParameter,parameter_name='LookEnv',texture=asset('/Game/Models/Materials/Look/T_TCLookEnv')),''),('HemiTop',params[0],''),('HemiBottom',params[1],''),('Look',params[2],''),('WPos',expr(water,unreal.MaterialExpressionWorldPosition),''),('SunDirection',sun_rgba,''),('SunColour',sun_colour,''),('ShadowRadius',shadow_radius,''),('Low',low,''),('WaterHaze',haze,''),('WaterFog',fog_params,''),('OpaqueDepth',opaque_depth,'')],T.CMOT_FLOAT3)
prop(light,'',P.MP_EMISSIVE_COLOR);prop(const(water,0),'',P.MP_AMBIENT_OCCLUSION)
prop(const(water,0),'',P.MP_BASE_COLOR);prop(const(water,0),'',P.MP_SPECULAR)
save(water)
REPORT['water_indirect_hlsl_sha256']=hashlib.sha256(code.encode()).hexdigest()
content=Path(unreal.Paths.convert_relative_path_to_full(unreal.Paths.project_content_dir()))
(content/'Fog').mkdir(exist_ok=True)
(content/'Fog/FogMaterials.json').write_text(json.dumps(REPORT,indent=2)+'\n')
unreal.log('Reference fog material generation complete: '+str(len(REPORT['materials'])))
