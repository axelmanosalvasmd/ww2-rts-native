//! Detached World footprint queries. These do not admit construction commands.
use crate::{
    game::{CELL, Point},
    match_world_rails::WorldRails,
    world_territories::inside_known_region,
};

/// Discovered geometry preserves absent versus explicitly empty explored runs.
#[derive(Clone, Copy)]
pub struct PlacementRegion<'a> {
    pub team: i32,
    pub runs: Option<&'a [[usize; 3]]>,
    pub bounds: Option<&'a [f64; 4]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlacementExclusion {
    NotVisible,
    Territory,
    Rail,
}
impl PlacementExclusion {
    pub fn reason(self) -> &'static str {
        match self {
            Self::NotVisible => "notVisible",
            Self::Territory => "territory",
            Self::Rail => "rail",
        }
    }
}

/// Check a finite request and its already bounded, nonempty footprint.
/// Sight masks ownership and rail reasons. An explicit run list takes precedence
/// over legacy bounds, including when no explored runs have been discovered.
/// The caller still checks terrain, occupancy, nodes and building eligibility.
pub fn footprint_exclusion(
    at: Point,
    width: usize,
    cells: &[usize],
    team: i32,
    regions: &[PlacementRegion<'_>],
    mut sees: impl FnMut(Point) -> bool,
    rails: &WorldRails,
) -> Option<PlacementExclusion> {
    assert!(
        width > 0 && !cells.is_empty(),
        "bounded World footprint required"
    );
    let center = |cell| Point {
        x: ((cell % width) as f64 + 0.5) * CELL,
        z: ((cell / width) as f64 + 0.5) * CELL,
    };
    if !sees(at) || cells.iter().any(|&cell| !sees(center(cell))) {
        return Some(PlacementExclusion::NotVisible);
    }
    if cells.iter().any(|&cell| {
        regions
            .iter()
            .find(|r| inside_known_region(r.runs, r.bounds, center(cell), CELL))
            .is_none_or(|r| r.team != team)
    }) {
        return Some(PlacementExclusion::Territory);
    }
    rails
        .excludes_footprint(cells)
        .then_some(PlacementExclusion::Rail)
}
