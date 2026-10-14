// client/wheel-motion.js wheelMaterial + client/track-motion.js trackShader, as an Unreal material Custom node (world
// position offset). NOT WIRED YET: the hull meshes as imported (unreal/Tools/models/import-models.py) keep UV0-3 only,
// so they carry no wheel pivots or track data. To adopt, the importer writes four more channels from the glb's
// _WHEELPIVOT, _WHEELSIDE and _TRACKDATA (the exporter already mirrors them, unreal/Tools/models/README.md):
//   UV4 = wheelPivot.xy            the wheel centre in the reference's hull space (metres, three.js x and y)
//   UV5 = (wheelSide, wheelPivot.w) the track side the wheel rolls with (else its pivot z) and its radius (0: static)
//   UV6 = (trackData.x, trackData.y) phase along the loop and the loop's row in the path table (-1: not a track link;
//                                   the importer must write -1, not leave 0, for every other vertex)
//   UV7 = (trackData.z, trackData.w) the link's side (for the turn) and its floor
// with full precision UVs, the path table as a texture parameter Paths (Client/TCUnitMotion.h FTCTrackPaths, the
// reference's 1024 x 128 RGBA32F layout, rows registered in the importer's order) and per-instance custom data
// 8 and 9 = FTCPartMotion WheelTravel and WheelTurn (FTCUnitMotion::Update). The normal turns the same way.
// Inputs: P (LocalPosition, cm), Pivot (UV4), Wheel (UV5), Track0 (UV6), Track1 (UV7), Travel (float2), Paths.
// Output: the local offset (cm); the material carries it to world space as the crowd skinning does.
float3 t = float3(P.x, P.z, P.y) * 0.01;                     // Unreal cm -> the reference's hull metres
float a = Wheel.y > 0 ? -(Travel.x + Travel.y * Wheel.x) / Wheel.y : 0;   // wheelAngle()
float2 q = t.xy - Pivot;
float c = cos(a), s = sin(a);
t.xy = Pivot + float2(c * q.x - s * q.y, s * q.x + c * q.y);
if (Track0.y >= 0)
{
    int row = (int)(Track0.y + 0.5);
    float length = Paths.Load(int3(0, row, 0)).z;
    // trackFrame(phase): the sampled point and direction at a phase
    float at0 = frac(Track0.x) * 1024.0;
    int i0 = (int)floor(at0);
    float4 a0 = Paths.Load(int3(i0, row, 0)), b0 = Paths.Load(int3((i0 + 1) % 1024, row, 0));
    float2 p0 = lerp(a0.xy, b0.xy, frac(at0)), d0 = normalize(b0.xy - a0.xy);
    float at1 = frac(Track0.x - (Travel.x + Travel.y * Track1.x) / length) * 1024.0;
    int i1 = (int)floor(at1);
    float4 a1 = Paths.Load(int3(i1, row, 0)), b1 = Paths.Load(int3((i1 + 1) % 1024, row, 0));
    float2 p1 = lerp(a1.xy, b1.xy, frac(at1)), d1 = normalize(b1.xy - a1.xy);
    // trackTurn: carry the link from its rest frame to its travelled frame
    float tc = dot(d0, d1), ts = d0.x * d1.y - d0.y * d1.x;
    float2 r = t.xy - p0;
    t.xy = p1 + float2(tc * r.x - ts * r.y, ts * r.x + tc * r.y);
    t.y = max(t.y, Track1.y);
}
return float3(t.x, t.z, t.y) * 100.0 - P;
