use ww2_core::orders::{
    ORDER_QUEUE_VERSION, OrderKey, OrderQueue, OrderSnapshot, QueueError, StampedOrder,
};
use ww2_core::random::{MatchRandom, RandomError, SplitMix64, StreamId};

#[test]
fn splitmix_matches_the_authors_c_reference() {
    // Recorded by compiling https://prng.di.unimi.it/splitmix64.c without edits
    // to next(), then setting x to each seed and calling next() five times.
    let cases = [
        (
            0,
            [
                0xe220_a839_7b1d_cdaf,
                0x6e78_9e6a_a1b9_65f4,
                0x06c4_5d18_8009_454f,
                0xf88b_b8a8_724c_81ec,
                0x1b39_896a_51a8_749b,
            ],
        ),
        (
            1,
            [
                0x910a_2dec_8902_5cc1,
                0xbeeb_8da1_658e_ec67,
                0xf893_a2ee_fb32_555e,
                0x71c1_8690_ee42_c90b,
                0x71bb_54d8_d101_b5b9,
            ],
        ),
        (
            u64::MAX,
            [
                0xe4d9_7177_1b65_2c20,
                0xe99f_f867_dbf6_82c9,
                0x382f_f84c_b272_81e9,
                0x6d1d_b36c_cba9_82d2,
                0xb4a0_472e_5780_69ae,
            ],
        ),
        (
            0x3c1055,
            [
                0xdf49_5522_958b_5987,
                0xbacb_77ce_a1e0_fc8c,
                0x0f7e_9210_9e8f_5ac8,
                0x9de4_2bd8_fc59_1a39,
                0xa945_6c5a_79eb_7c02,
            ],
        ),
    ];
    for (seed, expected) in cases {
        let mut rng = SplitMix64::new(seed);
        for value in expected {
            assert_eq!(rng.next_u64(), value);
        }
    }
}

#[test]
fn purpose_streams_resume_and_do_not_depend_on_other_draws() {
    let purposes = [
        StreamId::Terrain,
        StreamId::Weather,
        StreamId::Combat,
        StreamId::OffMap,
        StreamId::Ai(0),
        StreamId::Ai(5),
    ];
    let mut control = MatchRandom::new(0x3c1055);
    let mut perturbed = control.clone();
    for _ in 0..100 {
        perturbed.next_u64(StreamId::Ai(4));
    }
    for purpose in purposes {
        assert_eq!(control.next_u64(purpose), perturbed.next_u64(purpose));
    }
    let mut resumed = MatchRandom::restore(perturbed.snapshot()).unwrap();
    for purpose in purposes.into_iter().rev() {
        for _ in 0..40 {
            let a = resumed.next_fraction(purpose);
            assert_eq!(a.to_bits(), perturbed.next_fraction(purpose).to_bits());
            assert!((0.0..1.0).contains(&a));
            assert_eq!(resumed.below(purpose, 11), perturbed.below(purpose, 11));
        }
    }
    let before = resumed.snapshot();
    assert_eq!(
        resumed.below(StreamId::Combat, 0),
        Err(RandomError::EmptyRange)
    );
    assert_eq!(before, resumed.snapshot());
}

#[test]
fn random_restore_rejects_unknown_versions_and_repeated_streams() {
    let mut rng = MatchRandom::new(1);
    rng.next_u64(StreamId::Combat);
    let mut state = rng.snapshot();
    state.version += 1;
    assert_eq!(
        MatchRandom::restore(state),
        Err(RandomError::UnsupportedVersion)
    );
    let mut state = rng.snapshot();
    state.streams.push(state.streams[0]);
    assert_eq!(
        MatchRandom::restore(state),
        Err(RandomError::UnorderedStreams)
    );
}

fn order(tick: u64, issuer: u32, sequence: u64) -> StampedOrder<u64> {
    StampedOrder {
        key: OrderKey {
            tick,
            issuer,
            sequence,
        },
        value: tick * 100 + u64::from(issuer) * 10 + sequence,
    }
}

#[test]
fn order_batches_ignore_arrival_order_and_resume_at_the_same_tick() {
    let items = [
        order(0, 2, 1),
        order(2, 1, 0),
        order(0, 1, 2),
        order(0, 1, 1),
        order(2, 0, 1),
    ];
    let mut forward = OrderQueue::new(8);
    let mut reverse = OrderQueue::new(8);
    for item in &items {
        forward.enqueue(item.key, item.value).unwrap();
    }
    for item in items.iter().rev() {
        reverse.enqueue(item.key, item.value).unwrap();
    }
    let first = forward.advance().unwrap();
    assert_eq!(first, reverse.advance().unwrap());
    assert_eq!(
        first.orders,
        [items[3].clone(), items[2].clone(), items[0].clone()]
    );
    let mut resumed = OrderQueue::restore(forward.snapshot()).unwrap();
    for tick in 1..=3 {
        let batch = forward.advance().unwrap();
        assert_eq!(batch.tick, tick);
        assert_eq!(batch, reverse.advance().unwrap());
        assert_eq!(batch, resumed.advance().unwrap());
        if tick == 1 || tick == 3 {
            assert!(batch.orders.is_empty());
        }
    }
}

#[test]
fn invalid_admission_and_restore_cannot_change_order_state() {
    let item = order(0, 0, 1);
    let mut queue = OrderQueue::new(1);
    queue.enqueue(item.key, item.value).unwrap();
    let before = queue.snapshot();
    assert_eq!(queue.enqueue(item.key, 99), Err(QueueError::DuplicateKey));
    assert_eq!(queue.enqueue(order(1, 0, 1).key, 99), Err(QueueError::Full));
    assert_eq!(queue.snapshot(), before);
    queue.advance().unwrap();
    let before = queue.snapshot();
    assert_eq!(queue.enqueue(item.key, 99), Err(QueueError::PastTick));
    assert_eq!(queue.snapshot(), before);
    let duplicate = OrderSnapshot {
        version: ORDER_QUEUE_VERSION,
        next_tick: Some(0),
        capacity: 2,
        pending: vec![item.clone(), item],
    };
    assert_eq!(
        OrderQueue::restore(duplicate),
        Err(QueueError::InvalidSnapshot)
    );
    let mut exhausted = OrderQueue::<u64>::restore(OrderSnapshot {
        version: ORDER_QUEUE_VERSION,
        next_tick: Some(u64::MAX),
        capacity: 1,
        pending: vec![],
    })
    .unwrap();
    assert_eq!(exhausted.advance().unwrap().tick, u64::MAX);
    assert_eq!(exhausted.advance(), Err(QueueError::Exhausted));
    assert_eq!(
        exhausted.enqueue(
            OrderKey {
                tick: u64::MAX,
                issuer: 0,
                sequence: 0
            },
            0
        ),
        Err(QueueError::Exhausted)
    );
}
