//! Checks `ww2_core::formation` against the pinned `shared/formation.js` bit for bit, using
//! `fixtures/formation.txt` from `harness/formation-fixture.mjs`. Each seeded range is rebuilt here
//! with the harness's generator, written out again below, and compared case by case through a hash
//! of its spots; the written-out cases are compared spot by spot.

use std::f64::consts::PI;

use ww2_core::formation::{self, Item, Shape, Spot};
use ww2_core::math;

const FIXTURE: &str = include_str!("fixtures/formation.txt");

/// mulberry32 in wrapping u32 arithmetic (the harness's `seeded`).
struct Mulberry32(u32);

impl Mulberry32 {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6D2B_79F5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }

    /// `below(rand, n)`: a whole number in [0, n).
    fn below(&mut self, n: f64) -> f64 {
        (self.next() * n).floor()
    }

    fn index(&mut self, n: usize) -> usize {
        self.below(n as f64) as usize
    }
}

const SPREADS: [f64; 7] = [1.0, 1.25, 1.5, 1.75, 2.0, 2.25, 2.5];

fn make_face(rand: &mut Mulberry32) -> f64 {
    let r = rand.next();
    if r < 0.25 {
        return (rand.below(17.0) - 8.0) * (PI / 2.0);
    }
    if r < 0.3 {
        return if rand.next() < 0.5 { PI } else { -PI };
    }
    (rand.next() * 4.0 - 2.0) * PI
}

fn make_reach(rand: &mut Mulberry32) -> f64 {
    let r = rand.next();
    if r < 0.25 {
        0.0
    } else if r < 0.32 {
        f64::INFINITY
    } else if r < 0.36 {
        f64::NAN
    } else if r < 0.39 {
        f64::NEG_INFINITY
    } else if r < 0.44 {
        -rand.next() * 40.0
    } else {
        rand.next() * 160.0
    }
}

fn make_spread(rand: &mut Mulberry32) -> f64 {
    let r = rand.next();
    if r < 0.75 {
        SPREADS[rand.index(SPREADS.len())]
    } else if r < 0.95 {
        0.5 + rand.next() * 2.5
    } else if r < 0.97 {
        0.0
    } else if r < 0.98 {
        f64::NAN
    } else {
        -1.0 - rand.next()
    }
}

