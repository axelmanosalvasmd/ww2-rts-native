//! Checks `ww2_core::math` against Node bit for bit, using `fixtures/math.txt` from
//! `harness/math-fixture.mjs`. Each range rebuilds the fixture's inputs with the same mulberry32 and
//! compares a hash of every output; each edge case is compared directly.

use ww2_core::math;

const FIXTURE: &str = include_str!("fixtures/math.txt");

/// mulberry32 as `reference/ww2-rts/shared/ai-schedule.js` writes it, in wrapping u32 arithmetic.
struct Mulberry32(u32);

impl Mulberry32 {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6D2B_79F5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }

    /// A uniform number in [0, 1] with about 53 random bits, from two draws (the harness's `unit`).
    fn unit(&mut self) -> f64 {
        let high = self.next();
        let low = self.next();
        high + low / 4_294_967_296.0
    }
}

/// How one argument of a range maps a unit number into its bounds (the harness's `KINDS`).
struct Arg {
    kind: String,
    lo: f64,
    hi: f64,
}

impl Arg {
    fn parse(text: &str) -> Arg {
        let mut parts = text.split(':');
        let mut next = || {
            parts
                .next()
                .unwrap_or_else(|| panic!("bad argument {text}"))
        };
        let kind = next().to_string();
        let lo = next()
            .parse()
            .unwrap_or_else(|_| panic!("bad bound in {text}"));
        let hi = next()
            .parse()
            .unwrap_or_else(|_| panic!("bad bound in {text}"));
        Arg { kind, lo, hi }
    }

    fn draw(&self, rng: &mut Mulberry32) -> f64 {
        let x = self.lo + (self.hi - self.lo) * rng.unit();
        match self.kind.as_str() {
            "real" => x,
            "int" => x.floor(),
            "half" => x.floor() + 0.5,
            kind => panic!("unknown argument kind {kind}"),
        }
    }
}

struct Range {
    label: String,
    function: String,
    seed: u32,
    count: usize,
    hash: u64,
    args: Vec<Arg>,
    /// The first inputs and output Node recorded, for diagnosis.
    samples: Vec<Vec<f64>>,
}

/// One edge case: the inputs, then Node's output.
struct Edge {
    function: String,
    values: Vec<f64>,
}

struct Fixture {
    node: String,
    ranges: Vec<Range>,
    edges: Vec<Edge>,
}

fn parse_bits(text: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(text, 16).unwrap_or_else(|_| panic!("bad hex {text}")))
}

fn parse_fixture() -> Fixture {
    let mut fixture = Fixture {
        node: String::new(),
        ranges: Vec::new(),
        edges: Vec::new(),
    };
    for line in FIXTURE.lines().filter(|line| !line.starts_with('#')) {
        let fields: Vec<&str> = line.split(' ').collect();
        match fields[0] {
            "node" | "v8" | "arch" => {
                fixture.node = format!("{} {line}", fixture.node).trim().to_string()
            }
            "range" => fixture.ranges.push(Range {
                label: fields[1].to_string(),
                function: fields[2].to_string(),
                seed: fields[3].parse().expect("seed"),
                count: fields[4].parse().expect("count"),
                hash: u64::from_str_radix(fields[5], 16).expect("hash"),
                args: fields[6..].iter().map(|text| Arg::parse(text)).collect(),
                samples: Vec::new(),
            }),
            "sample" => {
                let range = fixture
                    .ranges
                    .last_mut()
                    .expect("a sample before any range");
                assert_eq!(range.label, fields[1], "sample out of place");
                range
                    .samples
                    .push(fields[2..].iter().map(|text| parse_bits(text)).collect());
            }
            "edge" => fixture.edges.push(Edge {
                function: fields[1].to_string(),
                values: fields[2..].iter().map(|text| parse_bits(text)).collect(),
            }),
            other => panic!("unknown fixture line kind {other}"),
        }
    }
    fixture
}

fn call(function: &str, args: &[f64]) -> f64 {
    match (function, args) {
        ("sin", &[x]) => math::sin(x),
        ("cos", &[x]) => math::cos(x),
        ("tan", &[x]) => math::tan(x),
        ("atan", &[x]) => math::atan(x),
        ("atan2", &[y, x]) => math::atan2(y, x),
        ("exp", &[x]) => math::exp(x),
        ("log", &[x]) => math::log(x),
        ("pow", &[x, y]) => math::pow(x, y),
        ("hypot", &[a, b]) => math::hypot(a, b),
        ("hypot3", &[a, b, c]) => math::hypot3(a, b, c),
        ("round", &[x]) => math::round(x),
        ("max", &[a, b]) => math::max(a, b),
        ("min", &[a, b]) => math::min(a, b),
        ("sign", &[x]) => math::sign(x),
        _ => panic!("no {function} taking {} arguments", args.len()),
    }
}

