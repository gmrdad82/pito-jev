#[path = "support/stand_in.rs"]
mod stand_in;

use std::net::TcpListener;

use pito_jev::{Client, Failure, Key, MODEL, Question, body};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let address = stand_in::start(0, "demo-key")?;
    let base = format!("http://{address}/api/v1/systemone");
    let closed = TcpListener::bind("127.0.0.1:0")?.local_addr()?;
    let key = Key::new("demo-key").ok_or("the key is one word")?;
    let wrong = Key::new("wrong-key").ok_or("the key is one word")?;
    let question = Question {
        instructions: "Is it raining?".into(),
        ..Question::default()
    };
    let text = body(MODEL, &json!("a calm day"), &[("rain", question)])?;
    let cases = [
        ("a refused key", base.clone(), &wrong),
        ("a spent balance", format!("{base}/status/402"), &key),
        ("a refused body", format!("{base}/status/422"), &key),
        ("a rate limit", format!("{base}/status/429"), &key),
        ("Jev unavailable", format!("{base}/status/503"), &key),
        ("an unknown outcome", format!("{base}/status/504"), &key),
        ("a slow answer", format!("{base}/slow"), &key),
        ("a reply that isn't JSON", format!("{base}/garbled"), &key),
        ("a reply over 1 MiB", format!("{base}/huge"), &key),
        (
            "a closed port",
            format!("http://{closed}/api/v1/systemone"),
            &key,
        ),
    ];
    for (case, url, key) in cases {
        let client = Client {
            url,
            ..Client::live()
        };
        let what = match client.ask(key, &text) {
            Ok(called) => format!("answered in {} ms", called.latency_ms),
            Err(failure) => format!("{failure}\n    {}", advice(&failure)),
        };
        println!("{case}: {what}");
    }
    Ok(())
}

fn advice(failure: &Failure) -> String {
    match failure {
        Failure::BadKey => "ask for a new key; trying again won't help".into(),
        Failure::NoBalance => "top up the balance, then ask again".into(),
        Failure::BadBody(_) => "fix the body; the same body is refused again".into(),
        Failure::Limited(_) => match failure.retry_after_ms() {
            Some(ms) => format!("wait {ms} ms before the next call"),
            None => "wait a while before the next call".into(),
        },
        Failure::Unavailable(_) => "try again later".into(),
        Failure::Unknown => "it may have been charged; check before asking again".into(),
        Failure::Deadline => "it may have been charged; ask again only if that's fine".into(),
        Failure::Unreached(_) => "check the network and the URL".into(),
        Failure::Malformed(_) => "the answer can't be trusted; report the reply".into(),
        Failure::Other(code, _) => format!("{code} was not expected; report it"),
    }
}