fn make_size(rand: &mut Mulberry32) -> f64 {
    let r = rand.next();
    if r < 0.55 {
        3.0
    } else if r < 0.95 {
        5.0 + rand.next() * 6.0
    } else {
        rand.next() * 3.0
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Style {
    Scatter,
    Ties,
    Ordered,
    Split,
}

impl Style {
    fn parse(name: &str) -> Style {
        match name {
            "scatter" => Style::Scatter,
            "ties" => Style::Ties,
            "ordered" => Style::Ordered,
            "split" => Style::Split,
            _ => panic!("unknown placement style {name}"),
        }
    }
}

/// The harness's `PLACE` styles, after their setup draws.
enum Place {
    Scatter {
        cx: f64,
        cz: f64,
        scatter: f64,
    },
    Ties {
        cx: f64,
        cz: f64,
        dx: f64,
        dz: f64,
        rows: f64,
    },
    Ordered {
        cx: f64,
        cz: f64,
        ex: f64,
        ez: f64,
        cols: f64,
        spacing: f64,
        depth: f64,
        jitter: f64,
    },
    /// The first `cut` units loose, the rest in ranks.
    Split {
        loose: Box<Place>,
        ranks: Box<Place>,
        cut: usize,
    },
}

impl Place {
    fn new(style: Style, rand: &mut Mulberry32, face: f64, cx: f64, cz: f64, n: usize) -> Place {
        match style {
            Style::Scatter => Place::Scatter {
                cx,
                cz,
                scatter: 5.0 + rand.next() * 60.0,
            },
            Style::Ties => Place::Ties {
                cx,
                cz,
                dx: math::cos(face),
                dz: math::sin(face),
                rows: 1.0 + rand.below(4.0),
            },
            Style::Ordered => {
                let turn = [0.0, 0.0, PI, PI / 2.0, -PI / 2.0, 0.3][rand.index(6)];
                let (ex, ez) = (math::cos(face + turn), math::sin(face + turn));
                let cols = 1.0 + rand.below(12.0);
                let spacing = 3.0 + rand.next() * 4.0;
                let depth = 4.0 + rand.next() * 3.0;
                let jitter = [0.0, 0.3][rand.index(2)];
                Place::Ordered {
                    cx,
                    cz,
                    ex,
                    ez,
                    cols,
                    spacing,
                    depth,
                    jitter,
                }
            }
            Style::Split => {
                let loose = Place::new(Style::Scatter, rand, face, cx, cz, n);
                let ranks = Place::new(Style::Ordered, rand, face, cx, cz, n);
                let cut = (n as f64 * (0.2 + rand.next() * 0.4)).floor() as usize;
                Place::Split {
                    loose: Box::new(loose),
                    ranks: Box::new(ranks),
                    cut,
                }
            }
        }
    }

    fn at(&self, rand: &mut Mulberry32, i: usize, items: &[Item]) -> (f64, f64) {
        match *self {
            Place::Scatter { cx, cz, scatter } => {
                let x = cx + (rand.next() * 2.0 - 1.0) * scatter;
                let z = cz + (rand.next() * 2.0 - 1.0) * scatter;
                (x, z)
            }
            Place::Ties {
                cx,
                cz,
                dx,
                dz,
                rows,
            } => {
                if i > 0 && rand.next() < 0.1 {
                    let j = rand.index(i);
                    return (items[j].x, items[j].z);
                }
                let row = rand.below(rows);
                let across = (rand.next() * 2.0 - 1.0) * 30.0;
                let depth = row * 4.0 + (rand.next() * 2.0 - 1.0) * 0.012;
                (cx + dx * depth - dz * across, cz + dz * depth + dx * across)
            }
            Place::Ordered {
                cx,
                cz,
                ex,
                ez,
                cols,
                spacing,
                depth,
                jitter,
            } => {
                let i = i as f64;
                let (r, c) = ((i / cols).floor(), i % cols);
                let across = (c - cols / 2.0) * spacing;
                let fwd = -r * depth
                    + if rand.next() < jitter {
                        (rand.next() * 2.0 - 1.0) * 0.01
                    } else {
                        0.0
                    };
                (cx + ex * fwd - ez * across, cz + ez * fwd + ex * across)
            }
            Place::Split {
                ref loose,
                ref ranks,
                cut,
            } => {
                if i < cut {
                    loose.at(rand, i, items)
                } else {
                    ranks.at(rand, i - cut, items)
                }
            }
        }
    }
}

/// One call of `facingSpots`.
struct Case {
    shape: Shape,
    spread: f64,
    reach: f64,
    face: f64,
    at_x: f64,
    at_z: f64,
    items: Vec<Item>,
}

impl Case {
    fn run(&self) -> Vec<Spot> {
        formation::facing_spots(
            &self.items,
            self.at_x,
            self.at_z,
            self.face,
            self.reach,
            self.shape,
            self.spread,
        )
    }

    /// Whether the inputs are the same, bit for bit.
    fn same_inputs(&self, other: &Case) -> bool {
        let numbers = |c: &Case| [c.spread, c.reach, c.face, c.at_x, c.at_z].map(bits);
        let item_bits = |u: &Item| (u.id, bits(u.x), bits(u.z), bits(u.size), u.back);
        self.shape == other.shape
            && numbers(self) == numbers(other)
            && self
                .items
                .iter()
                .map(item_bits)
                .eq(other.items.iter().map(item_bits))
    }
}

/// The harness's `makeCase`.
fn make_case(rand: &mut Mulberry32, fewest: usize, most: usize, styles: &[Style]) -> Case {
    let shape = Shape::ALL[rand.index(Shape::ALL.len())];
    let n = fewest + rand.index(most - fewest + 1);
    let style = styles[rand.index(styles.len())];
    let face = make_face(rand);
    let reach = make_reach(rand);
    let spread = make_spread(rand);
    let at_x = rand.next() * 1200.0;
    let at_z = rand.next() * 1200.0;
    let kinds = 1.0 + rand.below(3.0);
    let palette = [make_size(rand), make_size(rand), make_size(rand)];
    let back_rate = [0.0, 0.0, 0.1, 0.3][rand.index(4)];
    let cx = rand.next() * 1200.0;
    let cz = rand.next() * 1200.0;
    let place = Place::new(style, rand, face, cx, cz, n);
    let mut items: Vec<Item> = Vec::with_capacity(n);
    for i in 0..n {
        let (x, z) = place.at(rand, i, &items);
        let id = rand.below(4_294_967_296.0) as u32;
        let size = palette[rand.below(kinds) as usize];
        let back = rand.next() < back_rate;
        items.push(Item {
            id,
            x,
            z,
            size,
            back,
        });
    }
    if style == Style::Ordered {
        let swaps = [0, 0, 1, 3][rand.index(4)];
        for _ in 0..swaps {
            let i = rand.index(n);
            let j = rand.index(n);
            items.swap(i, j);
        }
        if rand.next() < 0.25 {
            items.reverse();
        }
    }
    Case {
        shape,
        spread,
        reach,
        face,
        at_x,
        at_z,
        items,
    }
}

/// The bits the fixture records: JavaScript has one NaN.
fn bits(x: f64) -> u64 {
    if x.is_nan() {
        0x7FF8_0000_0000_0000
    } else {
        x.to_bits()
    }
}

fn parse_bits(text: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(text, 16).unwrap_or_else(|_| panic!("bad hex {text}")))
}

/// The harness's `hashSpots`: FNV-1a 64 over each spot's id, x and z bits.
fn hash_spots(spots: &[Spot]) -> u64 {
    let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
    for spot in spots {
        for value in [f64::from(spot.id), spot.x, spot.z] {
            for byte in bits(value).to_le_bytes() {
                hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01B3);
            }
        }
    }
    hash
}

