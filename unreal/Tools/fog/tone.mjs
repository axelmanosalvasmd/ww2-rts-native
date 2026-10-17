// three r186 colour helpers for the haze oracle: ACESFilmicToneMapping as its GLSL writes it (tonemapping_pars_fragment)
// and sRGBTransferOETF (colorspace_pars_fragment), on linear colour.
const mul = (m, v) => [m[0][0] * v[0] + m[1][0] * v[1] + m[2][0] * v[2], m[0][1] * v[0] + m[1][1] * v[1] + m[2][1] * v[2], m[0][2] * v[0] + m[1][2] * v[1] + m[2][2] * v[2]];
const IN = [[0.59719, 0.07600, 0.02840], [0.35458, 0.90834, 0.13383], [0.04823, 0.01566, 0.83777]];
const OUT = [[1.60475, -0.10208, -0.00327], [-0.53108, 1.10813, -0.07276], [-0.07367, -0.00605, 1.07602]];
const fit = (v) => (v * (v + 0.0245786) - 0.000090537) / (v * (0.983729 * v + 0.4329510) + 0.238081);
export const aces = (c, exposure) => { let v = c.map((x) => x * exposure / 0.6); v = mul(IN, v).map(fit); v = mul(OUT, v); return v.map((x) => Math.min(1, Math.max(0, x))); };
export const oetf = (x) => (x < 0.0031308 ? x * 12.92 : 1.055 * Math.pow(x, 0.41666) - 0.055);
// what the canvas shows for a linear colour drawn through the composer (OutputPass: ACES at the exposure, then sRGB)
export const screen = (c, exposure) => aces(c, exposure).map(oetf).map((x) => x * 255);
