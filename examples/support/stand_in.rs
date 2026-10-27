use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde_json::{Map, Value, json};

const MOST_BODY: usize = 256_000;
const MOST_QUESTIONS: usize = 64;
const MOST_REPLY: usize = 1 << 20;
const SLOW: Duration = Duration::from_millis(2_500);
const HEADER_ROOM: usize = 64 * 1024;

pub fn start(port: u16, key: &str) -> std::io::Result<SocketAddr> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    let address = listener.local_addr()?;
    let key = Arc::new(key.to_string());
    let runs = Arc::new(AtomicU64::new(0));
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let key = key.clone();
            let runs = runs.clone();
            std::thread::spawn(move || {
                let _ = handle(stream, &key, &runs);
            });
        }
    });
    Ok(address)
}

struct Asked {
    path: String,
    bearer: String,
    body: Vec<u8>,
}

fn read(stream: &TcpStream) -> std::io::Result<Asked> {
    let mut reader = BufReader::new(
        stream
            .try_clone()?
            .take((HEADER_ROOM + MOST_BODY * 2) as u64),
    );
    let mut first = String::new();
    reader.read_line(&mut first)?;
    let path = first
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_string();
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
    let mut body = vec![0; length.min(MOST_BODY * 2)];
    reader.read_exact(&mut body)?;
    Ok(Asked { path, bearer, body })
}

fn handle(mut stream: TcpStream, key: &str, runs: &AtomicU64) -> std::io::Result<()> {
    let asked = read(&stream)?;
    let route = asked
        .path
        .strip_prefix("/api/v1/systemone")
        .map(|rest| rest.trim_start_matches('/'));
    let (status, headers, text) = if asked.bearer != format!("Bearer {key}") {
        refused(401, "unauthorized", "the key is not known")
    } else {
        match route {
            Some("") => answered(&asked.body, runs),
            Some("slow") => {
                std::thread::sleep(SLOW);
                answered(&asked.body, runs)
            }
            Some("garbled") => (200, String::new(), "<html>not Jev's JSON</html>".into()),
            Some("huge") => {
                let pad = "x".repeat(MOST_REPLY);
                (
                    200,
                    String::new(),
                    json!({"model": "stand-in", "answers": {}, "pad": pad}).to_string(),
                )
            }
            Some(rest) => match rest
                .strip_prefix("status/")
                .and_then(|code| code.parse().ok())
            {
                Some(code @ 400..=599) => refused(code, "stand_in", "a status asked for by path"),
                _ => refused(404, "not_found", "no such route on the stand-in"),
            },
            None => refused(404, "not_found", "no such route on the stand-in"),
        }
    };
    let head = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: application/json\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
        reason(status),
        text.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(text.as_bytes())
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        401 => "Unauthorized",
        402 => "Payment Required",
        404 => "Not Found",
        422 => "Unprocessable Content",
        429 => "Too Many Requests",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "Stand-in",
    }
}

fn refused(status: u16, code: &str, message: &str) -> (u16, String, String) {
    let headers = if status == 429 {
        "Retry-After: 30\r\n".to_string()
    } else {
        String::new()
    };
    let text = json!({"error": {"code": code, "message": message}}).to_string();
    (status, headers, text)
}

fn answered(body: &[u8], runs: &AtomicU64) -> (u16, String, String) {
    if body.len() > MOST_BODY {
        return refused(422, "too_large", "the body is over 256000 bytes");
    }
    let Ok(asked) = serde_json::from_slice::<Value>(body) else {
        return refused(422, "invalid", "the body is not JSON");
    };
    let questions = asked["questions"].as_object().cloned().unwrap_or_default();
    if questions.is_empty() || questions.len() > MOST_QUESTIONS {
        return refused(422, "invalid", "a call takes 1 to 64 questions");
    }
    let mut answers = Map::new();
    for (id, question) in &questions {
        answers.insert(id.clone(), answer(question));
    }
    let run = runs.fetch_add(1, Ordering::Relaxed) + 1;
    let charged = 0.25 * questions.len() as f64;
    let headers = format!(
        "X-Jev-Run-Id: run-{run:04}\r\nX-Jev-Credits-Charged: {charged}\r\nX-Jev-Tokens-Remaining: {}\r\nX-Jev-Credits-Remaining: {}\r\n",
        1_000_000_usize.saturating_sub(body.len() / 4 * run as usize),
        50.0 - charged * run as f64
    );
    let reply = json!({
        "model": asked["model"],
        "answers": answers,
        "usage": {"input_tokens": body.len() / 4, "output_tokens": questions.len()},
    });
    (200, headers, reply.to_string())
}

fn answer(question: &Value) -> Value {
    let instructions = question["instructions"].as_str().unwrap_or_default();
    match question["criteria"].as_object() {
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
                    |best, next| {
                        if next.1 > best.1 { next } else { best }
                    },
                );
            json!({"type": "choice", "choice": pick, "probabilities": probabilities, "confidence": round(w / total)})
        }
        _ => {
            let p = round(0.05 + 0.9 * share(instructions));
            json!({"type": "noul", "noul": p, "probabilities": {"true": p, "false": round(1.0 - p)}, "confidence": round(0.6 + 0.35 * share(&format!("{instructions}!")))})
        }
    }
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
