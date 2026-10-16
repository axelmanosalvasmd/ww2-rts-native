//! Detached World supply graph and region rates. No Match or room owns this state yet.
use crate::{
    catalog::Catalog,
    game::{CELL, MOVE, Point, VBLOCK},
    math,
    objectives::{Objective, ObjectivePlayer},
    supply_network::SupplyUnit,
    world::World,
};

/// Canonical terrain and immutable region membership borrowed for one graph rebuild.
#[derive(Clone, Copy)]
pub struct WorldSupplyTerrain<'a> {
    pub width: usize,
    pub height: usize,
    pub version: u32,
    pub flags: &'a [u16],
    pub chars: &'a [u8],
    pub heights: Option<&'a [i8]>,
    pub initial_chars: &'a [u8],
    pub initial_heights: Option<&'a [i8]>,
    pub region_map: &'a [i16],
    pub region_count: usize,
}
impl<'a> WorldSupplyTerrain<'a> {
    pub fn from_world(world: &'a World) -> Result<Self, String> {
        let geography = world.geography().ok_or("missing World geography")?;
        Ok(Self {
            width: world.game.w,
            height: world.game.h,
            version: world.game.terrain_version,
            flags: &world.game.flags,
            chars: &world.game.chars,
            heights: world.game.height.as_deref(),
            initial_chars: &world.initial().chars,
            initial_heights: world.initial().height.as_deref(),
            region_map: geography
                .region_map
                .as_deref()
                .ok_or("missing region map")?,
            region_count: geography.regions.len(),
        })
    }
    fn validate(self) -> Result<(), String> {
        let n = self
            .width
            .checked_mul(self.height)
            .ok_or("invalid World supply shape")?;
        if n == 0
            || self.region_count == 0
            || self.flags.len() != n
            || self.chars.len() != n
            || self.initial_chars.len() != n
            || self.heights.is_some_and(|v| v.len() != n)
            || self.initial_heights.is_some_and(|v| v.len() != n)
        {
            return Err("invalid World supply shape".into());
        }
        validate_regions(self.region_map, n, self.region_count)
    }
    fn fillable(self, c: usize) -> bool {
        match self.chars[c] {
            b'+' => true,
            b'R' => self.initial_chars[c] != b'R',
            b'F' => self.initial_chars[c] != b'F',
            b'.' => level(self.heights, c) < level(self.initial_heights, c),
            _ => false,
        }
    }
}
fn validate_regions(map: &[i16], cells: usize, regions: usize) -> Result<(), String> {
    if map.len() != cells
        || map
            .iter()
            .any(|&r| r < -1 || r >= 0 && r as usize >= regions)
    {
        return Err("invalid World supply region map".into());
    }
    Ok(())
}
fn level(heights: Option<&[i8]>, c: usize) -> i16 {
    heights.map_or(0, |v| i16::from(v[c]))
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldSupplyGraph {
    key: Option<u32>,
    edges: Vec<Vec<usize>>,
    damage: Vec<f32>,
}
impl WorldSupplyGraph {
    pub fn terrain_version(&self) -> Option<u32> {
        self.key
    }
    pub fn edges(&self) -> &[Vec<usize>] {
        &self.edges
    }
    pub fn damage(&self) -> &[f32] {
        &self.damage
    }
    /// The region map is immutable for a cache's lifetime. A terrain-version change rebuilds it.
    /// Invalid input leaves the previous graph intact.
    pub fn refresh(&mut self, terrain: WorldSupplyTerrain<'_>) -> Result<bool, String> {
        terrain.validate()?;
        if self.key == Some(terrain.version) {
            return Ok(false);
        }
        let mut edges = vec![Vec::new(); terrain.region_count];
        let mut damage = vec![0.0f32; terrain.region_count];
        let open = |c: usize| terrain.flags[c] & (MOVE | VBLOCK) == 0;
        for (c, &a) in terrain.region_map.iter().enumerate() {
            if a < 0 {
                continue;
            }
            let a = a as usize;
            if terrain.fillable(c) {
                damage[a] += 1.0;
            }
            if !open(c) {
                continue;
            }
            for nb in [
                (c % terrain.width + 1 < terrain.width).then_some(c + 1),
                (c + terrain.width < terrain.region_map.len()).then_some(c + terrain.width),
            ]
            .into_iter()
            .flatten()
            {
                let b = terrain.region_map[nb];
                if b < 0
                    || b as usize == a
                    || !open(nb)
                    || (level(terrain.heights, c) - level(terrain.heights, nb)).abs() > 1
                {
                    continue;
                }
                let b = b as usize;
                if !edges[a].contains(&b) {
                    edges[a].push(b);
                }
                if !edges[b].contains(&a) {
                    edges[b].push(a);
                }
            }
        }
        *self = Self {
            key: Some(terrain.version),
            edges,
            damage,
        };
        Ok(true)
    }
}

pub struct WorldSupplyInput<'a> {
    pub width: usize,
    pub height: usize,
    pub region_map: &'a [i16],
    /// Indexed by region vector position, never the authored sparse region ID.
    pub region_teams: &'a [i32],
    pub players: &'a [ObjectivePlayer],
    pub units: &'a [SupplyUnit<'a>],
    pub points: &'a [Objective],
}
#[derive(Clone, Debug, PartialEq)]
pub struct WorldTeamSupply {
    pub team: i32,
    pub cost: Vec<f32>,
    pub rate: Vec<f32>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldSupplyNetwork {
    pub teams: Vec<WorldTeamSupply>,
}
impl WorldSupplyNetwork {
    /// Compute one detached network. Neutral armed guards and retreating units count as occupants.
    pub fn compute(graph: &WorldSupplyGraph, input: WorldSupplyInput<'_>) -> Result<Self, String> {
        let n = input.region_teams.len();
        let cells = input
            .width
            .checked_mul(input.height)
            .ok_or("invalid World supply shape")?;
        if cells == 0 || n == 0 || graph.key.is_none() || graph.edges.len() != n {
            return Err("invalid World supply graph".into());
        }
        validate_regions(input.region_map, cells, n)?;
        let valid_at = |p: Point| p.x.is_finite() && p.z.is_finite();
        if input.players.iter().any(|p| !valid_at(p.spawn))
            || input.units.iter().any(|u| {
                !valid_at(u.fact.at)
                    || !u.fact.hp.is_finite()
                    || u.built.is_some_and(|v| !v.is_finite())
            })
            || input
                .points
                .iter()
                .any(|p| !valid_at(p.at) || p.owner.is_some_and(|o| o >= input.players.len()))
        {
            return Err("invalid World supply facts".into());
        }
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let rules = catalog
            .table("CFG")
            .map_err(|e| e.to_string())?
            .get("supply")
            .ok_or("missing World supply rules")?;
        let damage = rules
            .get("damage")
            .and_then(|v| v.number())
            .ok_or("missing damage rate")?;
        let hop = rules
            .get("hop")
            .and_then(|v| v.number())
            .ok_or("missing hop rate")?;
        let floor = rules
            .get("floor")
            .and_then(|v| v.number())
            .ok_or("missing rate floor")?;
        let region_at = |at: Point| {
            let x = (at.x / CELL).floor();
            let y = (at.z / CELL).floor();
            if x < 0.0 || y < 0.0 || x >= input.width as f64 || y >= input.height as f64 {
                return None;
            }
            let r = input.region_map[y as usize * input.width + x as usize];
            (r >= 0).then_some(r as usize)
        };
        let mut armed: Vec<Vec<i32>> = vec![Vec::new(); n];
        for u in input.units {
            let def = catalog.unit(u.fact.kind).map_err(|e| e.to_string())?;
            let range = def
                .get("w")
                .and_then(|w| w.get("range"))
                .and_then(|v| v.number())
                .unwrap_or(0.0);
            if u.fact.hp <= 0.0 || u.fact.airborne || u.fact.riding || range == 0.0 {
                continue;
            }
            if let Some(r) = region_at(u.fact.at) {
                let team = input.players.get(u.fact.owner).map_or(-1, |p| p.team);
                if !armed[r].contains(&team) {
                    armed[r].push(team);
                }
            }
        }
        let mut result = Self::default();
        for player in input.players.iter().filter(|p| !p.out) {
            let team = player.team;
            if result.teams.iter().any(|t| t.team == team) {
                continue;
            }
            let usable = |r: usize| {
                input.region_teams[r] == team && (armed[r].is_empty() || armed[r].contains(&team))
            };
            let mut sources: Vec<_> = input
                .units
                .iter()
                .filter(|u| {
                    u.fact.kind == "hq"
                        && u.fact.hp > 0.0
                        && u.built.unwrap_or(1.0) >= 1.0
                        && input
                            .players
                            .get(u.fact.owner)
                            .is_some_and(|p| p.team == team && !p.out)
                })
                .map(|u| u.fact.at)
                .collect();
            if sources.is_empty() {
                sources.extend(
                    input
                        .players
                        .iter()
                        .filter(|p| p.team == team && !p.out)
                        .map(|p| p.spawn),
                );
            }
            sources.extend(
                input
                    .points
                    .iter()
                    .filter(|p| {
                        p.owner.is_some_and(|o| input.players[o].team == team)
                            && !p.cut
                            && !p.contested
                            && p.kind.as_deref() == Some("depot")
                    })
                    .map(|p| p.at),
            );
            let mut cost = vec![f32::INFINITY; n];
            let mut done = vec![false; n];
            for r in sources
                .into_iter()
                .filter_map(region_at)
                .filter(|&r| usable(r))
            {
                cost[r] = 0.0;
            }
            loop {
                let mut cheapest: Option<usize> = None;
                for r in 0..n {
                    if !done[r]
                        && cost[r] < f32::INFINITY
                        && cheapest.is_none_or(|a| cost[r] < cost[a])
                    {
                        cheapest = Some(r);
                    }
                }
                let Some(a) = cheapest else { break };
                done[a] = true;
                for &b in &graph.edges[a] {
                    if !done[b] && usable(b) {
                        // The expression is binary64, then the Float32Array assignment rounds it.
                        cost[b] = math::min(
                            f64::from(cost[b]),
                            f64::from(cost[a]) + 1.0 + f64::from(graph.damage[b]) * damage,
                        ) as f32;
                    }
                }
            }
            let rate = cost
                .iter()
                .map(|&c| {
                    if c == f32::INFINITY {
                        0.0
                    } else {
                        math::max(floor, 1.0 - hop * math::max(0.0, f64::from(c) - 1.0)) as f32
                    }
                })
                .collect();
            result.teams.push(WorldTeamSupply { team, cost, rate });
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{game::LOCAL_DEFENDER_OWNER, objectives::ObjectiveUnit};
    use serde_json::{Value, json};

    fn fixture() -> Value {
        serde_json::from_str(include_str!(
            "../tests/fixtures/world-supply-components.json"
        ))
        .unwrap()
    }
    struct TerrainCase {
        width: usize,
        height: usize,
        version: u32,
        flags: Vec<u16>,
        chars: Vec<u8>,
        initial_chars: Vec<u8>,
        heights: Option<Vec<i8>>,
        initial_heights: Option<Vec<i8>>,
        region_map: Vec<i16>,
        region_count: usize,
    }
    fn heights(value: &Value) -> Option<Vec<i8>> {
        value
            .as_array()
            .map(|a| a.iter().map(|v| v.as_i64().unwrap() as i8).collect())
    }
    impl TerrainCase {
        fn parse(v: &Value) -> Self {
            Self {
                width: v["w"].as_u64().unwrap() as usize,
                height: v["h"].as_u64().unwrap() as usize,
                version: v["version"].as_u64().unwrap() as u32,
                flags: v["flags"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap() as u16)
                    .collect(),
                chars: v["chars"].as_str().unwrap().as_bytes().to_vec(),
                initial_chars: v["initial"].as_str().unwrap().as_bytes().to_vec(),
                heights: heights(&v["heights"]),
                initial_heights: heights(&v["initialHeights"]),
                region_map: v["map"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_i64().unwrap() as i16)
                    .collect(),
                region_count: v["n"].as_u64().unwrap() as usize,
            }
        }
        fn view(&self) -> WorldSupplyTerrain<'_> {
            WorldSupplyTerrain {
                width: self.width,
                height: self.height,
                version: self.version,
                flags: &self.flags,
                chars: &self.chars,
                heights: self.heights.as_deref(),
                initial_chars: &self.initial_chars,
                initial_heights: self.initial_heights.as_deref(),
                region_map: &self.region_map,
                region_count: self.region_count,
            }
        }
        fn edit(&mut self, v: &Value) {
            self.version = v["version"].as_u64().unwrap() as u32;
            if let Some(a) = v["flags"].as_array() {
                self.flags = a.iter().map(|v| v.as_u64().unwrap() as u16).collect();
            }
            if let Some(s) = v["chars"].as_str() {
                self.chars = s.as_bytes().to_vec();
            }
            if v.get("heights").is_some() {
                self.heights = heights(&v["heights"]);
            }
        }
    }
    fn bits(values: &[f32]) -> Vec<u32> {
        values.iter().map(|v| v.to_bits()).collect()
    }
    fn at(v: &Value) -> Point {
        Point {
            x: v["x"].as_f64().unwrap(),
            z: v["z"].as_f64().unwrap(),
        }
    }

    #[test]
    fn log26_world_graph_adjacency_damage_and_version_cache_match_original_js() {
        let fixture = fixture();
        assert_eq!(
            fixture["reference"],
            "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
        );
        for case in fixture["graph"].as_array().unwrap() {
            let mut terrain = TerrainCase::parse(&case["terrain"]);
            let mut graph = WorldSupplyGraph::default();
            for (i, expected) in case["expected"].as_array().unwrap().iter().enumerate() {
                if i > 0 {
                    terrain.edit(&case["edits"][i - 1]);
                }
                let rebuilt = graph.refresh(terrain.view()).unwrap();
                assert_eq!(
                    json!({
                        "key": graph.terrain_version().unwrap(),
                        "reused": !rebuilt,
                        "edges": graph.edges(),
                        "damage": bits(graph.damage()),
                    }),
                    *expected,
                    "{} state {i}",
                    case["name"]
                );
            }
            let previous = graph.clone();
            terrain.version += 1;
            terrain.region_map[0] = terrain.region_count as i16;
            assert!(graph.refresh(terrain.view()).is_err());
            assert_eq!(
                graph, previous,
                "invalid input must preserve the cached graph"
            );
        }
    }

    #[test]
    fn log27_world_usable_regions_sources_and_f32_rates_match_original_js() {
        let fixture = fixture();
        for case in fixture["rates"].as_array().unwrap() {
            let terrain = TerrainCase::parse(&case["terrain"]);
            let mut graph = WorldSupplyGraph::default();
            graph.refresh(terrain.view()).unwrap();
            let teams: Vec<_> = case["teams"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_i64().unwrap() as i32)
                .collect();
            let players: Vec<_> = case["players"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| ObjectivePlayer {
                    team: p["team"].as_i64().unwrap() as i32,
                    out: p["out"].as_bool().unwrap(),
                    spawn: at(&p["spawn"]),
                })
                .collect();
            let units: Vec<_> = case["units"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .map(|(id, u)| SupplyUnit {
                    fact: ObjectiveUnit {
                        id: id as u32,
                        owner: u["owner"]
                            .as_u64()
                            .map_or(LOCAL_DEFENDER_OWNER, |o| o as usize),
                        kind: u["type"].as_str().unwrap(),
                        at: at(u),
                        hp: u["hp"].as_f64().unwrap(),
                        airborne: u["air"].as_bool().unwrap_or(false),
                        riding: u["riding"].as_u64().is_some_and(|v| v != 0),
                        retreating: u["retreating"].as_bool().unwrap_or(false),
                        forced_reserve: false,
                    },
                    built: u["built"].as_f64(),
                })
                .collect();
            let points: Vec<_> = case["points"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| Objective {
                    id: p["id"].clone(),
                    at: at(p),
                    owner: p["owner"].as_u64().map(|o| o as usize),
                    kind: p["kind"].as_str().map(str::to_owned),
                    cut: p["cut"].as_bool().unwrap(),
                    contested: p["contested"].as_bool().unwrap(),
                    vp: 0.0,
                    mp: 0.0,
                    needs: None,
                    capper: None,
                    progress: 0.0,
                    on_point: Vec::new(),
                })
                .collect();
            let network = WorldSupplyNetwork::compute(
                &graph,
                WorldSupplyInput {
                    width: terrain.width,
                    height: terrain.height,
                    region_map: &terrain.region_map,
                    region_teams: &teams,
                    players: &players,
                    units: &units,
                    points: &points,
                },
            )
            .unwrap();
            let expected = case["expected"].as_array().unwrap();
            assert_eq!(network.teams.len(), expected.len(), "{}", case["name"]);
            for row in expected {
                let team = row["team"].as_i64().unwrap() as i32;
                let actual = network.teams.iter().find(|t| t.team == team).unwrap();
                assert_eq!(
                    json!(bits(&actual.rate)),
                    row["rate"],
                    "{} team {team}",
                    case["name"]
                );
            }
            let live_teams = players
                .iter()
                .filter(|p| !p.out)
                .fold(Vec::new(), |mut teams, p| {
                    if !teams.contains(&p.team) {
                        teams.push(p.team);
                    }
                    teams
                });
            assert_eq!(
                network.teams.iter().map(|t| t.team).collect::<Vec<_>>(),
                live_teams
            );
        }
    }
}