struct Range {
    label: String,
    seed: u32,
    count: usize,
    fewest: usize,
    most: usize,
    styles: Vec<Style>,
    hashes: Vec<u64>,
}

struct Written {
    label: String,
    case: Case,
    spots: Vec<Spot>,
}

#[derive(Default)]
struct Fixture {
    node: String,
    ranges: Vec<Range>,
    written: Vec<Written>,
    /// An angle and what `normalizeFace` returned for it.
    faces: Vec<(f64, Option<f64>)>,
    /// `infantry`, `radius` and what `slotSize` returned.
    slots: Vec<(bool, f64, f64)>,
}

fn parse_fixture() -> Fixture {
    let mut fixture = Fixture::default();
    for line in FIXTURE.lines().filter(|line| !line.starts_with('#')) {
        let fields: Vec<&str> = line.split(' ').collect();
        match fields[0] {
            "node" | "v8" | "arch" => {
                fixture.node = format!("{} {line}", fixture.node).trim().to_string()
            }
            "range" => fixture.ranges.push(Range {
                label: fields[1].to_string(),
                seed: fields[2].parse().expect("seed"),
                count: fields[3].parse().expect("count"),
                fewest: fields[4].parse().expect("fewest"),
                most: fields[5].parse().expect("most"),
                styles: fields[6].split(',').map(Style::parse).collect(),
                hashes: Vec::new(),
            }),
            "hashes" => {
                let range = fixture.ranges.last_mut().expect("hashes before any range");
                for text in &fields[1..] {
                    range
                        .hashes
                        .push(u64::from_str_radix(text, 16).expect("hash"));
                }
            }
            "case" => fixture.written.push(Written {
                label: fields[1].to_string(),
                case: Case {
                    shape: Shape::from_name(fields[2]).expect("shape"),
                    spread: parse_bits(fields[3]),
                    reach: parse_bits(fields[4]),
                    face: parse_bits(fields[5]),
                    at_x: parse_bits(fields[6]),
                    at_z: parse_bits(fields[7]),
                    items: Vec::new(),
                },
                spots: Vec::new(),
            }),
            "item" => {
                let written = fixture.written.last_mut().expect("an item before any case");
                written.case.items.push(Item {
                    id: fields[1].parse().expect("id"),
                    x: parse_bits(fields[2]),
                    z: parse_bits(fields[3]),
                    size: parse_bits(fields[4]),
                    back: fields[5] == "1",
                });
            }
            "spot" => {
                let written = fixture.written.last_mut().expect("a spot before any case");
                written.spots.push(Spot {
                    id: fields[1].parse().expect("id"),
                    x: parse_bits(fields[2]),
                    z: parse_bits(fields[3]),
                });
            }
            "face" => fixture.faces.push((
                parse_bits(fields[1]),
                (fields[2] != "null").then(|| parse_bits(fields[2])),
            )),
            "slot" => fixture.slots.push((
                fields[1] == "1",
                parse_bits(fields[2]),
                parse_bits(fields[3]),
            )),
            other => panic!("unknown fixture line kind {other}"),
        }
    }
    fixture
}

