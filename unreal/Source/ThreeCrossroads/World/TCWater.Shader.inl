// three 0.186.1: common F_Schlick, lights_physical_pars_fragment RE_Direct_Physical,
// lights_fragment_begin energy compensation and shadowmap_pars_fragment PCFShadowMap.
// The native atlas supplies visibility only. Its forward translucent BRDF and one-tap
// shadow filter are bypassed. The existing source PMREM/hemisphere shader supplies IBL.
// Inputs: WPos, CameraVector, NormalWS, VertexNormal, Color, Roughness, Low,
// SunDirection (xyz toward sun, w shadow intensity), SunColour (linear irradiance),
// ShadowRadius (source disk radius expressed in native atlas texels). 'lut' is the
// source DFG lookup supplied by TCLookIndirect. Included by make-fog-materials.py.
inline constexpr const char* TCWaterDirectShader = R"TCWATER(
struct TCWaterLight {
    float Compare(Texture2D Atlas, int2 P, int2 First, int2 Last, float Depth) {
        return Depth < Atlas.Load(int3(clamp(P, First, Last),0)).r ? 1.0 : 0.0;
    }
    float Bilinear(Texture2D Atlas, float2 UV, float2 Size, int2 First, int2 Last, float Depth) {
        float2 P = UV * Size - .5, F = frac(P);
        int2 I = int2(floor(P));
        return lerp(lerp(Compare(Atlas,I,First,Last,Depth),Compare(Atlas,I+int2(1,0),First,Last,Depth),F.x),
                    lerp(Compare(Atlas,I+int2(0,1),First,Last,Depth),Compare(Atlas,I+int2(1,1),First,Last,Depth),F.x),F.y);
    }
    float2 Vogel(int I, float Phi) {
        float R = sqrt((float(I) + .5) / 5.0), A = float(I) * 2.399963229728653 + Phi;
        return float2(cos(A),sin(A)) * R;
    }
    float Fresnel(float VH) {
        float F = exp2((-5.55473 * VH - 6.98316) * VH);
        return .04 * (1.0-F) + F;
    }
    float Visibility(float Alpha, float NL, float NV) {
        float A2=Alpha*Alpha;
        float GV=NL*sqrt(A2+(1.0-A2)*NV*NV), GL=NV*sqrt(A2+(1.0-A2)*NL*NL);
        return .5/max(GV+GL,1e-6);
    }
    float Distribution(float Alpha, float NH) {
        float A2=Alpha*Alpha, D=NH*NH*(A2-1.0)+1.0;
        return A2/(PI*D*D);
    }
};
TCWaterLight waterLight;
float3 waterDirect=0;
#if PIXELSHADER && IS_BASE_PASS && MATERIALBLENDING_TRANSLUCENT
float shadow=1.0;
float3 translated=LWCHackToFloat(Parameters.WorldPosition_CamRelative);
float sceneDepth=Parameters.ScreenPosition.w;
uint cascade=uint(sceneDepth>=TranslucentBasePass.Shared.ForwardDirLightShadow.CascadeEndDepths.x)
    +uint(sceneDepth>=TranslucentBasePass.Shared.ForwardDirLightShadow.CascadeEndDepths.y)
    +uint(sceneDepth>=TranslucentBasePass.Shared.ForwardDirLightShadow.CascadeEndDepths.z)
    +uint(sceneDepth>=TranslucentBasePass.Shared.ForwardDirLightShadow.CascadeEndDepths.w);
if (cascade<TranslucentBasePass.Shared.ForwardDirLightShadow.NumDirectionalLightCascades) {
    float4 sp=mul(float4(translated,1),TranslucentBasePass.Shared.ForwardDirLightShadow.DirectionalLightTranslatedWorldToShadowMatrix[cascade]);
    float2 uv=sp.xy/sp.w;
    float4 bounds=TranslucentBasePass.Shared.ForwardDirLightShadow.DirectionalLightShadowmapMinMax[cascade];
    // getShadow's frustum test includes the biased receiver depth. Without it,
    // offshore water beyond the light volume compares against every caster as dark.
    float depth=1.0-sp.z+TranslucentBasePass.Shared.ForwardDirLightShadow.DirectionalLightDepthBias.x;
    if (all(uv>=bounds.xy) && all(uv<=bounds.zw) && depth<=1.0) {
        float2 size=TranslucentBasePass.Shared.ForwardDirLightShadow.DirectionalLightShadowmapAtlasBufferSize.xy;
        int2 first=clamp(int2(ceil(bounds.xy*size-.5)),int2(0,0),int2(size)-1);
        int2 last=clamp(int2(floor(bounds.zw*size-.5)),first,int2(size)-1);
        // ForwardShadowingCommon: 1-sp.z < sampledDepth-bias.x. Move the
        // receiver bias to the left before the source comparison filter.
        // Every tap stays inside this tile, matching source ClampToEdge rather
        // than reading the neighbouring native atlas allocation at large radii.
        if (Low>.5) shadow=waterLight.Compare(TranslucentBasePass.Shared.ForwardDirLightShadow.DirectionalLightShadowmapAtlas,int2(uv*size),first,last,depth);
        else {
            // WebGL gl_FragCoord has its origin at the bottom left.
            float2 pixel=float2(Parameters.SvPosition.x,View.ViewSizeAndInvSize.y-Parameters.SvPosition.y);
            float phi=frac(52.9829189*frac(dot(pixel,float2(.06711056,.00583715))))*6.283185307179586;
            shadow=0;
            [unroll] for (int i=0;i<5;i++)
                shadow+=waterLight.Bilinear(TranslucentBasePass.Shared.ForwardDirLightShadow.DirectionalLightShadowmapAtlas,uv+waterLight.Vogel(i,phi)*ShadowRadius/size,size,first,last,depth)*.2;
        }
    }
}
shadow=lerp(1.0,shadow,saturate(SunDirection.a));
float3 n=normalize(NormalWS), v=normalize(CameraVector), l=normalize(SunDirection.xyz), h=normalize(l+v);
float nl=saturate(dot(n,l)), nv=saturate(dot(n,v)), nh=saturate(dot(n,h)), vh=saturate(dot(v,h));
float3 dxy=max(abs(ddx(normalize(VertexNormal))),abs(ddy(normalize(VertexNormal))));
float rough=min(max(Roughness,.0525)+max(max(dxy.x,dxy.y),dxy.z),1.0), alpha=rough*rough;
float2 fab=lut.At(rough,nv);
float compensation=1.0+.04*(1.0/(fab.x+fab.y)-1.0);
float f=waterLight.Fresnel(vh);
float spec=f*waterLight.Visibility(alpha,nl,nv)*waterLight.Distribution(alpha,nh)*compensation;
waterDirect=SunColour.rgb*(nl*shadow)*(Color*(1.0-f)/PI+spec);
#endif
// Fog in the source transparent material uses water's vFogDepth. The native global
// post instead sees the opaque riverbed, including the large apron triangles. This
// exact linear decomposition cancels that extra haze on the water contribution.
float3 waterRadiance=Emissive+indirect+waterDirect;
#if PIXELSHADER
if (WaterFog.z>.5 && WaterFog.y>WaterFog.x) {
    float fw=smoothstep(WaterFog.x,WaterFog.y,Parameters.ScreenPosition.w*.01);
    float fb=smoothstep(WaterFog.x,WaterFog.y,OpaqueDepth*.01);
    if(fb<.9999) waterRadiance=(waterRadiance*(1-fw)+WaterHaze.rgb*(fw-fb))/(1-fb);
}
#endif
)TCWATER";
