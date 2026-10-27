#[path = "../examples/support/stand_in.rs"]
mod stand_in;

use std::net::TcpListener;
use std::time::{Duration, Instant};

use pito_jev::{
    Client, Failure, Key, MODEL, MOST_BODY, MOST_QUESTIONS, MOST_REPLY, Question, REQUEST_DEADLINE,
    body,
};
use serde_json::{Value, json};

const KEY: &str = "stand-in-key-0001";

fn key() -> Key {
    Key::new(KEY).unwrap()
}

fn at(route: &str) -> Client {
    let address = stand_in::start(0, KEY).unwrap();
    let route = if route.is_empty() {
        String::new()
    } else {
        format!("/{route}")
    };
    Client {
        url: format!("http://{address}/api/v1/systemone{route}"),
        deadline: Duration::from_millis(500),
    }
}

fn ready() -> Question {
    Question {
        instructions: "Is this train ready to merge?".into(),
        yes: Some("every branch is reviewed and the gate is green".into()),
        ..Question::default()
    }
}

fn track() -> Question {
    Question {
        instructions: "Which track?".into(),
        options: vec![
            ("fast".into(), Some("the short gate".into())),
            ("full".into(), Some("every suite".into())),
        ],
        ..Question::default()
    }
}

fn one() -> String {
    body(
        MODEL,
        &json!("train 41: 6 branches, gate green"),
        &[("ready", ready())],
    )
    .unwrap()
}

#[test]
fn a_yes_no_question_reads_its_probability_tokens_and_credits() {
    let called = at("").ask(&key(), &one()).unwrap();
    assert_eq!(called.reply.model, MODEL);
    let answer = called.reply.answer("ready").unwrap();
    assert_eq!(answer.kind, "noul");
    let yes = answer.yes().unwrap();
    assert!((0.05..=0.95).contains(&yes), "{yes}");
    assert!(answer.confidence.is_some());
    assert_eq!(answer.picked(), None);
    assert_eq!(called.reply.usage.input_tokens, one().len() as u64 / 4);
    assert_eq!(called.reply.usage.output_tokens, 1);
    assert_eq!(called.billing.run_id.as_deref(), Some("run-0001"));
    assert_eq!(called.billing.credits_charged, Some(0.25));
    assert_eq!(called.billing.credits_remaining, Some(49.75));
    assert!(called.billing.tokens_remaining.is_some());
    assert!(called.latency_ms < 500);
}

#[test]
fn the_same_question_gets_the_same_made_up_answer() {
    let client = at("");
    let first = client.ask(&key(), &one()).unwrap();
    let second = client.ask(&key(), &one()).unwrap();
    assert_eq!(
        first.reply.answer("ready").unwrap().yes(),
        second.reply.answer("ready").unwrap().yes()
    );
    assert_eq!(second.billing.run_id.as_deref(), Some("run-0002"));
    assert_eq!(second.billing.credits_remaining, Some(49.5));
}

#[test]
fn a_choice_question_reads_its_pick_and_every_option() {
    let text = body(MODEL, &json!({"train": 41}), &[("track", track())]).unwrap();
    let called = at("").ask(&key(), &text).unwrap();
    let answer = called.reply.answer("track").unwrap();
    assert_eq!(answer.kind, "choice");
    assert_eq!(answer.yes(), None);
    let (pick, p) = answer.picked().unwrap();
    assert!(pick == "fast" || pick == "full", "{pick}");
    let options = answer.probabilities.as_object().unwrap();
    assert_eq!(options.len(), 2);
    let best = options
        .values()
        .filter_map(Value::as_f64)
        .fold(0.0, f64::max);
    assert_eq!(p, best);
    let sum: f64 = options.values().filter_map(Value::as_f64).sum();
    assert!((sum - 1.0).abs() < 0.02, "{sum}");
}

#[test]
fn sixty_four_questions_go_in_one_call_and_each_is_answered() {
    let names: Vec<String> = (0..MOST_QUESTIONS).map(|n| format!("q{n:02}")).collect();
    let questions: Vec<(&str, Question)> = names
        .iter()
        .enumerate()
        .map(|(n, name)| {
            let question = if n % 2 == 0 { ready() } else { track() };
            (name.as_str(), question)
        })
        .collect();
    let text = body(MODEL, &json!("a busy day"), &questions).unwrap();
    let called = at("").ask(&key(), &text).unwrap();
    assert_eq!(called.reply.answers.len(), 64);
    for (n, name) in names.iter().enumerate() {
        let answer = called.reply.answer(name).unwrap();
        if n % 2 == 0 {
            assert!(answer.yes().is_some(), "{name}");
        } else {
            assert!(answer.picked().is_some(), "{name}");
        }
    }
    let order: Vec<&String> = called.reply.answers.keys().collect();
    assert_eq!(order, names.iter().collect::<Vec<_>>());
    assert_eq!(called.reply.usage.output_tokens, 64);
    assert_eq!(called.billing.credits_charged, Some(16.0));
}

#[test]
fn a_sixty_fifth_question_is_refused_before_anything_is_sent() {
    let names: Vec<String> = (0..=MOST_QUESTIONS).map(|n| format!("q{n}")).collect();
    let questions: Vec<(&str, Question)> =
        names.iter().map(|name| (name.as_str(), ready())).collect();
    assert_eq!(
        body(MODEL, &json!({}), &questions),
        Err("a call takes 1 to 64 questions, not 65".to_string())
    );
    let mut asked = serde_json::Map::new();
    for name in &names {
        asked.insert(name.clone(), json!({"type": "noul", "instructions": "?"}));
    }
    let by_hand = json!({"model": MODEL, "state": {}, "questions": asked}).to_string();
    assert_eq!(
        at("").ask(&key(), &by_hand),
        Err(Failure::BadBody(
            "invalid: a call takes 1 to 64 questions".into()
        ))
    );
}

