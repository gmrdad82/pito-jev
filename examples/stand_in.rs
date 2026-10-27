use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Map, Value, json};

static RUNS: AtomicU64 = AtomicU64::new(0);

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [port, key] = args.as_slice() else {
        eprintln!("usage: stand_in <PORT> <KEY>");
        return ExitCode::from(2);
    };
    let listener = match TcpListener::bind(("127.0.0.1", port.parse().unwrap_or(0))) {
        Ok(listener) => listener,
        Err(err) => {
            eprintln!("stand_in: {err}");
            return ExitCode::FAILURE;
        }
    };
    for stream in listener.incoming().flatten() {
        let _ = handle(stream, key);
    }
    ExitCode::SUCCESS
}

fn handle(mut stream: TcpStream, key: &str) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut length = 0;
    let mut bearer = String::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().unwrap_or(0);
            } else if name.eq_ignore_ascii_case("authorization") {
                bearer = value.trim().to_string();
            }
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    let (status, headers, reply) = if bearer != format!("Bearer {key}") {
        (
            "401 Unauthorized",
            String::new(),
            json!({"error": {"code": "unauthorized", "message": "the key is not known"}}),
        )
    } else {
        match serde_json::from_slice::<Value>(&body) {
            Ok(asked) => answered(&asked, body.len()),
            Err(_) => (
                "422 Unprocessable Content",
                String::new(),
                json!({"error": {"code": "invalid", "message": "the body is not JSON"}}),
            ),
        }
    };
    let text = reply.to_string();
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
        text.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(text.as_bytes())
}

fn answered(asked: &Value, bytes: usize) -> (&'static str, String, Value) {
    let questions = asked["questions"].as_object().cloned().unwrap_or_default();
    let mut answers = Map::new();
    for (id, question) in &questions {
        let instructions = question["instructions"].as_str().unwrap_or_default();
        let answer = match question["criteria"].as_object() {
            Some(criteria) if question["type"] == "choice" && !criteria.is_empty() => {
                let weights: Vec<(String, f64)> = criteria
                    .keys()
                    .map(|option| {
                        (
                            option.clone(),
                            1.0 + share(&format!("{instructions}{option}")),
                        )
                    })
                    .collect();
                let total: f64 = weights.iter().map(|(_, w)| w).sum();
                let probabilities: Map<String, Value> = weights
                    .iter()
                    .map(|(option, w)| (option.clone(), json!(round(w / total))))
                    .collect();
                let (pick, w) =
                    weights.iter().fold(
                        &weights[0],
                        |best, next| if next.1 > best.1 { next } else { best },
                    );
                json!({"type": "choice", "choice": pick, "probabilities": probabilities, "confidence": round(w / total)})
            }
            _ => {
                let p = round(0.05 + 0.9 * share(instructions));
                json!({"type": "noul", "noul": p, "probabilities": {"true": p, "false": round(1.0 - p)}, "confidence": round(0.6 + 0.35 * share(&format!("{instructions}!")))})
            }
        };
        answers.insert(id.clone(), answer);
    }
    let run = RUNS.fetch_add(1, Ordering::Relaxed) + 1;
    let charged = 0.25 * questions.len() as f64;
    let headers = format!(
        "X-Jev-Run-Id: run-{run:04}\r\nX-Jev-Credits-Charged: {charged}\r\nX-Jev-Credits-Remaining: {}\r\n",
        50.0 - charged * run as f64
    );
    let reply = json!({
        "model": asked["model"],
        "answers": answers,
        "usage": {"input_tokens": bytes / 4, "output_tokens": questions.len()},
    });
    ("200 OK", headers, reply)
}

fn share(text: &str) -> f64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    (hash % 10_000) as f64 / 10_000.0
}

fn round(p: f64) -> f64 {
    (p * 100.0).round() / 100.0
}