fn show(spots: &[Spot]) -> String {
    let shown: Vec<String> = spots
        .iter()
        .map(|s| format!("[{}, {:016x}, {:016x}]", s.id, bits(s.x), bits(s.z)))
        .collect();
    shown.join(" ")
}

#[test]
fn seeded_cases_match_node() {
    let fixture = parse_fixture();
    assert!(!fixture.ranges.is_empty(), "the fixture has no ranges");
    let mut failures = Vec::new();
    for range in &fixture.ranges {
        assert_eq!(
            range.hashes.len(),
            range.count,
            "{}: hash count",
            range.label
        );
        let mut rand = Mulberry32(range.seed);
        for (i, &hash) in range.hashes.iter().enumerate() {
            let case = make_case(&mut rand, range.fewest, range.most, &range.styles);
            let label = format!("{}#{i}", range.label);
            if let Some(written) = fixture.written.iter().find(|w| w.label == label) {
                assert!(
                    case.same_inputs(&written.case),
                    "{label}: the Rust generator built other inputs than the harness"
                );
            }
            let spots = case.run();
            if hash_spots(&spots) != hash && failures.len() < 5 {
                failures.push(format!(
                    "{label} ({} {} units, spread {:?}, reach {:?}, face {:?}): Rust gave {}",
                    case.shape.name(),
                    case.items.len(),
                    case.spread,
                    case.reach,
                    case.face,
                    show(&spots[..spots.len().min(6)]),
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "against {}:\n{}",
        fixture.node,
        failures.join("\n")
    );
}

#[test]
fn written_cases_match_node() {
    let fixture = parse_fixture();
    assert!(
        !fixture.written.is_empty(),
        "the fixture has no written cases"
    );
    let failures: Vec<String> = fixture
        .written
        .iter()
        .filter_map(|w| {
            let spots = w.case.run();
            let same = spots.len() == w.spots.len()
                && spots
                    .iter()
                    .zip(&w.spots)
                    .all(|(a, b)| a.id == b.id && bits(a.x) == bits(b.x) && bits(a.z) == bits(b.z));
            (!same).then(|| {
                format!(
                    "{}: Rust {}\n    Node {}",
                    w.label,
                    show(&spots),
                    show(&w.spots)
                )
            })
        })
        .collect();
    assert!(
        failures.is_empty(),
        "against {}:\n{}",
        fixture.node,
        failures.join("\n")
    );
}

#[test]
fn normalize_face_matches_node() {
    let fixture = parse_fixture();
    assert!(!fixture.faces.is_empty(), "the fixture has no faces");
    for &(a, node) in &fixture.faces {
        let rust = formation::normalize_face(a);
        assert_eq!(
            rust.map(bits),
            node.map(bits),
            "normalizeFace({a:?}): Rust {rust:?}, Node {node:?}"
        );
    }
}

#[test]
fn slot_size_matches_node() {
    let fixture = parse_fixture();
    assert!(!fixture.slots.is_empty(), "the fixture has no slot sizes");
    for &(infantry, radius, node) in &fixture.slots {
        let rust = formation::slot_size(infantry, radius);
        assert_eq!(
            bits(rust),
            bits(node),
            "slotSize({{ infantry: {infantry}, radius: {radius:?} }}): Rust {rust:?}, Node {node:?}"
        );
    }
}
