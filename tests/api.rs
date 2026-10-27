use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use std::time::Duration;

use pito_jev::{Billing, Client, Failure, Key, Question, Usage, body};
use serde_json::{Value, json};

struct Heard {
    head: String,
    body: String,
}

struct Fake {
    url: String,
    heard: Arc<Mutex<Vec<Heard>>>,
    _hold: Option<mpsc::Sender<()>>,
}

impl Fake {
    fn heard(&self) -> Vec<(String, String)> {
        self.heard
            .lock()
            .unwrap()
            .iter()
            .map(|heard| (heard.head.clone(), heard.body.clone()))
            .collect()
    }
}

fn read_request(stream: &std::net::TcpStream) -> Heard {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut head = String::new();
    let mut length = 0;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap_or(0);
        }
        head.push_str(&line);
    }
    let mut body = vec![0; length];
    let _ = reader.read_exact(&mut body);
    Heard {
        head,
        body: String::from_utf8_lossy(&body).into_owned(),
    }
}

fn serve(status: u16, headers: &[(&str, &str)], body: &str) -> Fake {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/api/v1/systemone", listener.local_addr().unwrap());
    let heard = Arc::new(Mutex::new(Vec::new()));
    let log = heard.clone();
    let mut extra = String::new();
    for (name, value) in headers {
        extra.push_str(&format!("{name}: {value}\r\n"));
    }
    let body = body.to_string();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let request = read_request(&stream);
            log.lock().unwrap().push(request);
            let head = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(body.as_bytes());
        }
    });
    Fake {
        url,
        heard,
        _hold: None,
    }
}

fn silent() -> Fake {
    stalled("")
}

fn stalled(sent: &str) -> Fake {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/api/v1/systemone", listener.local_addr().unwrap());
    let heard = Arc::new(Mutex::new(Vec::new()));
    let log = heard.clone();
    let sent = sent.to_string();
    let (hold, wait) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let request = read_request(&stream);
            log.lock().unwrap().push(request);
            let _ = stream.write_all(sent.as_bytes());
            let _ = wait.recv();
            drop(stream);
        }
    });
    Fake {
        url,
        heard,
        _hold: Some(hold),
    }
}

fn key() -> Key {
    Key::new("jev-test-key-0001").unwrap()
}

fn client(fake: &Fake) -> Client {
    Client {
        url: fake.url.clone(),
        deadline: Duration::from_millis(300),
    }
}

const ANSWERED: &str = r#"{"model":"jev-1.14.0","answers":{"ready":{"type":"noul","noul":0.1,"probabilities":{"true":0.1,"false":0.9},"confidence":0.8}},"usage":{"input_tokens":1234,"output_tokens":3}}"#;

const BILLED: [(&str, &str); 4] = [
    ("X-Jev-Run-Id", "run-42"),
    ("X-Jev-Credits-Charged", "0.25"),
    ("X-Jev-Tokens-Remaining", "998766"),
    ("X-Jev-Credits-Remaining", "41.75"),
];

fn yes_no() -> Question {
    Question {
        instructions: "Is this train ready to merge?".into(),
        yes: Some("it can merge as it is".into()),
        no: None,
        ..Question::default()
    }
}

#[test]
fn an_answer_reads_with_its_usage_and_billing_and_the_key_rides_only_the_header() {
    let fake = serve(200, &BILLED, ANSWERED);
    let body = body(
        "jev-1.14.0",
        &json!("train train-1 into main"),
        &[("ready", yes_no())],
    )
    .unwrap();
    let called = client(&fake).ask(&key(), &body).unwrap();
    let answer = called.reply.answer("ready").unwrap();
    assert_eq!(answer.kind, "noul");
    assert_eq!(answer.yes(), Some(0.1));
    assert_eq!(answer.confidence, Some(0.8));
    assert_eq!(called.reply.model, "jev-1.14.0");
    assert_eq!(
        called.reply.usage,
        Usage {
            input_tokens: 1234,
            output_tokens: 3
        }
    );
    assert_eq!(
        called.billing,
        Billing {
            run_id: Some("run-42".into()),
            credits_charged: Some(0.25),
            tokens_remaining: Some(998_766.0),
            credits_remaining: Some(41.75),
        }
    );
    let heard = fake.heard();
    assert_eq!(heard.len(), 1, "one call, never a retry");
    let (head, sent) = &heard[0];
    assert!(head.starts_with("POST /api/v1/systemone "), "{head}");
    assert!(head.contains("Bearer jev-test-key-0001"), "{head}");
    assert!(
        head.to_ascii_lowercase()
            .contains("content-type: application/json")
    );
    assert!(!sent.contains("jev-test-key-0001"));
    let sent: Value = serde_json::from_str(sent).unwrap();
    assert_eq!(sent["model"], "jev-1.14.0");
    assert_eq!(sent["state"], "train train-1 into main");
    assert_eq!(sent["questions"]["ready"]["type"], "noul");
    assert_eq!(
        sent["questions"]["ready"]["criteria"],
        json!({"true": "it can merge as it is"})
    );
}

