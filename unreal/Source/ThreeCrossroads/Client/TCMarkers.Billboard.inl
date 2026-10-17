// Consumed by Tools/markers/make-marker-materials.py. Source: Three r186 fog_fragment,
// tonemapping_pars_fragment and colorspace_pars_fragment. Texture is decoded from sRGB once.
static constexpr const char TCMarkerBillboardShader[] = R"TCBILLBOARD(
struct TCMarkerColour
{
    float3 aces(float3 c, float exposure)
    {
        c *= exposure / 0.6;
        c = mul(float3x3(0.59719,0.35458,0.04823, 0.07600,0.90834,0.01566, 0.02840,0.13383,0.83777), c);
        c = (c * (c + 0.0245786) - 0.000090537) / (c * (0.983729 * c + 0.4329510) + 0.238081);
        return saturate(mul(float3x3(1.60475,-0.53108,-0.07367, -0.10208,1.10813,-0.00605, -0.00327,-0.07276,1.07602), c));
    }
    float3 display(float3 c)
    {
        return lerp(1.055 * pow(max(c, 0), 1.0 / 2.4) - 0.055, c * 12.92, step(c, 0.0031308));
    }
    float3 toLinear(float3 c)
    {
        return lerp(pow((c + 0.055) / 1.055, 2.4), c / 12.92, step(c, 0.04045));
    }
};
TCMarkerColour colour;
float4 Texel = Texture2DSample(Badge, BadgeSampler, UV);
float3 rgb;
if (BeforeTone > 0.5) rgb = colour.aces(lerp(Texel.rgb, Haze, saturate(Fog)), Exposure);
else rgb = colour.toLinear(lerp(colour.display(colour.aces(Texel.rgb, Exposure)), colour.display(Haze), saturate(Fog)));
// FCanvas material tiles use the base pass directly, with no output transfer.
// Their destination is the already encoded HUD backbuffer, unlike Slate UI materials.
return float4(colour.display(rgb), Texel.a);
)TCBILLBOARD";
