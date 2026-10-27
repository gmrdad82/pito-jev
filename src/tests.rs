use super::*;

fn yes_no() -> Question {
    Question {
        instructions: "Is this train ready to merge?".into(),
        yes: Some("it can merge as it is".into()),
        ..Question::default()
    }
}

fn answer(value: Value) -> Answer {
    serde_json::from_value(value).unwrap()
}

fn sent(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

#[test]
fn a_key_is_one_trimmed_word() {
    let key = Key::new("  jev-test-key-0001\n").unwrap();
    assert_eq!(Key::new("jev-test-key-0001"), Some(key.clone()));
    assert_eq!(key.bearer(), "Bearer jev-test-key-0001");
    assert_eq!(Key::new(""), None);
    assert_eq!(Key::new("   "), None);
    assert_eq!(Key::new("two words"), None);
    assert_eq!(Key::new("tab\tinside"), None);
    assert_eq!(Key::new("line\nbreak"), None);
}

#[test]
fn a_key_never_shows_itself() {
    let key = Key::new("jev-test-key-0001").unwrap();
    assert_eq!(format!("{key:?}"), "Key(hidden)");
    assert_eq!(format!("{key:#?}"), "Key(hidden)");
    assert_eq!(format!("{key}"), "hidden");
    assert_eq!(format!("{:?}", Some(&key)), "Some(Key(hidden))");
    assert_eq!(format!("{:?}", vec![key.clone()]), "[Key(hidden)]");
}

#[test]
fn a_bare_yes_no_question_has_no_criteria() {
    let question = Question {
        instructions: "?".into(),
        ..Question::default()
    };
    assert_eq!(
        question.json(),
        json!({"type": "noul", "instructions": "?"})
    );
}

#[test]
fn a_yes_no_question_carries_only_the_meanings_it_is_given() {
    assert_eq!(
        yes_no().json()["criteria"],
        json!({"true": "it can merge as it is"})
    );
    let no_only = Question {
        instructions: "?".into(),
        no: Some("n".into()),
        ..Question::default()
    };
    assert_eq!(no_only.json()["criteria"], json!({"false": "n"}));
    let both = Question {
        instructions: "?".into(),
        yes: Some("y".into()),
        no: Some("n".into()),
        ..Question::default()
    };
    assert_eq!(both.json()["criteria"], json!({"true": "y", "false": "n"}));
}

#[test]
fn options_make_a_choice_and_win_over_yes_and_no() {
    let question = Question {
        instructions: "which?".into(),
        yes: Some("ignored".into()),
        options: vec![("c1".into(), Some("first".into())), ("hold".into(), None)],
        ..Question::default()
    };
    assert_eq!(
        question.json(),
        json!({"type": "choice", "instructions": "which?", "criteria": {"c1": "first", "hold": null}})
    );
    let keys: Vec<String> = question.json()["criteria"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert_eq!(keys, ["c1", "hold"]);
}

#[test]
fn a_body_holds_the_model_the_state_and_the_questions() {
    let text = body(
        MODEL,
        &json!({"train": 41}),
        &[("a", yes_no()), ("b", yes_no())],
    )
    .unwrap();
    let value = sent(&text);
    assert_eq!(value["model"], "jev-1.14.0");
    assert_eq!(value["state"], json!({"train": 41}));
    assert_eq!(value["questions"]["a"], yes_no().json());
    assert_eq!(value["questions"]["b"], yes_no().json());
    let keys: Vec<&String> = value.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["model", "state", "questions"]);
}

#[test]
fn a_body_keeps_the_order_the_questions_are_given() {
    let ids = ["zeta", "alpha", "mid"];
    let questions: Vec<(&str, Question)> = ids.iter().map(|id| (*id, yes_no())).collect();
    let value = sent(&body("m", &json!({}), &questions).unwrap());
    let order: Vec<&str> = value["questions"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(order, ids);
}

#[test]
fn a_body_takes_one_to_sixty_four_questions() {
    assert_eq!(
        body("m", &json!({}), &[]),
        Err("a call takes 1 to 64 questions, not 0".to_string())
    );
    let names: Vec<String> = (0..65).map(|n| format!("q{n}")).collect();
    let all: Vec<(&str, Question)> = names.iter().map(|name| (name.as_str(), yes_no())).collect();
    assert!(body("m", &json!({}), &all[..1]).is_ok());
    let full = sent(&body("m", &json!({}), &all[..MOST_QUESTIONS]).unwrap());
    assert_eq!(full["questions"].as_object().unwrap().len(), 64);
    assert_eq!(
        body("m", &json!({}), &all),
        Err("a call takes 1 to 64 questions, not 65".to_string())
    );
}

#[test]
fn a_body_refuses_an_id_given_twice() {
    assert_eq!(
        body(
            "m",
            &json!({}),
            &[("a", yes_no()), ("b", yes_no()), ("a", yes_no())]
        ),
        Err("the question id a is given twice".to_string())
    );
}

#[test]
fn a_body_may_be_exactly_the_cap_and_not_a_byte_over() {
    let bare = body("m", &json!(""), &[("a", yes_no())]).unwrap().len();
    let room = MOST_BODY - bare;
    let at = body("m", &json!("x".repeat(room)), &[("a", yes_no())]).unwrap();
    assert_eq!(at.len(), MOST_BODY);
    assert_eq!(
        body("m", &json!("x".repeat(room + 1)), &[("a", yes_no())]),
        Err("the body is 256001 bytes, over Jev's 256000".to_string())
    );
}

#[test]
fn a_yes_needs_its_probability_between_zero_and_one() {
    assert_eq!(
        answer(json!({"type": "noul", "noul": 1.0})).yes(),
        Some(1.0)
    );
    assert_eq!(
        answer(json!({"type": "noul", "noul": 0.0})).yes(),
        Some(0.0)
    );
    assert_eq!(
        answer(json!({"type": "noul", "noul": 0.3})).yes(),
        Some(0.3)
    );
    assert_eq!(answer(json!({"type": "noul"})).yes(), None);
    assert_eq!(answer(json!({"type": "noul", "noul": 1.5})).yes(), None);
    assert_eq!(answer(json!({"type": "noul", "noul": -0.1})).yes(), None);
}

#[test]
fn a_pick_reads_its_probability_or_else_the_confidence() {
    let probable = answer(
        json!({"type": "choice", "choice": "c2", "probabilities": {"c1": 0.1, "c2": 0.9}, "confidence": 0.5}),
    );
    assert_eq!(probable.picked(), Some(("c2".to_string(), 0.9)));
    let confident = answer(json!({"type": "choice", "choice": "c2", "confidence": 0.7}));
    assert_eq!(confident.picked(), Some(("c2".to_string(), 0.7)));
    let elsewhere = answer(
        json!({"type": "choice", "choice": "c2", "probabilities": {"c1": 0.4}, "confidence": 0.6}),
    );
    assert_eq!(elsewhere.picked(), Some(("c2".to_string(), 0.6)));
}

#[test]
fn a_pick_without_a_usable_key_or_probability_is_none() {
    assert_eq!(
        answer(json!({"type": "choice", "confidence": 0.7})).picked(),
        None
    );
    assert_eq!(
        answer(json!({"type": "choice", "choice": "", "confidence": 0.7})).picked(),
        None
    );
    assert_eq!(
        answer(json!({"type": "choice", "choice": "c1"})).picked(),
        None
    );
    assert_eq!(
        answer(json!({"type": "choice", "choice": "c1", "probabilities": {"c1": 1.5}})).picked(),
        None
    );
    assert_eq!(
        answer(json!({"type": "choice", "choice": "c1", "confidence": -1.0})).picked(),
        None
    );
}

#[test]
fn an_answer_keeps_every_field_it_reads_and_defaults_the_rest() {
    let full = answer(json!({
        "type": "choice", "noul": null, "choice": "a", "score": 3, "legend": {"a": "first"},
        "probabilities": {"a": 1.0}, "confidence": 0.9, "extra": true
    }));
    assert_eq!(full.kind, "choice");
    assert_eq!(full.score, Some(json!(3)));
    assert_eq!(full.legend, Some(json!({"a": "first"})));
    assert_eq!(full.probabilities, json!({"a": 1.0}));
    assert_eq!(answer(json!({})), Answer::default());
}

#[test]
fn a_reply_reads_its_answers_by_id() {
    let reply = reply_of(
        r#"{"model":"jev-1.14.0","answers":{"x":{"type":"noul","noul":0.7},"bad":[1,2]},"usage":{"input_tokens":9,"output_tokens":2}}"#,
    )
    .unwrap();
    assert_eq!(reply.model, "jev-1.14.0");
    assert_eq!(reply.answer("x").unwrap().yes(), Some(0.7));
    assert_eq!(reply.answer("missing"), None);
    assert_eq!(reply.answer("bad"), None);
    assert_eq!(
        reply.usage,
        Usage {
            input_tokens: 9,
            output_tokens: 2
        }
    );
}

#[test]
fn an_empty_reply_object_reads_as_no_answers() {
    let reply = reply_of("{}").unwrap();
    assert_eq!(reply, Reply::default());
    assert_eq!(reply.answer("ready"), None);
}

#[test]
fn a_reply_that_does_not_read_is_malformed() {
    for text in [
        "",
        "not json",
        "3",
        r#"{"answers": 3}"#,
        r#"{"usage": {"input_tokens": -1}}"#,
    ] {
        assert!(
            matches!(reply_of(text), Err(Failure::Malformed(why)) if !why.is_empty()),
            "{text}"
        );
    }
}

#[test]
fn billing_reads_each_header_by_its_lowercase_name() {
    let billing = billing_of(&|name| match name {
        "x-jev-run-id" => Some("run-42".into()),
        "x-jev-credits-charged" => Some(" 0.25 ".into()),
        "x-jev-tokens-remaining" => Some("998766".into()),
        "x-jev-credits-remaining" => Some("41.75".into()),
        _ => None,
    });
    assert_eq!(
        billing,
        Billing {
            run_id: Some("run-42".into()),
            credits_charged: Some(0.25),
            tokens_remaining: Some(998_766.0),
            credits_remaining: Some(41.75),
        }
    );
}

#[test]
fn billing_headers_that_do_not_read_are_left_out() {
    let billing = billing_of(&|name| match name {
        "x-jev-run-id" => Some("  ".into()),
        "x-jev-credits-charged" => Some("NaN".into()),
        "x-jev-tokens-remaining" => Some("inf".into()),
        "x-jev-credits-remaining" => Some("twelve".into()),
        _ => None,
    });
    assert_eq!(billing, Billing::default());
    assert_eq!(billing_of(&|_| None), Billing::default());
}

#[test]
fn each_status_maps_to_its_failure() {
    let error = r#"{"error":{"code":"invalid","message":"questions is empty"}}"#;
    assert_eq!(failure_of(401, None, error), Failure::BadKey);
    assert_eq!(failure_of(402, None, error), Failure::NoBalance);
    assert_eq!(
        failure_of(422, None, error),
        Failure::BadBody("invalid: questions is empty".into())
    );
    assert_eq!(
        failure_of(429, Some("30"), error),
        Failure::Limited(Some(30))
    );
    assert_eq!(failure_of(429, Some(" 7 "), ""), Failure::Limited(Some(7)));
    assert_eq!(failure_of(429, Some("soon"), ""), Failure::Limited(None));
    assert_eq!(failure_of(429, Some("-1"), ""), Failure::Limited(None));
    assert_eq!(failure_of(429, None, ""), Failure::Limited(None));
    assert_eq!(failure_of(502, None, ""), Failure::Unavailable(502));
    assert_eq!(failure_of(503, None, ""), Failure::Unavailable(503));
    assert_eq!(failure_of(504, None, error), Failure::Unknown);
    assert_eq!(
        failure_of(500, None, r#"{"error":{"message":"boom"}}"#),
        Failure::Other(500, "boom".into())
    );
    assert_eq!(
        failure_of(418, None, "nope"),
        Failure::Other(418, String::new())
    );
    assert_eq!(
        failure_of(200, None, "{}"),
        Failure::Other(200, String::new())
    );
}

#[test]
fn what_jev_said_is_its_code_its_message_or_both() {
    assert_eq!(
        failure_of(422, None, r#"{"error":{"code":"invalid"}}"#),
        Failure::BadBody("invalid".into())
    );
    assert_eq!(
        failure_of(422, None, r#"{"error":{"message":"too long"}}"#),
        Failure::BadBody("too long".into())
    );
    assert_eq!(
        failure_of(422, None, r#"{"error":{"code":"","message":""}}"#),
        Failure::BadBody(String::new())
    );
    assert_eq!(
        failure_of(422, None, r#"{"error":"flat"}"#),
        Failure::BadBody(String::new())
    );
    assert_eq!(failure_of(422, None, "{}"), Failure::BadBody(String::new()));
    assert_eq!(
        failure_of(422, None, "<html>"),
        Failure::BadBody(String::new())
    );
}

#[test]
fn what_jev_said_is_clipped_to_two_hundred_characters() {
    let long = "é".repeat(500);
    let text = json!({"error": {"message": long}}).to_string();
    let Failure::BadBody(said) = failure_of(422, None, &text) else {
        panic!("not a 422");
    };
    assert_eq!(said.chars().count(), 200);
    assert!(said.ends_with('…'));
    let fits = "a".repeat(200);
    let text = json!({"error": {"message": fits}}).to_string();
    assert_eq!(failure_of(422, None, &text), Failure::BadBody(fits));
}

#[test]
fn clip_keeps_short_text_and_cuts_long_text_on_a_character() {
    assert_eq!(clip("", 3), "");
    assert_eq!(clip("abc", 3), "abc");
    assert_eq!(clip("abcd", 3), "ab…");
    assert_eq!(clip("ééé", 2), "é…");
    assert_eq!(clip("ab", 0), "…");
}

#[test]
fn a_number_header_reads_only_a_finite_number() {
    assert_eq!(number(Some("1.5")), Some(1.5));
    assert_eq!(number(Some(" 2 ")), Some(2.0));
    assert_eq!(number(Some("-3")), Some(-3.0));
    assert_eq!(number(Some("NaN")), None);
    assert_eq!(number(Some("-inf")), None);
    assert_eq!(number(Some("")), None);
    assert_eq!(number(None), None);
}

#[test]
fn every_failure_says_itself_in_one_line() {
    for (failure, words) in [
        (Failure::BadKey, "401: Jev refused the key"),
        (Failure::NoBalance, "402: Jev's balance is spent"),
        (
            Failure::BadBody("x".into()),
            "422: Jev refused the body (x); never sent again unchanged",
        ),
        (
            Failure::Limited(Some(30)),
            "429: Jev is rate limiting; no call for 30 s",
        ),
        (Failure::Limited(None), "429: Jev is rate limiting"),
        (Failure::Unavailable(503), "503: Jev is unavailable"),
        (Failure::Unknown, "504: Jev's outcome is unknown"),
        (Failure::Deadline, "no answer within 2 s"),
        (
            Failure::Unreached("the connection was refused".into()),
            "Jev was not reached: the connection was refused",
        ),
        (
            Failure::Malformed("eof".into()),
            "Jev's answer did not read: eof",
        ),
        (Failure::Other(500, "boom".into()), "500: boom"),
        (Failure::Other(418, String::new()), "418: Jev answered 418"),
    ] {
        assert_eq!(failure.words(), words);
        assert_eq!(failure.to_string(), words);
        assert!(!words.contains('\n'));
    }
}

#[test]
fn a_failure_is_an_error() {
    let error: Box<dyn std::error::Error> = Box::new(Failure::BadKey);
    assert_eq!(error.to_string(), "401: Jev refused the key");
    assert!(error.source().is_none());
}

#[test]
fn only_a_limit_with_its_seconds_names_a_wait() {
    assert_eq!(Failure::Limited(Some(30)).retry_after_ms(), Some(30_000));
    assert_eq!(Failure::Limited(Some(0)).retry_after_ms(), Some(0));
    assert_eq!(
        Failure::Limited(Some(u64::MAX)).retry_after_ms(),
        Some(u64::MAX)
    );
    assert_eq!(Failure::Limited(None).retry_after_ms(), None);
    for other in [
        Failure::BadKey,
        Failure::NoBalance,
        Failure::BadBody(String::new()),
        Failure::Unavailable(502),
        Failure::Unknown,
        Failure::Deadline,
        Failure::Unreached(String::new()),
        Failure::Malformed(String::new()),
        Failure::Other(500, String::new()),
    ] {
        assert_eq!(other.retry_after_ms(), None, "{other:?}");
    }
}

#[test]
fn a_transport_error_reads_as_a_reason() {
    use std::io::{Error, ErrorKind};
    for (error, reason) in [
        (
            ureq::Error::Io(Error::from(ErrorKind::ConnectionRefused)),
            "the connection was refused",
        ),
        (
            ureq::Error::Io(Error::from(ErrorKind::UnexpectedEof)),
            "unexpected end of file",
        ),
        (ureq::Error::Timeout(ureq::Timeout::Global), "it timed out"),
        (
            ureq::Error::BodyExceedsLimit(MOST_REPLY),
            "the reply is over 1 MiB",
        ),
        (ureq::Error::HostNotFound, "the host wasn't found"),
        (ureq::Error::ConnectionFailed, "the connection failed"),
        (ureq::Error::TooManyRedirects, "the request failed"),
    ] {
        assert_eq!(http_reason(&error), reason);
    }
}

#[test]
fn the_live_client_aims_at_jev_with_a_two_second_deadline() {
    let live = Client::live();
    assert_eq!(live.url, "https://jev-ai.pro/api/v1/systemone");
    assert_eq!(live.deadline, Duration::from_secs(2));
    assert_eq!(Client::default(), live);
}

#[test]
fn the_caps_are_jevs() {
    assert_eq!(MODEL, "jev-1.14.0");
    assert_eq!(REQUEST_DEADLINE, Duration::from_secs(2));
    assert_eq!(MOST_BODY, 256_000);
    assert_eq!(MOST_QUESTIONS, 64);
    assert_eq!(MOST_REPLY, 1_048_576);
    assert_eq!(MOST_SAID, 200);
}
