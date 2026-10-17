use super::*;

struct Failed {
    goal: Point,
    dropped: bool,
}
impl PathRequestContext for Failed {
    type Error = ();
    fn current_goal(&mut self, _: UnitId, _: RequestKind) -> Option<Point> {
        Some(self.goal)
    }
    fn unit_incarnation(&self, _: UnitId) -> Option<u64> {
        Some(1)
    }
    fn knowledge_version(&self, _: UnitId) -> u64 {
        1
    }
    fn search(&mut self, _: Request) -> Result<SearchOutcome, ()> {
        Ok(SearchOutcome {
            path: vec![],
            failed: true,
        })
    }
    fn apply(&mut self, update: PathUpdate) {
        self.dropped = update.dropped.is_some();
    }
}

#[test]
fn if26_path_goal_retry_keys_fold_zero_distinguish_adjacent_and_include_kind() {
    let mut scheduler = PathRequests::new();
    let mut context = Failed {
        goal: Point { x: -0.0, z: 10.0 },
        dropped: false,
    };
    scheduler
        .begin_tick(
            TickConfig {
                budget: 20,
                retry_limit: 2,
                ..TickConfig::default()
            },
            &mut context,
        )
        .unwrap();
    let mut request = Request {
        unit: 1,
        kind: RequestKind::World,
        goal: context.goal,
    };
    scheduler.request(request, &mut context).unwrap();
    assert!(!context.dropped);
    request.goal.x = 0.0;
    scheduler.request(request, &mut context).unwrap();
    assert!(context.dropped, "signed zero keeps the retry identity");

    request.goal.x = 1.0;
    scheduler.request(request, &mut context).unwrap();
    assert!(!context.dropped);
    request.goal.x = f64::from_bits(1.0_f64.to_bits() + 1);
    scheduler.request(request, &mut context).unwrap();
    assert!(
        !context.dropped,
        "adjacent doubles start another retry identity"
    );
    request.kind = RequestKind::Amove;
    scheduler.request(request, &mut context).unwrap();
    assert!(!context.dropped, "kind is part of the goal key");
    scheduler.request(request, &mut context).unwrap();
    assert!(context.dropped);
    request.goal.x = f64::NAN;
    assert_eq!(
        scheduler.request(request, &mut context),
        Err(SchedulerError::InvalidGoal)
    );
}