#[test]
fn a_body_over_the_cap_is_refused_before_anything_is_sent() {
    let state = json!("x".repeat(MOST_BODY));
    let refused = body(MODEL, &state, &[("ready", ready())]).unwrap_err();
    assert!(refused.contains("over Jev's 256000"), "{refused}");
    let by_hand = json!({"model": MODEL, "state": state, "questions": {"ready": {"type": "noul", "instructions": "?"}}}).to_string();
    assert!(by_hand.len() > MOST_BODY);
    assert_eq!(
        at("").ask(&key(), &by_hand),
        Err(Failure::BadBody(
            "too_large: the body is over 256000 bytes".into()
        ))
    );
}

#[test]
fn a_body_at_the_cap_is_answered() {
    let bare = body(MODEL, &json!(""), &[("ready", ready())])
        .unwrap()
        .len();
    let state = json!("x".repeat(MOST_BODY - bare));
    let text = body(MODEL, &state, &[("ready", ready())]).unwrap();
    assert_eq!(text.len(), MOST_BODY);
    let called = Client {
        deadline: REQUEST_DEADLINE,
        ..at("")
    }
    .ask(&key(), &text)
    .unwrap();
    assert!(called.reply.answer("ready").unwrap().yes().is_some());
}

#[test]
fn a_refused_key_is_bad_key() {
    let wrong = Key::new("wrong-key").unwrap();
    let got = at("").ask(&wrong, &one()).unwrap_err();
    assert_eq!(got, Failure::BadKey);
    assert_eq!(got.to_string(), "401: Jev refused the key");
    assert!(!got.to_string().contains(KEY));
    assert_eq!(got.retry_after_ms(), None);
}

#[test]
fn a_refused_key_wins_over_every_route() {
    let wrong = Key::new("wrong-key").unwrap();
    for route in ["status/429", "garbled", "huge", "nowhere"] {
        assert_eq!(
            at(route).ask(&wrong, &one()),
            Err(Failure::BadKey),
            "{route}"
        );
    }
}

#[test]
fn a_rate_limit_names_its_wait() {
    let got = at("status/429").ask(&key(), &one()).unwrap_err();
    assert_eq!(got, Failure::Limited(Some(30)));
    assert_eq!(got.retry_after_ms(), Some(30_000));
    assert_eq!(
        got.to_string(),
        "429: Jev is rate limiting; no call for 30 s"
    );
}

#[test]
fn every_other_status_is_its_own_failure() {
    let said = "stand_in: a status asked for by path".to_string();
    for (route, want) in [
        ("status/402", Failure::NoBalance),
        ("status/422", Failure::BadBody(said.clone())),
        ("status/502", Failure::Unavailable(502)),
        ("status/503", Failure::Unavailable(503)),
        ("status/504", Failure::Unknown),
        ("status/500", Failure::Other(500, said.clone())),
        (
            "nowhere",
            Failure::Other(404, "not_found: no such route on the stand-in".into()),
        ),
    ] {
        assert_eq!(at(route).ask(&key(), &one()), Err(want), "{route}");
    }
}

#[test]
fn a_body_that_is_not_json_is_refused_as_a_bad_body() {
    assert_eq!(
        at("").ask(&key(), "not json"),
        Err(Failure::BadBody("invalid: the body is not JSON".into()))
    );
}

#[test]
fn a_reply_that_is_not_json_is_malformed() {
    let got = at("garbled").ask(&key(), &one()).unwrap_err();
    assert!(
        matches!(&got, Failure::Malformed(why) if !why.is_empty()),
        "{got:?}"
    );
    assert!(got.to_string().starts_with("Jev's answer did not read: "));
}

#[test]
fn a_reply_over_one_mebibyte_is_malformed() {
    let client = Client {
        deadline: REQUEST_DEADLINE,
        ..at("huge")
    };
    assert_eq!(
        client.ask(&key(), &one()),
        Err(Failure::Malformed("the reply is over 1 MiB".into()))
    );
    assert_eq!(MOST_REPLY, 1 << 20);
}

#[test]
fn a_slow_answer_runs_into_the_deadline() {
    let client = Client {
        deadline: Duration::from_millis(200),
        ..at("slow")
    };
    let began = Instant::now();
    assert_eq!(client.ask(&key(), &one()), Err(Failure::Deadline));
    let took = began.elapsed();
    assert!(took >= Duration::from_millis(200), "{took:?}");
    assert!(took < Duration::from_millis(2_000), "{took:?}");
}

#[test]
fn the_live_deadline_holds_a_call_to_two_seconds() {
    let client = Client {
        deadline: REQUEST_DEADLINE,
        ..at("slow")
    };
    let began = Instant::now();
    let got = client.ask(&key(), &one());
    let took = began.elapsed();
    assert_eq!(got, Err(Failure::Deadline));
    assert_eq!(Failure::Deadline.to_string(), "no answer within 2 s");
    assert!(took >= REQUEST_DEADLINE, "{took:?}");
    assert!(took < Duration::from_millis(2_500), "{took:?}");
}

#[test]
fn a_closed_port_is_unreached() {
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let client = Client {
        url: format!("http://127.0.0.1:{port}/api/v1/systemone"),
        deadline: Duration::from_millis(500),
    };
    assert_eq!(
        client.ask(&key(), &one()),
        Err(Failure::Unreached("the connection was refused".into()))
    );
}

#[test]
fn a_host_that_does_not_parse_is_unreached() {
    let client = Client {
        url: "http://exa mple.invalid/".into(),
        deadline: Duration::from_millis(500),
    };
    assert!(matches!(
        client.ask(&key(), &one()),
        Err(Failure::Unreached(_))
    ));
}
