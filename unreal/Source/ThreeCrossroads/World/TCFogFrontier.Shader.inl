// surfaces.js MIST at e4e579a, with x/z metres in native north-to-south texture coordinates.
// Used verbatim by Tools/world/make-fog-materials.py. The CPU equivalents live in TCFogFrontier.h.
inline constexpr const char TCFogMistShader[] = R"TCFOG(
struct TCFogMist
{
    float Hash(float2 p) {
        p = frac(p * float2(0.1031, 0.1030));
        p += dot(p, p.yx + 33.33);
        return frac((p.x + p.y) * p.x);
    }
    float Noise(float2 p) {
        float2 i = floor(p), f = frac(p);
        f = f * f * (3.0 - 2.0 * f);
        return lerp(lerp(Hash(i), Hash(i + float2(1, 0)), f.x),
                    lerp(Hash(i + float2(0, 1)), Hash(i + float2(1, 1)), f.x), f.y);
    }
    float Alpha(float a, float2 p, float time) {
        float m = Noise(p * .05 + time * float2(.04, .015)) * .65
                + Noise(p * .13 - time * float2(.02, .05)) * .35;
        return saturate(a + (m - .5) * 1.4 * a * (1.0 - a));
    }
};
TCFogMist fow;
)TCFOG";
