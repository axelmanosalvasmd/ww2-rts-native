//! Native socket intent checks. No Source recorder or new fixture is required.
use super::*;

fn raw_text(client: &mut Client, text: &str) {
    let payload = text.as_bytes();
    assert!(payload.len() < 126);
    let mask = [7_u8, 11, 17, 23];
    let mut frame = vec![0x81, 0x80 | payload.len() as u8];
    frame.extend(mask);
    frame.extend(
        payload
            .iter()
            .enumerate()
            .map(|(index, byte)| byte ^ mask[index % 4]),
    );
    client.0.write_all(&frame).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn malformed_frames_before_and_after_hello_leave_the_room_usable() {
    sockets(|rig| {
        let mut host = Client::open(rig.address, "BatchBad");
        let malformed = [
            "{",
            "null",
            "[]",
            "false",
            "17",
            "\"text\"",
            "{}",
            "{\"t\":9}",
        ];
        for text in malformed {
            raw_text(&mut host, text);
        }
        host.send(json!({"t":"name","name":"Too early"}));
        host.send(json!({"t":"hello","token":false,"name":"Bad token"}));
        host.send(json!({"t":"hello","token":"host","name":"Host"}));
        let lobby = host.receive();
        assert_eq!(lobby["t"], "lobby");
        assert_eq!(lobby["players"].as_array().unwrap().len(), 1);
        assert_eq!(lobby["players"][0]["name"], "Host");
        for text in malformed {
            raw_text(&mut host, text);
        }
        for name in [json!(null), json!(false), json!(9), json!([]), json!({})] {
            host.send(json!({"t":"name","name":name}));
        }
        assert!(
            host.barrier().is_empty(),
            "all malformed frames remain silent"
        );
        host.send(json!({"t":"name","name":"Still here"}));
        let lobby = host.receive();
        assert_eq!(lobby["t"], "lobby");
        assert_eq!(lobby["players"][0]["name"], "Still here");
        let (mut guest, joined) = rig.join("BatchBad", "guest", "Guest", false);
        assert_eq!(joined["players"][0]["name"], "Still here");
        host.barrier();
        guest.send(json!({"t":"faction","slot":1,"v":3}));
        assert_eq!(guest.lobby()["players"][1]["faction"], 3);
        assert_eq!(host.lobby()["players"][1]["faction"], 3);
    })
    .await;
}