#[test]
fn every_error_code_maps_to_its_failure_after_one_call() {
    for (status, headers, body, want) in [
        (
            401,
            vec![],
            r#"{"error":{"code":"unauthorized","message":"bad key"}}"#,
            Failure::BadKey,
        ),
        (
            402,
            vec![],
            r#"{"error":{"code":"payment_required"}}"#,
            Failure::NoBalance,
        ),
        (
            422,
            vec![],
            r#"{"error":{"code":"invalid","message":"questions is empty"}}"#,
            Failure::BadBody("invalid: questions is empty".into()),
        ),
        (
            429,
            vec![("Retry-After", "30")],
            "{}",
            Failure::Limited(Some(30)),
        ),
        (429, vec![], "{}", Failure::Limited(None)),
        (502, vec![], "", Failure::Unavailable(502)),
        (503, vec![], "", Failure::Unavailable(503)),
        (504, vec![], "", Failure::Unknown),
        (
            500,
            vec![],
            r#"{"error":{"message":"boom"}}"#,
            Failure::Other(500, "boom".into()),
        ),
    ] {
        let fake = serve(status, &headers, body);
        let got = client(&fake).ask(&key(), "{}").unwrap_err();
        assert_eq!(got, want, "{status}");
        assert_eq!(fake.heard().len(), 1, "{status} is never retried");
        assert!(!got.words().is_empty());
        assert!(!got.words().contains("jev-test-key-0001"));
    }
}

#[test]
fn a_reply_that_is_not_json_is_malformed() {
    let fake = serve(200, &[], "not json");
    assert!(matches!(
        client(&fake).ask(&key(), "{}"),
        Err(Failure::Malformed(_))
    ));
}

#[test]
fn no_answer_within_the_deadline_is_a_deadline_failure() {
    let fake = silent();
    let got = client(&fake).ask(&key(), "{}");
    assert_eq!(got, Err(Failure::Deadline));
    assert_eq!(Failure::Deadline.words(), "no answer within 2 s");
}

#[test]
fn a_closed_port_is_unreached() {
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let client = Client {
        url: format!("http://127.0.0.1:{port}/x"),
        deadline: Duration::from_millis(300),
    };
    assert!(matches!(
        client.ask(&key(), "{}"),
        Err(Failure::Unreached(_) | Failure::Deadline)
    ));
}

#[test]
fn a_trimmed_key_rides_the_header_as_given() {
    let fake = serve(200, &[], ANSWERED);
    let key = Key::new("\tjev-test-key-0002 \n").unwrap();
    client(&fake).ask(&key, "{}").unwrap();
    let heard = fake.heard();
    assert!(
        heard[0].0.contains("Bearer jev-test-key-0002\r\n"),
        "{}",
        heard[0].0
    );
}

#[test]
fn a_reply_over_the_cap_is_malformed_and_not_read_whole() {
    let huge = format!(
        r#"{{"model":"m","answers":{{}},"pad":"{}"}}"#,
        "x".repeat(pito_jev::MOST_REPLY as usize)
    );
    let fake = serve(200, &[], &huge);
    assert_eq!(
        client(&fake).ask(&key(), "{}"),
        Err(Failure::Malformed("the reply is over 1 MiB".into()))
    );
}

#[test]
fn an_empty_reply_object_reads_as_no_answers() {
    let fake = serve(200, &[], "{}");
    let called = client(&fake).ask(&key(), "{}").unwrap();
    assert_eq!(called.reply.model, "");
    assert!(called.reply.answers.is_empty());
    assert_eq!(called.reply.usage, Usage::default());
    assert_eq!(called.billing, Billing::default());
    assert_eq!(called.reply.answer("ready"), None);
}

#[test]
fn a_reply_that_stops_halfway_runs_into_the_deadline() {
    let fake = stalled(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 100\r\n\r\n{\"model\":",
    );
    let began = std::time::Instant::now();
    assert_eq!(client(&fake).ask(&key(), "{}"), Err(Failure::Deadline));
    assert!(began.elapsed() >= Duration::from_millis(300));
    assert_eq!(fake.heard().len(), 1);
}

#[test]
fn a_reply_cut_short_is_malformed() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/api/v1/systemone", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            read_request(&stream);
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 100\r\n\r\n{\"model\":",
            );
        }
    });
    let client = Client {
        url,
        deadline: Duration::from_millis(2_000),
    };
    let began = std::time::Instant::now();
    assert!(matches!(
        client.ask(&key(), "{}"),
        Err(Failure::Malformed(why)) if !why.is_empty()
    ));
    assert!(began.elapsed() < Duration::from_millis(2_000));
}
