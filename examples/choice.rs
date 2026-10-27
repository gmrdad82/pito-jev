#[path = "support/stand_in.rs"]
mod stand_in;

use pito_jev::{Client, Key, MODEL, Question, body};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let address = stand_in::start(0, "demo-key")?;
    let client = Client {
        url: format!("http://{address}/api/v1/systemone"),
        ..Client::live()
    };
    let key = Key::new("demo-key").ok_or("the key is one word")?;
    let track = Question {
        instructions: "Which track should this train take?".into(),
        options: vec![
            ("fast".into(), Some("the short gate, in a minute".into())),
            ("full".into(), Some("every suite, in ten".into())),
            ("hold".into(), None),
        ],
        ..Question::default()
    };
    let text = body(
        MODEL,
        &json!({"train": 41, "branches": 6, "touched": ["src/jev.rs", "README.md"]}),
        &[("track", track)],
    )?;
    let called = client.ask(&key, &text)?;
    let answer = called.reply.answer("track").ok_or("no answer for track")?;
    match answer.picked() {
        Some((pick, p)) => println!("picked {pick} with p = {p:.2}"),
        None => println!("no usable pick ({})", answer.kind),
    }
    if let Some(options) = answer.probabilities.as_object() {
        for (option, p) in options {
            println!("  {option:<5} {:.2}", p.as_f64().unwrap_or_default());
        }
    }
    println!("the stand-in's numbers are made up, not Jev's");
    Ok(())
}