/// The bits the fixture records: JavaScript has one NaN, so every NaN hashes alike.
fn bits(x: f64) -> u64 {
    if x.is_nan() {
        0x7FF8_0000_0000_0000
    } else {
        x.to_bits()
    }
}

fn fnv1a(hash: u64, bits: u64) -> u64 {
    bits.to_le_bytes().iter().fold(hash, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01B3)
    })
}

fn show(values: &[f64]) -> String {
    let shown: Vec<String> = values
        .iter()
        .map(|x| format!("{x:?} ({:016x})", bits(*x)))
        .collect();
    shown.join(", ")
}

/// Rebuilds a range's inputs and compares the hash of the outputs; on a mismatch, names the first
/// recorded sample that differs.
fn check_range(range: &Range) -> Result<(), String> {
    let mut rng = Mulberry32(range.seed);
    let mut hash = 0xCBF2_9CE4_8422_2325;
    let mut first_diff = None;
    for i in 0..range.count {
        let inputs: Vec<f64> = range.args.iter().map(|arg| arg.draw(&mut rng)).collect();
        let output = call(&range.function, &inputs);
        hash = fnv1a(hash, bits(output));
        if let Some(sample) = range.samples.get(i) {
            let (node_inputs, node_output) = sample.split_at(sample.len() - 1);
            let same_inputs = inputs
                .iter()
                .zip(node_inputs)
                .all(|(a, b)| bits(*a) == bits(*b));
            if !same_inputs {
                return Err(format!(
                    "{}: input {i} is [{}], Node drew [{}]: the generators disagree",
                    range.label,
                    show(&inputs),
                    show(node_inputs)
                ));
            }
            if first_diff.is_none() && bits(output) != bits(node_output[0]) {
                first_diff = Some(format!(
                    "first difference at input {i}: {}([{}]) = {}, Node {}",
                    range.function,
                    show(&inputs),
                    show(&[output]),
                    show(node_output)
                ));
            }
        }
    }
    if hash == range.hash {
        return Ok(());
    }
    let detail =
        first_diff.unwrap_or_else(|| format!("the first {} samples match", range.samples.len()));
    Err(format!(
        "{}: output hash {hash:016x}, Node {:016x}; {detail}",
        range.label, range.hash
    ))
}

#[test]
fn ranges_match_node() {
    let fixture = parse_fixture();
    assert!(!fixture.ranges.is_empty(), "the fixture has no ranges");
    let failures: Vec<String> = fixture
        .ranges
        .iter()
        .filter_map(|range| check_range(range).err())
        .collect();
    assert!(
        failures.is_empty(),
        "against {}:\n{}",
        fixture.node,
        failures.join("\n")
    );
}

#[test]
fn edge_cases_match_node() {
    let fixture = parse_fixture();
    assert!(!fixture.edges.is_empty(), "the fixture has no edge cases");
    let failures: Vec<String> = fixture
        .edges
        .iter()
        .filter_map(|edge| {
            let (inputs, node_output) = edge.values.split_at(edge.values.len() - 1);
            let output = call(&edge.function, inputs);
            (bits(output) != bits(node_output[0])).then(|| {
                format!(
                    "{}([{}]) = {}, Node {}",
                    edge.function,
                    show(inputs),
                    show(&[output]),
                    show(node_output)
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
fn map64_floor_preserves_exact_world_thresholds_and_priority_boundaries() {
    // Math.floor at the pinned prototype uses the same IEEE floor operation
    // called directly by native world generation. Check both sides of the
    // integer thresholds used for landform passes and road priorities.
    for (input, expected) in [
        (0.0f64, 0.0f64),
        (-0.0, -0.0),
        (f64::from_bits(1), 0.0),
        (-f64::from_bits(1), -1.0),
        (f64::INFINITY, f64::INFINITY),
        (f64::NEG_INFINITY, f64::NEG_INFINITY),
        (4_503_599_627_370_495.5, 4_503_599_627_370_495.0),
        (4_503_599_627_370_496.0, 4_503_599_627_370_496.0),
    ] {
        assert_eq!(
            input.floor().to_bits(),
            expected.to_bits(),
            "floor({input:?})"
        );
    }
    for integer in [
        -4.0f64, -3.0, -2.0, -1.0, 1.0, 2.0, 3.0, 4.0, 65535.0, 65536.0,
    ] {
        for (input, expected) in [
            (integer.next_down(), integer - 1.0),
            (integer, integer),
            (integer.next_up(), integer),
        ] {
            assert_eq!(
                input.floor().to_bits(),
                expected.to_bits(),
                "floor({input:?})"
            );
        }
    }
    assert!(f64::NAN.floor().is_nan());
}
