#[path = "support/stand_in.rs"]
mod stand_in;

use pito_jev::{Client, Key, MODEL, MOST_QUESTIONS, Question, body};
use serde_json::json;

fn yes_no(instructions: &str, yes: &str) -> Question {
    Question {
        instructions: instructions.into(),
        yes: Some(yes.into()),
        ..Question::default()
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let address = stand_in::start(0, "demo-key")?;
    let client = Client {
        url: format!("http://{address}/api/v1/systemone"),
        ..Client::live()
    };
    let key = Key::new("demo-key").ok_or("the key is one word")?;
    let state = json!({
        "inbox": 214,
        "unread": 37,
        "oldest_unread_days": 9,
        "flagged": ["invoice 1182", "contract renewal"],
    });
    let questions = [
        (
            "urgent",
            yes_no(
                "Does anything need an answer today?",
                "a flagged mail is due today",
            ),
        ),
        (
            "triage",
            yes_no(
                "Should the inbox be triaged now?",
                "more than 30 are unread",
            ),
        ),
        (
            "first",
            Question {
                instructions: "Which flagged mail comes first?".into(),
                options: vec![
                    ("invoice".into(), Some("invoice 1182".into())),
                    ("contract".into(), Some("contract renewal".into())),
                ],
                ..Question::default()
            },
        ),
    ];
    let text = body(MODEL, &state, &questions)?;
    let called = client.ask(&key, &text)?;
    for (id, _) in &questions {
        let Some(answer) = called.reply.answer(id) else {
            println!("{id:<7} no answer");
            continue;
        };
        match (answer.yes(), answer.picked()) {
            (Some(p), _) => println!("{id:<7} yes {p:.2}"),
            (None, Some((pick, p))) => println!("{id:<7} picked {pick} {p:.2}"),
            (None, None) => println!("{id:<7} no usable answer"),
        }
    }
    let usage = &called.reply.usage;
    println!(
        "one call, {} questions: {} tokens in, {} out, {} ms",
        questions.len(),
        usage.input_tokens,
        usage.output_tokens,
        called.latency_ms
    );
    let billing = &called.billing;
    println!(
        "run {}: {} credits charged, {} left",
        billing.run_id.as_deref().unwrap_or("unnamed"),
        billing
            .credits_charged
            .map_or("?".into(), |c| c.to_string()),
        billing
            .credits_remaining
            .map_or("?".into(), |c| c.to_string())
    );
    let names: Vec<String> = (0..=MOST_QUESTIONS).map(|n| format!("q{n}")).collect();
    let too_many: Vec<(&str, Question)> = names
        .iter()
        .map(|name| (name.as_str(), yes_no("?", "yes")))
        .collect();
    if let Err(why) = body(MODEL, &state, &too_many) {
        println!("65 questions are refused before a call: {why}");
    }
    println!("the stand-in's numbers are made up, not Jev's");
    Ok(())
}
