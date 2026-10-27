use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use std::time::Duration;

use pito_jev::{
    Answer, Billing, Client, Failure, Key, MOST_BODY, Question, Usage, billing_of, body,
    failure_of, reply_of,
};
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
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/api/v1/systemone", listener.local_addr().unwrap());
    let heard = Arc::new(Mutex::new(Vec::new()));
    let log = heard.clone();
    let (hold, wait) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            let request = read_request(&stream);
            log.lock().unwrap().push(request);
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
fn a_choice_reads_its_pick_with_its_probability_or_the_confidence() {
    let answer: Answer = serde_json::from_value(
        json!({"type": "choice", "choice": "c2", "probabilities": {"c1": 0.1, "c2": 0.9}}),
    )
    .unwrap();
    assert_eq!(answer.picked(), Some(("c2".to_string(), 0.9)));
    let answer: Answer =
        serde_json::from_value(json!({"type": "choice", "choice": "c2", "confidence": 0.7}))
            .unwrap();
    assert_eq!(answer.picked(), Some(("c2".to_string(), 0.7)));
    let answer: Answer =
        serde_json::from_value(json!({"type": "choice", "choice": "", "confidence": 0.7})).unwrap();
    assert_eq!(answer.picked(), None);
    let answer: Answer = serde_json::from_value(
        json!({"type": "choice", "choice": "c1", "probabilities": {"c1": 1.5}}),
    )
    .unwrap();
    assert_eq!(answer.picked(), None);
    let question = Question {
        instructions: "which?".into(),
        options: vec![("c1".into(), Some("first".into())), ("hold".into(), None)],
        ..Question::default()
    };
    let text = body("m", &json!({}), &[("next", question)]).unwrap();
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        value["questions"]["next"],
        json!({"type": "choice", "instructions": "which?", "criteria": {"c1": "first", "hold": null}})
    );
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
fn the_live_client_aims_at_jev_with_a_two_second_deadline() {
    let live = Client::live();
    assert_eq!(live.url, "https://jev-ai.pro/api/v1/systemone");
    assert_eq!(live.deadline, Duration::from_secs(2));
}

#[test]
fn bodies_hold_jevs_shape_and_limits() {
    let text = body("m", &json!("text"), &[("a", yes_no()), ("b", yes_no())]).unwrap();
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["model"], "m");
    assert_eq!(value["state"], "text");
    assert_eq!(value["questions"]["b"]["type"], "noul");
    assert_eq!(
        value["questions"]["a"]["instructions"],
        "Is this train ready to merge?"
    );
    assert!(body("m", &json!({}), &[]).is_err());
    let many: Vec<(&str, Question)> = (0..65).map(|_| ("a", yes_no())).collect();
    assert!(
        body("m", &json!({}), &many)
            .unwrap_err()
            .contains("1 to 64")
    );
    assert!(
        body("m", &json!({}), &[("a", yes_no()), ("a", yes_no())])
            .unwrap_err()
            .contains("twice")
    );
    let bare = Question {
        instructions: "?".into(),
        yes: None,
        no: None,
        ..Question::default()
    };
    let text = body("m", &json!({}), &[("a", bare)]).unwrap();
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        value["questions"]["a"],
        json!({"type": "noul", "instructions": "?"})
    );
    let both = Question {
        instructions: "?".into(),
        yes: Some("y".into()),
        no: Some("n".into()),
        ..Question::default()
    };
    let text = body("m", &json!({}), &[("a", both)]).unwrap();
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        value["questions"]["a"]["criteria"],
        json!({"true": "y", "false": "n"})
    );
    let huge = json!("x".repeat(MOST_BODY));
    assert!(
        body("m", &huge, &[("a", yes_no())])
            .unwrap_err()
            .contains("256000")
    );
}

