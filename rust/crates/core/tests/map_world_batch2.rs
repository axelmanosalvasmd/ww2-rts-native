//! MAP-94 fingerprints of the complete generated terrain, before match setup.
use ww2_core::world_generation::{WorldSize, generate_world_from_terrain_seed};

// The core declares no digest dependency. Keep this SHA-256 helper local to the test.
fn sha256_prefix(bytes: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut state = [
        0x6a09e667_u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut padded = bytes.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&(bytes.len() as u64 * 8).to_be_bytes());
    for block in padded.as_chunks::<64>().0 {
        let mut words = [0_u32; 64];
        for (word, bytes) in words[..16].iter_mut().zip(block.as_chunks::<4>().0) {
            *word = u32::from_be_bytes(*bytes);
        }
        for i in 16..64 {
            let a = words[i - 15];
            let b = words[i - 2];
            let s0 = a.rotate_right(7) ^ a.rotate_right(18) ^ (a >> 3);
            let s1 = b.rotate_right(17) ^ b.rotate_right(19) ^ (b >> 10);
            words[i] = words[i - 16]
                .wrapping_add(s0)
                .wrapping_add(words[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for (&word, &constant) in words.iter().zip(&K) {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choice = (e & f) ^ (!e & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(choice)
                .wrapping_add(constant)
                .wrapping_add(word);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (word, next) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *word = word.wrapping_add(next);
        }
    }
    format!("{:08x}{:08x}", state[0], state[1])
}

#[test]
fn map94_all_six_literal_world_fingerprints_and_river_counts() {
    // Standard SHA-256 vectors cover empty input, one block and two-block padding.
    for (input, expected) in [
        ("", "e3b0c44298fc1c14"),
        ("abc", "ba7816bf8f01cfea"),
        (
            "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
            "248d6a61d20638b8",
        ),
    ] {
        assert_eq!(sha256_prefix(input.as_bytes()), expected);
    }
    // Literal baselines: docs/parity/maps.md MAP-94 at reference e4e579a.
    // Source shared/world-conquest.js:22-28,183-204 uses scenery=true by default.
    // River kinds come from shared/world-landforms.js:71-103,123.
    for (seed, size, players, fingerprint, mains, tributaries) in [
        (0, WorldSize::Huge, 2, "49b0b8714ba2f928", 1, 0),
        (7, WorldSize::Huge, 2, "e34879af12ab948e", 1, 3),
        (1, WorldSize::Huge, 2, "f2ee8a0143430dc9", 2, 2),
        (2, WorldSize::Huge, 2, "6f42a84c7e7492dc", 3, 1),
        (20_261_003, WorldSize::Huge, 4, "5574bb8dfb95db4a", 3, 2),
        (42, WorldSize::Massive, 6, "4278447e0b1a27e8", 2, 3),
    ] {
        let teams: Vec<_> = (0..players).map(|slot| slot as i32).collect();
        let world = generate_world_from_terrain_seed(seed, size, &teams, true)
            .unwrap_or_else(|error| panic!("{size:?} seed {seed}, {players} players: {error}"));
        let geography = &world.geography;
        let grid = &geography.grid;
        assert_eq!(
            (grid.width, grid.height),
            match size {
                WorldSize::Huge => (512, 512),
                WorldSize::Massive => (1024, 512),
            }
        );
        assert_eq!(grid.chars.len(), grid.width * grid.height);
        assert_eq!(geography.spawns.len(), players);
        assert!(world.scenery.is_some());
        let rows = grid
            .chars
            .chunks_exact(grid.width)
            .map(|row| std::str::from_utf8(row).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            sha256_prefix(rows.as_bytes()),
            fingerprint,
            "{size:?} seed {seed}, {players} players: rows.join('\\n'), no trailing newline"
        );
        assert_eq!(
            (
                geography
                    .waterways
                    .rivers
                    .iter()
                    .filter(|river| river.parent.is_none())
                    .count(),
                geography
                    .waterways
                    .rivers
                    .iter()
                    .filter(|river| river.parent.is_some())
                    .count(),
            ),
            (mains, tributaries),
            "{size:?} seed {seed}, {players} players: main rivers and tributaries"
        );
    }
}
