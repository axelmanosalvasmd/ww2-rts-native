// The ground's units. The surface itself (what is drawn, what HeightAt and RayToGround read) is the reference's relief,
// World/TCRelief.h (client/relief.js); ATCTerrain (World/TCTerrain.h) builds and draws it. This header keeps the two
// sizes every ground user shares, engine-free.
#pragma once

namespace TCGround
{
	inline constexpr float CellMetres = 2.f;   // shared/sim.js CELL
	inline constexpr float LevelMetres = 2.5f; // shared/sim.js CFG.levelHeight
}