#[test]
fn a_yes_needs_its_probability_in_range() {
    let answer = |value: Value| -> Answer { serde_json::from_value(value).unwrap() };
    assert_eq!(
        answer(json!({"type": "noul", "noul": 1.0})).yes(),
        Some(1.0)
    );
    assert_eq!(
        answer(json!({"type": "noul", "noul": 0.0})).yes(),
        Some(0.0)
    );
    assert_eq!(answer(json!({"type": "noul"})).yes(), None);
    assert_eq!(answer(json!({"type": "noul", "noul": 1.5})).yes(), None);
    assert_eq!(answer(json!({"type": "noul", "noul": -0.1})).yes(), None);
    let reply = reply_of(r#"{"answers":{"x":{"type":"noul","noul":0.7}}}"#).unwrap();
    assert_eq!(reply.answer("x").unwrap().noul, Some(0.7));
    assert_eq!(reply.answer("y"), None);
}

#[test]
fn billing_headers_that_do_not_read_are_left_out() {
    let billing = billing_of(&|name| match name {
        "x-jev-run-id" => Some("  ".into()),
        "x-jev-credits-charged" => Some("NaN".into()),
        "x-jev-credits-remaining" => Some("12".into()),
        _ => None,
    });
    assert_eq!(
        billing,
        Billing {
            run_id: None,
            credits_charged: None,
            tokens_remaining: None,
            credits_remaining: Some(12.0),
        }
    );
}

#[test]
fn a_429_names_its_wait_and_others_none() {
    assert_eq!(Failure::Limited(Some(30)).retry_after_ms(), Some(30_000));
    assert_eq!(Failure::Limited(None).retry_after_ms(), None);
    assert_eq!(Failure::Unknown.retry_after_ms(), None);
    assert_eq!(failure_of(429, Some(" 7 "), ""), Failure::Limited(Some(7)));
    assert_eq!(failure_of(429, Some("soon"), ""), Failure::Limited(None));
    assert_eq!(
        failure_of(418, None, "nope"),
        Failure::Other(418, String::new())
    );
    assert_eq!(
        Failure::Other(418, String::new()).words(),
        "418: Jev answered 418"
    );
}

#[test]
fn a_key_is_one_word_and_never_shows_itself() {
    let key = Key::new("  jev-test-key-0001\n").unwrap();
    assert_eq!(Key::new("jev-test-key-0001"), Some(key.clone()));
    assert_eq!(format!("{key:?}"), "Key(hidden)");
    assert_eq!(format!("{key}"), "hidden");
    assert_eq!(format!("{:?}", Some(&key)), "Some(Key(hidden))");
    assert_eq!(Key::new(""), None);
    assert_eq!(Key::new("   "), None);
    assert_eq!(Key::new("two words"), None);
    assert_eq!(Key::new("tab\tinside"), None);
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
fn a_failure_reads_as_its_words_and_is_an_error() {
    let failure = Failure::Limited(Some(30));
    assert_eq!(failure.to_string(), failure.words());
    assert_eq!(
        failure.to_string(),
        "429: Jev is rate limiting; no call for 30 s"
    );
    let error: Box<dyn std::error::Error> = Box::new(Failure::BadKey);
    assert_eq!(error.to_string(), "401: Jev refused the key");
    assert_eq!(
        Failure::BadBody("x".into()).words(),
        "422: Jev refused the body (x); never sent again unchanged"
    );
    assert_eq!(Failure::NoBalance.words(), "402: Jev's balance is spent");
    assert_eq!(Failure::Unavailable(503).words(), "503: Jev is unavailable");
    assert_eq!(Failure::Unknown.words(), "504: Jev's outcome is unknown");
    assert_eq!(
        Failure::Unreached("the connection was refused".into()).words(),
        "Jev was not reached: the connection was refused"
    );
    assert_eq!(
        Failure::Malformed("eof".into()).words(),
        "Jev's answer did not read: eof"
    );
    assert_eq!(Failure::Other(500, "boom".into()).words(), "500: boom");
}

#[test]
fn what_jev_said_is_clipped_to_two_hundred_characters() {
    let long = "é".repeat(500);
    let body = json!({"error": {"message": long}}).to_string();
    let Failure::BadBody(said) = failure_of(422, None, &body) else {
        panic!("not a 422");
    };
    assert_eq!(said.chars().count(), 200);
    assert!(said.ends_with('…'));
    assert_eq!(
        failure_of(422, None, r#"{"error":{"code":"invalid"}}"#),
        Failure::BadBody("invalid".into())
    );
    assert_eq!(failure_of(422, None, "{}"), Failure::BadBody(String::new()));
}

#[test]
fn a_reply_over_the_cap_is_malformed_and_not_read_whole() {
    let huge = format!(
        r#"{{"model":"m","answers":{{}},"pad":"{}"}}"#,
        "x".repeat(pito_jev::MOST_REPLY as usize)
    );
    let fake = serve(200, &[], &huge);
    assert!(matches!(
        client(&fake).ask(&key(), "{}"),
        Err(Failure::Malformed(_))
    ));
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
fn questions_keep_the_order_they_are_given() {
    let ids = ["zeta", "alpha", "mid"];
    let questions: Vec<(&str, Question)> = ids.iter().map(|id| (*id, yes_no())).collect();
    let text = body(pito_jev::MODEL, &json!({}), &questions).unwrap();
    let value: Value = serde_json::from_str(&text).unwrap();
    let order: Vec<&str> = value["questions"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(order, ids);
    assert_eq!(value["model"], "jev-1.14.0");
    let names: Vec<String> = (0..64).map(|n| format!("q{n}")).collect();
    let full: Vec<(&str, Question)> = names.iter().map(|name| (name.as_str(), yes_no())).collect();
    assert!(body("m", &json!({}), &full).is_ok());
}

#[test]
fn the_default_client_is_the_live_one() {
    assert_eq!(Client::default(), Client::live());
}
