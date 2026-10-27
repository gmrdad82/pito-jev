use std::process::ExitCode;

use pito_jev::{Client, Key, MODEL, Question, body};
use serde_json::json;

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut client = Client::live();
    if args.first().map(String::as_str) == Some("--url") {
        if args.len() < 2 {
            return usage();
        }
        client.url = args.remove(1);
        args.remove(0);
    }
    let [state, instructions, options @ ..] = args.as_slice() else {
        return usage();
    };
    let Some(key) = std::env::var("JEV_API_KEY")
        .ok()
        .as_deref()
        .and_then(Key::new)
    else {
        eprintln!("ask: set JEV_API_KEY to your Jev API key");
        return ExitCode::from(2);
    };
    let question = Question {
        instructions: instructions.clone(),
        options: options
            .iter()
            .map(|option| match option.split_once('=') {
                Some((key, description)) => (key.to_string(), Some(description.to_string())),
                None => (option.clone(), None),
            })
            .collect(),
        ..Question::default()
    };
    let text = match body(MODEL, &json!(state), &[("q", question)]) {
        Ok(text) => text,
        Err(why) => {
            eprintln!("ask: {why}");
            return ExitCode::from(2);
        }
    };
    let called = match client.ask(&key, &text) {
        Ok(called) => called,
        Err(failure) => {
            eprintln!("ask: {failure}");
            return ExitCode::FAILURE;
        }
    };
    let Some(answer) = called.reply.answer("q") else {
        eprintln!("ask: Jev's reply holds no answer");
        return ExitCode::FAILURE;
    };
    let confidence = answer
        .confidence
        .map(|c| format!("  confidence {c:.2}"))
        .unwrap_or_default();
    match (answer.yes(), answer.picked()) {
        (Some(p), _) => println!("yes {p:.2}{confidence}"),
        (None, Some((pick, p))) => println!("picked {pick} {p:.2}{confidence}"),
        (None, None) => println!("no usable answer ({})", answer.kind),
    }
    let billing = &called.billing;
    let money = match (billing.credits_charged, billing.credits_remaining) {
        (Some(charged), Some(left)) => format!(" · {charged} credits, {left} left"),
        (Some(charged), None) => format!(" · {charged} credits"),
        _ => String::new(),
    };
    println!(
        "{} · {} tokens in, {} out{money} · {} ms",
        called.reply.model,
        called.reply.usage.input_tokens,
        called.reply.usage.output_tokens,
        called.latency_ms
    );
    ExitCode::SUCCESS
}

fn usage() -> ExitCode {
    eprintln!("usage: ask [--url URL] <STATE> <QUESTION> [OPTION[=DESCRIPTION]...]");
    ExitCode::from(2)
}
