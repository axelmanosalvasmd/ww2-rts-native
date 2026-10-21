// client/main.js startGame and applyCells, the parts the painted ground, its decals and its heights read, copied line
// for line (main.js wires the DOM and cannot be imported). Shared by the Node oracle (ground-oracle.mjs) and the browser
// page (tiles-page.mjs), so both replay a start and its cell rows the same way.
export function makeReplica({ ground, worldLayers, startState, levelChar }) {
  function rememberLook(map, cell, data) {
    if (data?.biome !== undefined && map.biome?.[cell] !== data.biome) {
      (map.biome ??= new Uint8Array(map.w * map.h).fill(255))[cell] = data.biome; (map.biomeCells ??= []).push(cell);
    }
  }
  function setLevel(map, cell, lv) {
    map.heights ??= map.rows.map(r => '0'.repeat(r.length));
    const x = cell % map.w, y = Math.floor(cell / map.w), r = map.heights[y];
    map.heights[y] = r.slice(0, x) + levelChar(lv) + r.slice(x + 1);
  }
  function clientStart(m) {
    const map = m.map;
    const gp = ground.createGround(map, null); // no frame adapter: paints synchronously, like the map editor
    const layers = worldLayers(map);
    const terrain = { w: map.w, groundGrid: Array.from({ length: map.h }, (_, y) => layers.ground.slice(y * map.w, (y + 1) * map.w)), objectGrid: Array.from({ length: map.h }, (_, y) => layers.objects.slice(y * map.w, (y + 1) * map.w)), materials: layers.materials, grid: map.rows.map(r => [...r]), ground: gp };
    terrain.state = Uint8Array.from(map.rows.join(''), startState);
    if (map.world) map.discovered = new Uint8Array(map.w * map.h);
    for (const [cell, ch, lv, st, data] of m.cells || []) { if (data) { terrain.groundGrid[Math.floor(cell / map.w)][cell % map.w] = data.ground; terrain.objectGrid[Math.floor(cell / map.w)][cell % map.w] = data.object; terrain.materials.set(cell, data.material); } if (map.discovered) { map.discovered[cell] = 1; rememberLook(map, cell, data); } terrain.grid[Math.floor(cell / map.w)][cell % map.w] = ch; terrain.state[cell] = st ?? 0; if (lv !== undefined) setLevel(map, cell, lv); }
    if (map.world) map.rows = terrain.grid.map(r => r.join(''));
    gp.paint(terrain.grid, terrain.state, terrain.groundGrid, terrain.objectGrid);
    return { map, terrain, gp };
  }
  // returns the cells the rubble (and World crater) decals look at
  function clientCells({ map, terrain }, cells) {
    for (const entry of cells) {
      const [cell, ch, lv, st = 0, data] = entry, x = cell % terrain.w, y = Math.floor(cell / terrain.w);
      terrain.grid[y][x] = ch; terrain.state[cell] = st;
      if (data) { terrain.groundGrid[y][x] = data.ground; terrain.objectGrid[y][x] = data.object; terrain.materials.set(cell, data.material); }
      if (map.world) {
        map.discovered[cell] = 1; rememberLook(map, cell, data);
        const row = map.rows[y]; map.rows[y] = row.slice(0, x) + ch + row.slice(x + 1);
      }
      if (lv !== undefined) setLevel(map, cell, lv);
    }
    terrain.ground.paint(terrain.grid, terrain.state, terrain.groundGrid, terrain.objectGrid, cells);
    const rubbleCells = new Set(cells.map(entry => entry[0]));
    for (const [c, , lv] of cells) if (lv !== undefined) {
      const x = c % terrain.w, y = Math.floor(c / terrain.w);
      for (let dz = -1; dz <= 1; dz++) for (let dx = -1; dx <= 1; dx++) {
        const nx = x + dx, ny = y + dz;
        if (nx >= 0 && nx < terrain.w && ny >= 0 && ny < terrain.grid.length && terrain.objectGrid[ny][nx] === 'R') rubbleCells.add(ny * terrain.w + nx);
      }
    }
    return [...rubbleCells];
  }
  return { clientStart, clientCells, rememberLook, setLevel };
}
