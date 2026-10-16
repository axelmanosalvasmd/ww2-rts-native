//! Immutable World geography validation and the native generator's typed map handoff.
//! These records are authoring data, not live ownership, capture progress or a Match.
use crate::{
    game::MOVE,
    map::{AuthoritativeMap, Building, Layers, MapError, Spawn, flags},
};
mod validation;
pub use validation::validate_geography;

use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldSize {
    Huge,
    Massive,
}
impl WorldSize {
    pub fn name(self) -> &'static str {
        match self {
            Self::Huge => "huge",
            Self::Massive => "massive",
        }
    }
    pub fn region_count(self) -> usize {
        match self {
            Self::Huge => 64,
            Self::Massive => 128,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldRegion {
    pub id: i32,
    pub x: usize,
    pub y: usize,
    pub bounds: [usize; 4],
    pub home: Option<usize>,
    pub kind: Option<String>,
    pub name: Option<String>,
    pub biome: Option<String>,
    /// Original generator hint. Runtime ownership is resolved from actual match players later.
    pub team: Option<i32>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct WorldDefinition {
    pub size: WorldSize,
    pub region_cells: usize,
    pub regions: Vec<WorldRegion>,
    /// Values index the ordered region vector, as in the source. Region IDs are not reindexed.
    pub region_map: Option<Vec<i16>>,
    pub biomes: Option<Vec<u8>>,
    pub seed: Option<u32>,
    pub generator_version: Option<u32>,
    /// Uninterpreted source fields, including waterways and rails. No algorithm consumes these here.
    pub metadata: BTreeMap<String, Value>,
}
#[derive(Clone, Debug)]
pub struct WorldMapInput {
    pub name: String,
    pub width: usize,
    pub height: usize,
    pub chars: Vec<u8>,
    pub heights: Option<Vec<i8>>,
    /// Normalized authoritative layers. None explicitly selects the source's rows-only migration.
    pub layers: Option<Layers>,
    pub spawns: Vec<Spawn>,
    pub buildings: Vec<Building>,
    pub geography: WorldDefinition,
}
fn invalid(field: &str, reason: &str) -> MapError {
    MapError::Invalid {
        field: field.into(),
        reason: reason.into(),
    }
}
fn whole(v: &Value, low: i64, high: i64, field: &str) -> Result<i64, MapError> {
    v.as_f64()
        .filter(|n| n.is_finite() && n.fract() == 0.0 && *n >= low as f64 && *n <= high as f64)
        .map(|n| n as i64)
        .ok_or_else(|| invalid(field, "expected a bounded integer"))
}
fn optional_text(v: &Value, key: &str) -> Result<Option<String>, MapError> {
    v.get(key)
        .map(|x| {
            x.as_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid(key, "expected text"))
        })
        .transpose()
}
const KEYS: &[&str] = &[
    "size",
    "total",
    "regionCells",
    "regions",
    "regionMap",
    "biomes",
    "seed",
    "generatorVersion",
];
pub(crate) fn parse(
    v: &Value,
    w: usize,
    h: usize,
    chars: &[u8],
    heights: Option<&[i8]>,
    spawns: usize,
) -> Result<WorldDefinition, MapError> {
    validate_geography(v, w, h, chars, heights, spawns)
        .map_err(|reason| invalid("world.validation", reason))?;
    let object = v
        .as_object()
        .ok_or_else(|| invalid("world", "expected an object"))?;
    let size = match v["size"].as_str() {
        Some("huge") => WorldSize::Huge,
        Some("massive") => WorldSize::Massive,
        _ => return Err(invalid("world.size", "expected huge or massive")),
    };
    let count = size.region_count();
    if whole(&v["total"], 0, 128, "world.total")? as usize != count
        || whole(&v["regionCells"], 0, 1024, "world.regionCells")? != 64
    {
        return Err(invalid("world", "invalid region count or cell span"));
    }
    let raw = v["regions"]
        .as_array()
        .filter(|a| a.len() == count)
        .ok_or_else(|| invalid("world.regions", "count must match size"))?;
    let mut regions = Vec::with_capacity(count);
    let mut ids = BTreeSet::new();
    let mut homes = BTreeSet::new();
    for r in raw {
        let id = whole(
            &r["id"],
            i32::MIN.into(),
            i32::MAX.into(),
            "world.regions.id",
        )? as i32;
        let x = whole(&r["x"], 0, w as i64 - 1, "world.regions.x")? as usize;
        let y = whole(&r["y"], 0, h as i64 - 1, "world.regions.y")? as usize;
        if !ids.insert(id) || flags(chars[y * w + x]) & MOVE != 0 {
            return Err(invalid(
                "world.regions",
                "sites must be open and IDs unique",
            ));
        }
        let b = r["bounds"]
            .as_array()
            .filter(|b| b.len() == 4)
            .ok_or_else(|| invalid("world.regions.bounds", "expected four bounds"))?;
        let bounds = [
            whole(&b[0], 0, w as i64, "world.regions.bounds")? as usize,
            whole(&b[1], 0, h as i64, "world.regions.bounds")? as usize,
            whole(&b[2], 0, w as i64, "world.regions.bounds")? as usize,
            whole(&b[3], 0, h as i64, "world.regions.bounds")? as usize,
        ];
        if bounds[0] > bounds[2] || bounds[1] > bounds[3] {
            return Err(invalid("world.regions.bounds", "bounds must be ordered"));
        }
        let home = r
            .get("home")
            .map(|v| whole(v, 0, spawns as i64 - 1, "world.regions.home").map(|n| n as usize))
            .transpose()?;
        if home.is_some_and(|n| !homes.insert(n)) {
            return Err(invalid("world.regions.home", "duplicate player home"));
        }
        regions.push(WorldRegion {
            id,
            x,
            y,
            bounds,
            home,
            kind: optional_text(r, "kind")?,
            name: optional_text(r, "name")?,
            biome: optional_text(r, "biome")?,
            team: r
                .get("team")
                .map(|v| {
                    whole(v, i32::MIN.into(), i32::MAX.into(), "world.regions.team")
                        .map(|n| n as i32)
                })
                .transpose()?,
        });
    }
    if homes.len() != spawns {
        return Err(invalid("world.regions.home", "each player needs one home"));
    }
    let region_map = match v.get("regionMap") {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            v.as_array()
                .filter(|a| a.len() == w * h)
                .ok_or_else(|| invalid("world.regionMap", "one membership per cell required"))?
                .iter()
                .map(|v| whole(v, -1, count as i64 - 1, "world.regionMap").map(|n| n as i16))
                .collect::<Result<_, _>>()?,
        ),
    };
    let biomes = match v.get("biomes") {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            v.as_array()
                .filter(|a| a.len() == w * h)
                .ok_or_else(|| invalid("world.biomes", "one byte per cell required"))?
                .iter()
                .map(|v| whole(v, 0, 255, "world.biomes").map(|n| n as u8))
                .collect::<Result<_, _>>()?,
        ),
    };
    Ok(WorldDefinition {
        size,
        region_cells: 64,
        regions,
        region_map,
        biomes,
        seed: v
            .get("seed")
            .map(|v| whole(v, 0, u32::MAX.into(), "world.seed").map(|n| n as u32))
            .transpose()?,
        generator_version: v
            .get("generatorVersion")
            .map(|v| whole(v, 0, u32::MAX.into(), "world.generatorVersion").map(|n| n as u32))
            .transpose()?,
        metadata: object
            .iter()
            .filter(|(k, _)| !KEYS.contains(&k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    })
}
impl WorldDefinition {
    fn value(&self) -> Result<Value, MapError> {
        if self.metadata.keys().any(|k| KEYS.contains(&k.as_str())) {
            return Err(invalid("world.metadata", "reserved field"));
        }
        let mut v = json!(self.metadata);
        v["size"] = json!(self.size.name());
        v["total"] = json!(self.regions.len());
        v["regionCells"] = json!(self.region_cells);
        v["regions"] = json!(
            self.regions
                .iter()
                .map(|r| {
                    let mut v = json!({"id":r.id,"x":r.x,"y":r.y,"bounds":r.bounds});
                    for (k, x) in [
                        ("home", r.home.map(|x| json!(x))),
                        ("kind", r.kind.as_ref().map(|x| json!(x))),
                        ("name", r.name.as_ref().map(|x| json!(x))),
                        ("biome", r.biome.as_ref().map(|x| json!(x))),
                        ("team", r.team.map(|x| json!(x))),
                    ] {
                        if let Some(x) = x {
                            v[k] = x;
                        }
                    }
                    v
                })
                .collect::<Vec<_>>()
        );
        for (k, x) in [
            ("regionMap", self.region_map.as_ref().map(|x| json!(x))),
            ("biomes", self.biomes.as_ref().map(|x| json!(x))),
            ("seed", self.seed.map(|x| json!(x))),
            ("generatorVersion", self.generator_version.map(|x| json!(x))),
        ] {
            if let Some(x) = x {
                v[k] = x;
            }
        }
        Ok(v)
    }
}
impl AuthoritativeMap {
    /// Validate the generator's explicit output before terrain or World construction.
    /// No client projection, terrain repair or home assignment is substituted on failure.
    pub fn from_world(input: WorldMapInput) -> Result<Self, MapError> {
        let n = input
            .width
            .checked_mul(input.height)
            .ok_or_else(|| invalid("dimensions", "overflow"))?;
        if !(20..=1024).contains(&input.width)
            || !(20..=1024).contains(&input.height)
            || input.chars.len() != n
            || input
                .heights
                .as_ref()
                .is_some_and(|v| v.len() != n || v.iter().any(|x| !(-2..=4).contains(x)))
        {
            return Err(invalid(
                "terrain",
                "dimensions, characters or heights do not match",
            ));
        }
        let rows = |v: &[u8]| -> Result<Vec<String>, MapError> {
            if v.len() != n {
                return Err(invalid("layers", "incorrect cell count"));
            }
            v.chunks(input.width)
                .map(|r| {
                    String::from_utf8(r.to_vec())
                        .map_err(|_| invalid("rows", "expected ASCII terrain"))
                })
                .collect()
        };
        let mut v = json!({"name":input.name,"w":input.width,"h":input.height,"rows":rows(&input.chars)?,"spawns":input.spawns.iter().map(|s|json!({"x":s.x,"y":s.y,"assault":s.assault})).collect::<Vec<_>>(),"points":[],"buildings":input.buildings.iter().map(|b|json!({"x":b.x,"y":b.y,"kind":b.kind})).collect::<Vec<_>>(),"world":input.geography.value()?});
        if let Some(h) = &input.heights {
            v["heights"] = json!(rows(
                &h.iter()
                    .map(|v| match v {
                        -2 => b'b',
                        -1 => b'a',
                        _ => b'0' + *v as u8,
                    })
                    .collect::<Vec<_>>()
            )?);
        }
        if let Some(l) = &input.layers {
            if l.mines.len() != n || l.mines.iter().any(|m| *m > 1) {
                return Err(invalid(
                    "layers.mines",
                    "expected one boolean byte per cell",
                ));
            }
            let list = |m: &BTreeMap<usize, String>| {
                m.iter()
                    .map(|(c, m)| json!({"c":c,"material":m}))
                    .collect::<Vec<_>>()
            };
            v["worldVersion"] = json!(2);
            v["layers"] = json!({"ground":rows(&l.ground)?,"objects":rows(&l.objects)?,"mines":rows(&l.mines.iter().map(|m|if *m==0{b'.'}else{b'N'}).collect::<Vec<_>>())?,"groundMaterials":list(&l.base_materials),"objectMaterials":list(&l.object_materials),"mineMaterials":list(&l.mine_materials)});
        }
        let map = Self::from_value(v)?;
        if input.layers.as_ref().is_some_and(|l| l != map.layers()) {
            return Err(invalid(
                "layers.materials",
                "normalized material maps must match authoritative layers",
            ));
        }
        Ok(map)
    }
}
