//! Single-commander World geography, panel and observed-list fragments.
//! This internal read does not assemble a snapshot, drain terrain cursors or adopt a wire contract.
use crate::{
    catalog::{Catalog, Value as CatalogValue},
    economy::Mode,
    economy_state::{Context, ModeContext, PendingDrop, Unit as EconomyUnit, WorldContext},
    game::{CELL, LOCAL_DEFENDER_OWNER, ModeKind, Point},
    match_runtime::Match,
    math,
    recipient_lists::{RecipientLists, display_rows},
    world_map::WorldSize,
    world_memory::{KnownRegion, TeamMemory},
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq)]
pub struct WorldRecovery {
    pub hq: f64,
    pub engineer: f64,
    pub available: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct KnownStation {
    pub region: usize,
    pub at: Point,
}
#[derive(Clone, Debug, PartialEq)]
pub struct WorldPanel {
    pub home: Point,
    pub size: WorldSize,
    pub total: usize,
    pub goal: usize,
    pub owned: usize,
    pub cap: f64,
    pub regions: Vec<KnownRegion>,
    pub stations: Option<Vec<KnownStation>>,
    pub recovery: WorldRecovery,
}
#[derive(Clone, Debug, PartialEq)]
pub struct KnownMaterial {
    pub cell: usize,
    pub material: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct KnownWorldMap {
    pub width: usize,
    pub height: usize,
    pub size: WorldSize,
    pub total: usize,
    pub rows: Vec<String>,
    pub heights: Vec<String>,
    pub ground: Vec<String>,
    pub objects: Vec<String>,
    pub mines: Vec<String>,
    pub materials: Vec<KnownMaterial>,
    pub ground_materials: Vec<KnownMaterial>,
    /// Enemy spawns remain absent even when reveal exposes geography.
    pub spawns: Vec<Option<[f64; 2]>>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct WorldRecipientMetadata {
    pub world: WorldPanel,
    pub lists: RecipientLists,
    pub geography: Option<KnownWorldMap>,
}
fn rounded(n: f64) -> Result<f64, String> {
    let value = math::round(n * 10.0) / 10.0;
    value
        .is_finite()
        .then_some(value)
        .ok_or("invalid rounded World metadata number".into())
}
fn source_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64() != Some(0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}
fn number(n: f64) -> Value {
    if n == 0.0 { json!(0) } else { json!(n) }
}
fn rule(v: &CatalogValue, field: &str) -> Result<f64, String> {
    v.get(field)
        .and_then(CatalogValue::number)
        .filter(|n| n.is_finite())
        .ok_or_else(|| format!("missing World metadata rule {field}"))
}
impl Match {
    /// Capture one actual commander seat. Every fallible projection runs before
    /// journal acknowledgment or publication to the existing team memory owner.
    pub fn world_recipient_metadata(
        &mut self,
        owner: usize,
        include_geography: bool,
    ) -> Result<WorldRecipientMetadata, String> {
        if self.options.mode != Mode::World
            || self.world.game.mode != Some(ModeKind::World)
            || owner == LOCAL_DEFENDER_OWNER
        {
            return Err("World metadata requires a World commander seat".into());
        }
        let team = self.validate_world_memory_context(owner)?;
        let runtime = self.world_runtime.as_ref().unwrap();
        let game = &self.world.game;
        if !(0..6).contains(&team)
            || self.start.economy.players.len() != game.players.len()
            || self
                .start
                .economy
                .players
                .iter()
                .zip(&game.players)
                .any(|(a, b)| a.team != b.team)
            || self
                .start
                .players
                .iter()
                .any(|p| !p.spawn.x.is_finite() || !p.spawn.z.is_finite())
            || self.world.mine_seen.keys().any(|c| *c >= game.chars.len())
            || runtime.nodes.iter().any(|n| {
                n.cell >= game.chars.len()
                    || ![n.at.x, n.at.z, n.rate].iter().all(|n| n.is_finite())
            })
            || runtime.rails.stations.iter().any(|s| {
                s.cell >= game.chars.len()
                    || s.region >= runtime.regions.len()
                    || ![s.x, s.z].iter().all(|n| n.is_finite())
            })
        {
            return Err("invalid World metadata facts".into());
        }
        let accepted = crate::match_world_memory::collect_world_wrecks(
            game,
            &self.rules,
            team,
            None,
            &self.environment,
        )?;
        let prior = runtime
            .memory
            .team(team)
            .map_or(&[][..], |m| m.wrecks.as_slice());
        let wreck_memory = crate::match_world_memory::staged_world_wreck_memory(
            prior,
            game,
            &self.rules,
            team,
            None,
            accepted,
        );
        let wrecks = display_rows(&wreck_memory)?;
        // This transient component copy includes existing cursors and route caches.
        // No Game, Match or second persistent observation authority is created.
        let mut candidate = runtime.memory.clone();
        let (_, receipt) =
            candidate.refresh_impl(&self.world, &self.rules, team, &runtime.rails, None, true)?;
        let regions = self.project_world_regions(owner, &candidate)?;
        let memory = candidate
            .team(team)
            .ok_or("missing World metadata memory")?;
        let nodes = runtime
            .nodes
            .iter()
            .filter(|n| game.reveal || memory.contains_cell(n.cell))
            .map(|n| {
                Ok([
                    rounded(n.at.x)?,
                    rounded(n.at.z)?,
                    n.rate,
                    f64::from(u8::from(n.fuel)),
                ])
            })
            .collect::<Result<Vec<_>, String>>()?;
        let found_mines = self
            .world
            .mine_seen
            .iter()
            .filter_map(|(&c, &bits)| {
                ((bits >> team & 1) != 0 && game.chars[c] == b'N').then_some(c)
            })
            .collect();
        let panel = self.project_world_panel(owner, memory, regions.clone())?;
        let geography = include_geography
            .then(|| self.project_known_world_map(owner, memory))
            .transpose()?;
        let target = candidate.team_mut(team).unwrap();
        target.regions = regions;
        target.wreck_queries = wreck_memory.iter().map(|r| r.query.clone()).collect();
        target.wrecks = wreck_memory;
        let result = WorldRecipientMetadata {
            world: panel,
            lists: RecipientLists {
                wrecks,
                found_mines,
                nodes: Some(nodes),
            },
            geography,
        };
        receipt.commit(&mut self.world)?;
        // Publication moves copied arrays for every team. Keep each matching
        // derived graph bound to those buffers without changing its history.
        for (team, memory) in &runtime.memory.teams {
            let prepared = candidate
                .team(*team)
                .expect("preview retains existing teams");
            self.commands
                .paths
                .rebind_terrain(memory.query().navigation(), prepared.query().navigation());
        }
        self.world_runtime.as_mut().unwrap().memory = candidate;
        Ok(result)
    }
    fn project_world_panel(
        &self,
        owner: usize,
        memory: &TeamMemory,
        regions: Vec<KnownRegion>,
    ) -> Result<WorldPanel, String> {
        let game = &self.world.game;
        let runtime = self.world_runtime.as_ref().unwrap();
        let geography = self.world.geography().unwrap();
        let player = &self.start.players[owner];
        let team = game.players[owner].team;
        let mut nearest = None;
        for (definition, live) in geography.regions.iter().zip(&runtime.regions) {
            if live.team == team {
                let at = Point {
                    x: (definition.x as f64 + 0.5) * CELL,
                    z: (definition.y as f64 + 0.5) * CELL,
                };
                let distance = math::hypot(at.x - player.spawn.x, at.z - player.spawn.z);
                if nearest.is_none_or(|(_, old)| distance < old) {
                    nearest = Some((at, distance));
                }
            }
        }
        let home = nearest.map_or(player.spawn, |(at, _)| at);
        let home = Point {
            x: rounded(home.x)?,
            z: rounded(home.z)?,
        };
        let stations = runtime
            .rails
            .stations
            .iter()
            .filter(|s| memory.contains_cell(s.cell))
            .map(|s| {
                Ok(KnownStation {
                    region: s.region,
                    at: Point {
                        x: rounded(s.x)?,
                        z: rounded(s.z)?,
                    },
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        // Source setupWorld calls setupRails only for truthy authored rails.
        // The immutable authoring fact distinguishes absent stations from a present empty list.
        let stations = geography
            .metadata
            .get("rails")
            .is_some_and(source_truthy)
            .then_some(stations);
        let queues: Vec<Vec<&str>> = game
            .units
            .values()
            .map(|u| {
                self.production
                    .building(u.id)
                    .map_or_else(Vec::new, |p| p.queue().collect())
            })
            .collect();
        let units: Vec<_> = game
            .units
            .values()
            .zip(&queues)
            .map(|(u, q)| EconomyUnit {
                id: u64::from(u.id),
                owner: (u.owner != LOCAL_DEFENDER_OWNER).then_some(u.owner),
                kind: &self.rules.kinds[u.kind].name,
                hp: u.hp,
                built: u.built.unwrap_or(0.0),
                queue: q,
            })
            .collect();
        let drops = game
            .strikes
            .iter()
            .map(|s| {
                let kind = match &s.runtime {
                    Some(timing) => {
                        if (s.kind == crate::game::StrikeKind::Recon)
                            != (timing.kind == crate::match_support::SupportKind::Recon)
                        {
                            return Err(
                                "World metadata strike kind does not match canonical timing"
                                    .to_string(),
                            );
                        }
                        timing.kind.name()
                    }
                    None if s.kind == crate::game::StrikeKind::Recon => "recon",
                    None => return Err("World metadata needs canonical strike timing".to_string()),
                };
                Ok(PendingDrop {
                    owner: s.owner,
                    kind,
                    live: s.live,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let economy_regions = self.world_economy_regions()?;
        let cap = self
            .start
            .economy
            .population_cap(
                &Context {
                    mode: ModeContext {
                        kind: Mode::World,
                        defender_team: None,
                        income_excluded_slot: None,
                    },
                    army: self.options.army,
                    units: &units,
                    drops: &drops,
                    points: &[],
                    nodes: &[],
                    world: Some(WorldContext {
                        regions: &economy_regions,
                    }),
                },
                owner,
            )
            .map_err(|e| e.to_string())?;
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let tuning = catalog.table("WORLD_TUNING").map_err(|e| e.to_string())?;
        let total = geography.regions.len();
        let distinct: BTreeSet<_> = game.players.iter().map(|p| p.team).collect();
        let goal = if distinct.len() > 1 {
            (total as f64 * rule(tuning, "winShare")?).ceil() as usize
        } else {
            total
        };
        let own = |kind: &str| {
            game.units
                .values()
                .any(|u| u.owner == owner && u.hp > 0.0 && self.rules.kinds[u.kind].name == kind)
        };
        if regions.iter().any(|r| {
            ![r.at.x, r.at.z, r.progress].iter().all(|n| n.is_finite())
                || r.bounds.is_some_and(|b| b.iter().any(|n| !n.is_finite()))
        }) {
            return Err("invalid remembered World region facts".into());
        }
        Ok(WorldPanel {
            home,
            size: geography.size,
            total,
            goal,
            owned: runtime.regions.iter().filter(|r| r.team == team).count(),
            cap,
            regions,
            stations,
            recovery: WorldRecovery {
                hq: rule(tuning, "recoverHQ")?,
                engineer: rule(catalog.unit("engineer").map_err(|e| e.to_string())?, "cost")?,
                available: !self.start.economy.players[owner].out
                    && (!own("hq") || !own("engineer")),
            },
        })
    }
    fn project_known_world_map(
        &self,
        owner: usize,
        memory: &TeamMemory,
    ) -> Result<KnownWorldMap, String> {
        let game = &self.world.game;
        let geography = self.world.geography().unwrap();
        let cells = game.chars.len();
        let mut rows = vec![b'.'; cells];
        let mut ground = rows.clone();
        let mut objects = rows.clone();
        let mut mines = rows.clone();
        let mut heights = vec!["0".to_owned(); cells];
        let mut materials = Vec::new();
        let mut ground_materials = Vec::new();
        for known in memory.rows() {
            let r = &known.observed.row;
            let c = r.cell;
            let d = &r.data;
            if c >= cells {
                return Err("invalid remembered World cell".into());
            }
            rows[c] = if r.ch == b'K' { d.ground } else { r.ch };
            ground[c] = d.ground;
            objects[c] = if r.ch == b'K' { b'.' } else { d.object };
            mines[c] = if r.ch == b'N' { b'N' } else { b'.' };
            heights[c] = if r.height < 0 {
                char::from_u32((96 - i32::from(r.height)) as u32)
                    .ok_or("invalid World height character")?
                    .to_string()
            } else {
                r.height.to_string()
            };
            if !d.material.is_empty() {
                materials.push(KnownMaterial {
                    cell: c,
                    material: d.material.clone(),
                });
            }
            if !d.ground_material.is_empty() {
                ground_materials.push(KnownMaterial {
                    cell: c,
                    material: d.ground_material.clone(),
                });
            }
        }
        let strings = |cells: Vec<u8>| -> Result<Vec<String>, String> {
            cells
                .chunks(game.w)
                .map(|row| {
                    String::from_utf8(row.to_vec()).map_err(|_| "invalid World map row".into())
                })
                .collect()
        };
        let spawns = self
            .start
            .players
            .iter()
            .enumerate()
            .map(|(slot, p)| {
                game.allied(slot, owner)
                    .then_some([(p.spawn.x / CELL).floor(), (p.spawn.z / CELL).floor()])
            })
            .collect();
        Ok(KnownWorldMap {
            width: game.w,
            height: game.h,
            size: geography.size,
            total: geography.regions.len(),
            rows: strings(rows)?,
            ground: strings(ground)?,
            objects: strings(objects)?,
            mines: strings(mines)?,
            heights: heights.chunks(game.w).map(|row| row.concat()).collect(),
            materials,
            ground_materials,
            spawns,
        })
    }
}
impl WorldRecipientMetadata {
    /// Reference field projection for internal comparison. It is not a complete envelope.
    pub fn to_json(&self) -> Value {
        let mut out = self.lists.to_json();
        let p = &self.world;
        out["home"] = json!([number(p.home.x), number(p.home.z)]);
        let regions:Vec<_>=p.regions.iter().map(|r| {
            let mut row=json!({"id":r.id,"team":r.team,"x":number(r.at.x),"z":number(r.at.z),"progress":number(r.progress),"capper":r.capper.map_or(-1,|c|c as i64),"locked":r.locked,"contested":r.contested});
            if let Some(name)=&r.name {row["name"]=json!(name);}
            if let Some(kind)=&r.kind {row["kind"]=json!(kind);}
            if let Some(b)=r.bounds {row["bounds"]=json!(b.map(number));} else {row["runs"]=json!(r.runs);}
            row
        }).collect();
        out["world"] = json!({"home":[number(p.home.x),number(p.home.z)],"size":p.size.name(),"total":p.total,"goal":p.goal,"owned":p.owned,"cap":number(p.cap),"regions":regions,
            "recovery":{"hq":number(p.recovery.hq),"engineer":number(p.recovery.engineer),"available":p.recovery.available}});
        if let Some(stations) = &p.stations {
            out["world"]["stations"] = json!(
                stations
                    .iter()
                    .map(|s| json!({"region":s.region,"x":number(s.at.x),"z":number(s.at.z)}))
                    .collect::<Vec<_>>()
            );
        }
        if let Some(g) = &self.geography {
            let mats = |rs: &[KnownMaterial]| {
                rs.iter()
                    .map(|m| json!({"c":m.cell,"material":m.material}))
                    .collect::<Vec<_>>()
            };
            out["map"] = json!({"name":format!("World Conquest ({})",if g.size==WorldSize::Massive {"Massive"} else {"Huge"}),"w":g.width,"h":g.height,"worldVersion":2,
                "layers":{"ground":g.ground,"objects":g.objects,"mines":g.mines,"materials":mats(&g.materials),"groundMaterials":mats(&g.ground_materials)},"rows":g.rows,"heights":g.heights,
                "spawns":g.spawns.iter().map(|s|s.map(|a|json!({"x":number(a[0]),"y":number(a[1])}))).collect::<Vec<_>>(),"points":[],"world":{"size":g.size.name(),"total":g.total}});
        }
        out
    }
}

#[cfg(test)]
mod tests;
