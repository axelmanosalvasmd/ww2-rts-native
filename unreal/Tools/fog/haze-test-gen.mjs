// haze-oracle.json -> the C++ rows of ThreeCrossroads.Look.Haze (pasted into Client/TCLighting.cpp)
import fs from 'node:fs';
const d = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
const lines = d.rows.map((r) => `\t\t{ TEXT("${r.mood}"), TEXT("${r.weather}"), ${r.hazeK.toPrecision(9)}f, 0x${r.hex.toString(16).padStart(6, '0')}, ${r.exposure.toFixed(3)}f, ${r.fog140[0].toPrecision(9)}f, ${r.fog140[1].toPrecision(9)}f, { ${r.screenHigh.map((v) => v.toPrecision(7) + 'f').join(', ')} } },`);
fs.writeFileSync(process.argv[3], lines.join('\n') + '\n');
console.log(lines.length);
