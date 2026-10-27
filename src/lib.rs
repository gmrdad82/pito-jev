#![doc = include_str!("../README.md")]

use std::fmt;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{Map, Value, json};

pub const URL: &str = "https://jev-ai.pro/api/v1/systemone";
pub const MODEL: &str = "jev-1.14.0";
pub const REQUEST_DEADLINE: Duration = Duration::from_secs(2);
pub const MOST_BODY: usize = 256_000;
pub const MOST_QUESTIONS: usize = 64;
pub const MOST_REPLY: u64 = 1 << 20;
const MOST_SAID: usize = 200;

#[derive(Clone, PartialEq, Eq)]
pub struct Key(String);

impl Key {
    pub fn new(value: &str) -> Option<Key> {
        let value = value.trim();
        (!value.is_empty() && !value.chars().any(char::is_whitespace))
            .then(|| Key(value.to_string()))
    }

    fn bearer(&self) -> String {
        format!("Bearer {}", self.0)
    }
}

impl fmt::Debug for Key {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Key(hidden)")
    }
}

impl fmt::Display for Key {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("hidden")
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Question {
    pub instructions: String,
    pub yes: Option<String>,
    pub no: Option<String>,
    pub options: Vec<(String, Option<String>)>,
}

impl Question {
    fn json(&self) -> Value {
        if !self.options.is_empty() {
            let options: Map<String, Value> = self
                .options
                .iter()
                .map(|(key, description)| (key.clone(), json!(description)))
                .collect();
            return json!({"type": "choice", "instructions": self.instructions, "criteria": options});
        }
        let mut criteria = Map::new();
        if let Some(yes) = &self.yes {
            criteria.insert("true".into(), json!(yes));
        }
        if let Some(no) = &self.no {
            criteria.insert("false".into(), json!(no));
        }
        let mut question = json!({"type": "noul", "instructions": self.instructions});
        if !criteria.is_empty() {
            question["criteria"] = Value::Object(criteria);
        }
        question
    }
}

pub fn body(model: &str, state: &Value, questions: &[(&str, Question)]) -> Result<String, String> {
    if questions.is_empty() || questions.len() > MOST_QUESTIONS {
        return Err(format!(
            "a call takes 1 to {MOST_QUESTIONS} questions, not {}",
            questions.len()
        ));
    }
    let mut asked = Map::new();
    for (id, question) in questions {
        if asked.insert((*id).to_string(), question.json()).is_some() {
            return Err(format!("the question id {id} is given twice"));
        }
    }
    let text = json!({"model": model, "state": state, "questions": asked}).to_string();
    if text.len() > MOST_BODY {
        return Err(format!(
            "the body is {} bytes, over Jev's {MOST_BODY}",
            text.len()
        ));
    }
    Ok(text)
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Answer {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub noul: Option<f64>,
    #[serde(default)]
    pub choice: Option<String>,
    #[serde(default)]
    pub score: Option<Value>,
    #[serde(default)]
    pub legend: Option<Value>,
    #[serde(default)]
    pub probabilities: Value,
    #[serde(default)]
    pub confidence: Option<f64>,
}

impl Answer {
    pub fn yes(&self) -> Option<f64> {
        self.noul.filter(|p| (0.0..=1.0).contains(p))
    }

    pub fn picked(&self) -> Option<(String, f64)> {
        let key = self.choice.clone().filter(|key| !key.is_empty())?;
        let p = self.probabilities[key.as_str()]
            .as_f64()
            .or(self.confidence)
            .filter(|p| (0.0..=1.0).contains(p))?;
        Some((key, p))
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Reply {
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub answers: Map<String, Value>,
    #[serde(default)]
    pub usage: Usage,
}

impl Reply {
    pub fn answer(&self, id: &str) -> Option<Answer> {
        serde_json::from_value(self.answers.get(id)?.clone()).ok()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Billing {
    pub run_id: Option<String>,
    pub credits_charged: Option<f64>,
    pub tokens_remaining: Option<f64>,
    pub credits_remaining: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Called {
    pub reply: Reply,
    pub billing: Billing,
    pub latency_ms: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Failure {
    BadKey,
    NoBalance,
    BadBody(String),
    Limited(Option<u64>),
    Unavailable(u16),
    Unknown,
    Deadline,
    Unreached(String),
    Malformed(String),
    Other(u16, String),
}

impl Failure {
    pub fn words(&self) -> String {
        match self {
            Failure::BadKey => "401: Jev refused the key".into(),
            Failure::NoBalance => "402: Jev's balance is spent".into(),
            Failure::BadBody(said) => {
                format!("422: Jev refused the body ({said}); never sent again unchanged")
            }
            Failure::Limited(Some(secs)) => {
                format!("429: Jev is rate limiting; no call for {secs} s")
            }
            Failure::Limited(None) => "429: Jev is rate limiting".into(),
            Failure::Unavailable(code) => format!("{code}: Jev is unavailable"),
            Failure::Unknown => "504: Jev's outcome is unknown".into(),
            Failure::Deadline => format!("no answer within {} s", REQUEST_DEADLINE.as_secs()),
            Failure::Unreached(why) => format!("Jev was not reached: {why}"),
            Failure::Malformed(why) => format!("Jev's answer did not read: {why}"),
            Failure::Other(code, said) if said.is_empty() => format!("{code}: Jev answered {code}"),
            Failure::Other(code, said) => format!("{code}: {said}"),
        }
    }

    pub fn retry_after_ms(&self) -> Option<u64> {
        match self {
            Failure::Limited(Some(secs)) => Some(secs.saturating_mul(1000)),
            _ => None,
        }
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.words())
    }
}

impl std::error::Error for Failure {}

fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(max.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

pub fn failure_of(status: u16, retry_after: Option<&str>, body: &str) -> Failure {
    let said = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            let error = &value["error"];
            let code = error["code"].as_str().unwrap_or_default();
            let message = error["message"].as_str().unwrap_or_default();
            match (code.is_empty(), message.is_empty()) {
                (true, true) => None,
                (false, true) => Some(code.to_string()),
                (true, false) => Some(message.to_string()),
                (false, false) => Some(format!("{code}: {message}")),
            }
        })
        .map(|said| clip(&said, MOST_SAID))
        .unwrap_or_default();
    match status {
        401 => Failure::BadKey,
        402 => Failure::NoBalance,
        422 => Failure::BadBody(said),
        429 => Failure::Limited(retry_after.and_then(|secs| secs.trim().parse().ok())),
        502 | 503 => Failure::Unavailable(status),
        504 => Failure::Unknown,
        _ => Failure::Other(status, said),
    }
}

fn number(value: Option<&str>) -> Option<f64> {
    value
        .and_then(|text| text.trim().parse::<f64>().ok())
        .filter(|n| n.is_finite())
}

pub fn billing_of(header: &dyn Fn(&str) -> Option<String>) -> Billing {
    Billing {
        run_id: header("x-jev-run-id").filter(|id| !id.trim().is_empty()),
        credits_charged: number(header("x-jev-credits-charged").as_deref()),
        tokens_remaining: number(header("x-jev-tokens-remaining").as_deref()),
        credits_remaining: number(header("x-jev-credits-remaining").as_deref()),
    }
}

pub fn reply_of(body: &str) -> Result<Reply, Failure> {
    serde_json::from_str(body).map_err(|e| Failure::Malformed(e.to_string()))
}

fn http_reason(error: &ureq::Error) -> String {
    match error {
        ureq::Error::Io(io) if io.kind() == std::io::ErrorKind::ConnectionRefused => {
            "the connection was refused".into()
        }
        ureq::Error::Io(io) => io.kind().to_string(),
        ureq::Error::Timeout(_) => "it timed out".into(),
        ureq::Error::BodyExceedsLimit(_) => "the reply is over 1 MiB".into(),
        ureq::Error::HostNotFound => "the host wasn't found".into(),
        ureq::Error::ConnectionFailed => "the connection failed".into(),
        _ => "the request failed".into(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Client {
    pub url: String,
    pub deadline: Duration,
}

impl Default for Client {
    fn default() -> Client {
        Client::live()
    }
}

impl Client {
    pub fn live() -> Client {
        Client {
            url: URL.into(),
            deadline: REQUEST_DEADLINE,
        }
    }

    pub fn ask(&self, key: &Key, body: &str) -> Result<Called, Failure> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(self.deadline))
            .http_status_as_error(false)
            .build()
            .into();
        let began = Instant::now();
        let sent = agent
            .post(&self.url)
            .header("Authorization", &key.bearer())
            .header("Content-Type", "application/json")
            .send(body)
            .map_err(|e| match e {
                ureq::Error::Timeout(_) => Failure::Deadline,
                other => Failure::Unreached(http_reason(&other)),
            })?;
        let status = sent.status().as_u16();
        let headers = sent.headers().clone();
        let header = |name: &str| {
            headers
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string)
        };
        let text = sent
            .into_body()
            .with_config()
            .limit(MOST_REPLY)
            .read_to_string()
            .map_err(|e| match e {
                ureq::Error::Timeout(_) => Failure::Deadline,
                other => Failure::Malformed(http_reason(&other)),
            })?;
        let latency_ms = u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX);
        if !(200..300).contains(&status) {
            return Err(failure_of(status, header("retry-after").as_deref(), &text));
        }
        Ok(Called {
            reply: reply_of(&text)?,
            billing: billing_of(&header),
            latency_ms,
        })
    }
}

#[cfg(test)]
mod tests;
