//! CMB-152: explicit-clock throttle and real Hub reconnect preserve one history.
use super::*;

fn connection(id: u64) -> (Connection, mpsc::Receiver<Output>) {
    let (tx, rx) = mpsc::channel(64);
    let (cancel, _) = watch::channel(false);
    (Connection::new(id, tx, cancel), rx)
}
fn join(h: &mut Hub, c: &Connection, now: u64) {
    h.message("ABC",c,&mut false,json!({"t":"hello","token":"cmb152","name":"cmb152","spectate":false,"listing":{"public":true}}),now);
}
fn deny_count(rx: &mut mpsc::Receiver<Output>) -> usize {
    let mut count = 0;
    while let Ok(output) = rx.try_recv() {
        if matches!(output,Output::Json(ref v) if v["t"]=="deny") {
            count += 1;
        }
    }
    count
}
#[test]
fn cmb152_four_denials_per_rolling_second_survive_reconnect() {
    let mut h = Hub::new(32, String::new());
    let (old, mut old_rx) = connection(1);
    join(&mut h, &old, 0);
    assert_eq!(deny_count(&mut old_rx), 0);
    for now in [0, 100, 200, 300, 400, 500] {
        h.rooms.get_mut("ABC").unwrap().players[0].deny("attack", "unseen", now);
    }
    assert_eq!(deny_count(&mut old_rx), 4);
    assert_eq!(h.rooms["ABC"].players[0].denies, vec![0, 100, 200, 300]);
    h.disconnected("ABC", old.id, 600);
    let (new, mut new_rx) = connection(2);
    join(&mut h, &new, 700);
    assert_eq!(deny_count(&mut new_rx), 0);
    assert_eq!(h.rooms["ABC"].players[0].denies, vec![0, 100, 200, 300]);
    h.rooms.get_mut("ABC").unwrap().players[0].deny("attack", "unseen", 999);
    assert_eq!(deny_count(&mut new_rx), 0);
    h.rooms.get_mut("ABC").unwrap().players[0].deny("attack", "unseen", 1000);
    h.rooms.get_mut("ABC").unwrap().players[0].deny("attack", "unseen", 1000);
    assert_eq!(deny_count(&mut new_rx), 1);
    h.rooms.get_mut("ABC").unwrap().players[0].deny("attack", "unseen", 1100);
    assert_eq!(deny_count(&mut new_rx), 1);
    assert_eq!(
        h.rooms["ABC"].players[0].connection.as_ref().unwrap().id,
        new.id
    );
}
