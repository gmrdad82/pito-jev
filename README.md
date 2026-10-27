<p align="center"><img src="docs/images/ask.gif" width="800" alt="A terminal: the ask example puts a yes/no question to a local stand-in for Jev and prints yes 0.14 with its confidence, the model, the tokens and the credits; then a choice question between fast and full prints picked full 0.55; then, with a wrong key, it prints 401: Jev refused the key."></p>

# pito-jev

[![Version](https://img.shields.io/github/v/tag/gmrdad82/pito-jev)](https://github.com/gmrdad82/pito-jev/tags)

**A small blocking Rust client for Jev's systemone API.** Describe a state, ask
up to 64 yes/no or choice questions about it in one call, and get each answer
back as a probability, with the tokens used and the credits charged. One call
is one request, held to a 2 s deadline, never retried, and every way it can go
wrong is a typed `Failure`. It's part of [PITO MD](https://pitomd.com) and is
not made by or affiliated with Jev.

- **359 lines of Rust** in one file, `src/lib.rs`, with 30 unit tests beside
  it, 28 tests against local servers and 12 doc tests; 100% of its lines are
  covered.
- **3 direct dependencies** (ureq with rustls, serde, serde_json), 36 crates in
  all, no OpenSSL or other system library.
- **Bounded on every side:** at most 2 s a call, 64 questions, a body of
  256,000 bytes and a reply of 1 MiB.
- **0.09 ms a call** against the local stand-in, so the time a call takes is
  Jev's and the network's, not the library's.

Tested on Linux; written for Windows and macOS too, untested there.

Got questions or doubts? Ping me on Discord: https://discord.gg/XsNcn3kYzn

## Install and first use

pito-jev isn't on crates.io. Add it from this repository, pinned to a
[tag](https://github.com/gmrdad82/pito-jev/tags); the lines are the same on
Linux, Windows and macOS, with Rust 1.97 or newer installed:

```sh
cargo add pito-jev --git https://github.com/gmrdad82/pito-jev --tag v0.1.1
cargo add serde_json
```

or by hand in `Cargo.toml`:

```toml
[dependencies]
pito-jev = { git = "https://github.com/gmrdad82/pito-jev", tag = "v0.1.1" }
serde_json = "1"
```

The package is `pito-jev` and the code says `use pito_jev::…`. This asks Jev
one yes/no question, with your key in `JEV_API_KEY`:

```rust,no_run
use pito_jev::{Client, Key, MODEL, Question, body};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let key = Key::new(&std::env::var("JEV_API_KEY")?).ok_or("JEV_API_KEY is not one word")?;
    let ready = Question {
        instructions: "Is this train ready to merge?".into(),
        yes: Some("every branch is reviewed and the gate is green".into()),
        ..Question::default()
    };
    let text = body(MODEL, &json!("train 41: 6 branches, gate green"), &[("ready", ready)])?;
    let called = Client::live().ask(&key, &text)?;
    if let Some(p) = called.reply.answer("ready").and_then(|answer| answer.yes()) {
        println!("ready to merge: p = {p:.2}");
    }
    Ok(())
}
```

`yes()` is the probability, from 0 to 1, that the answer is yes.

To watch it work without a Jev account, run the examples from a checkout. Each
of `choice`, `many` and `failures` starts its own stand-in on a free local
port, which answers in Jev's shape with made-up numbers; none of them reaches
Jev:

```sh
git clone https://github.com/gmrdad82/pito-jev
cd pito-jev
cargo run --release --example choice
cargo run --release --example many
cargo run --release --example failures
```

The clip above is the `ask` example against the `stand_in` example, its numbers
made up too:

```sh
cargo build --release --examples
target/release/examples/stand_in 8472 demo-key &
export JEV_API_KEY=demo-key
target/release/examples/ask --url http://127.0.0.1:8472/api/v1/systemone 'a calm day' 'Is it raining?'
target/release/examples/ask --url http://127.0.0.1:8472/api/v1/systemone 'train 41' 'Which track?' fast='the short gate' full='every suite'
kill %1
```

Leave out `--url` and set `JEV_API_KEY` to a real key to ask Jev itself.

## Speed and bounds

| A call against the local stand-in | Median | p99 | Worst |
| --- | --- | --- | --- |
| One yes/no question (a 183-byte body) | 0.094 ms | 0.163 ms | 0.614 ms |
| 64 questions (a 6,597-byte body) | 0.354 ms | 0.424 ms | 0.751 ms |
| Building the 64-question body alone | 0.061 ms | 0.066 ms | 0.177 ms |

How it was measured, once, on 2026-10-27: a release build (rustc 1.97.1) made
1,000 calls of each kind, after 50 to warm up, to the stand-in in
`examples/support/stand_in.rs` on the same machine, each call a fresh
connection over loopback, and built the body 10,000 times; an AMD Ryzen 9
9950X on Linux, busy with other work (load average about 10 on 32 threads).
Not measured on macOS or Windows. A real call adds Jev's time and the
network's, and the deadline holds the whole call to 2 s.

| Bound | Value | What happens past it |
| --- | --- | --- |
| Questions in a call | 1 to 64 (`MOST_QUESTIONS`) | `body` refuses, nothing is sent |
| Body | 256,000 bytes (`MOST_BODY`) | `body` refuses, nothing is sent |
| Reply | 1 MiB (`MOST_REPLY`) | the read stops there: `Failure::Malformed("the reply is over 1 MiB")` |
| A call, connecting and reading included | 2 s (`REQUEST_DEADLINE`) | `Failure::Deadline` |
| What Jev said in a refusal | 200 characters | cut, ending in `…` |
| Tries a call makes | 1 | never retried, so never charged twice |

## Features

### Yes/no and choice questions

A `Question` is a yes/no question (Jev's `noul` type) unless it has `options`.
A yes/no question says what `yes` and `no` mean when that helps; a choice
lists each option's key with an optional description. The answer's `yes()` is
the probability of yes, and `picked()` is the chosen option with its
probability, or with Jev's confidence when the reply gives no probability for
it. A probability outside 0 to 1 is never handed back.

### Many questions in one call

`body` puts up to 64 questions about one state into one call, under their
ids, in the order given, and refuses an id given twice. The state is any JSON
value: a sentence, a record, a list. The reply holds each answer by its id, so
one call, one charge and one wait cover them all.

### One bounded call

`Client::ask` posts once, with one deadline over the whole call, and reads at
most 1 MiB of reply. It never retries: a call that might have been charged is
never charged twice, and whether to ask again is the caller's choice.
`retry_after_ms()` says how long Jev asked to wait.

### Typed answers, tokens and credits

A call that succeeds returns `Called`: the `reply` (the model, the answers,
the input and output tokens), the `billing` read from Jev's `X-Jev-*` headers
(the run id, the credits charged, and the tokens and credits left; a header
that doesn't read as a finite number is left out), and the `latency_ms`.

### Typed failures

Every failure is a `Failure`, and its words say it in one line that never
holds the key:

| Failure | When |
| --- | --- |
| `BadKey` | 401: Jev refused the key |
| `NoBalance` | 402: the balance is spent |
| `BadBody(said)` | 422: Jev refused the body, with what it said; never send it again unchanged |
| `Limited(secs)` | 429: rate limited, with the `Retry-After` seconds when Jev gives them |
| `Unavailable(code)` | 502 or 503 |
| `Unknown` | 504: the call's outcome is unknown |
| `Deadline` | no answer within the deadline |
| `Unreached(why)` | the host wasn't found, the URL didn't parse or the connection failed |
| `Malformed(why)` | the reply isn't Jev's JSON, stopped short, or is over 1 MiB |
| `Other(code, said)` | any other status |

### A key that can't leak

The library never looks for a key by itself: it reads no environment
variable, file or keyring, and takes the `Key` you give it. A `Key` prints as
`hidden` in `Debug` and `Display`, so it can't reach a log, and it travels
only in the `Authorization` header, never in the body or a failure's words.

## Everything it does

- **Builds the request.** `body(model, &state, &questions)` writes
  `{"model": …, "state": …, "questions": {id: question, …}}` as one JSON
  string: a yes/no question as `{"type": "noul", "instructions": …}` with
  `criteria` `{"true": yes, "false": no}` holding only the meanings given, a
  choice as `{"type": "choice", "instructions": …, "criteria": {option:
  description or null, …}}`, the options and the questions in the order
  given. `MODEL` is `jev-1.14.0`.
- **Refuses a bad request before it is sent:** no questions, more than 64, an
  id given twice, or a body over 256,000 bytes, each with a one-line reason.
- **Sends it once.** `Client::live()` posts to
  `https://jev-ai.pro/api/v1/systemone` with `Authorization: Bearer <key>`
  and `Content-Type: application/json`; `Client { url, deadline }` points it
  at any other URL, such as a test server, with its own deadline.
- **Holds the call to its deadline,** connecting, sending, waiting and reading
  included; a reply that stops halfway runs into it too.
- **Reads at most 1 MiB of reply** and refuses anything longer as malformed,
  so a broken server can't fill memory.
- **Reads the answer.** `Reply::answer(id)` reads one answer, or `None` when
  it is missing or not an answer's shape. `Answer` keeps every field Jev
  sends (`type`, `noul`, `choice`, `score`, `legend`, `probabilities`,
  `confidence`), and fields it doesn't send default; unknown fields are
  ignored.
- **Reads the cost.** `Usage` holds the input and output tokens; `Billing`
  holds the `X-Jev-Run-Id`, `X-Jev-Credits-Charged`, `X-Jev-Tokens-Remaining`
  and `X-Jev-Credits-Remaining` headers, each left out when blank or not a
  finite number; `latency_ms` is the whole call's time.
- **Names every failure.** A status maps to its `Failure`; a refusal's
  `{"error": {"code", "message"}}` becomes what Jev said (`code: message`, or
  whichever of the two is there), cut to 200 characters. `Limited` reads
  `Retry-After` in whole seconds, and `retry_after_ms()` turns it into
  milliseconds. Transport errors read as a short reason: the connection was
  refused, the host wasn't found, the connection failed, the reply is over
  1 MiB.
- **Hides the key** in `Debug` (`Key(hidden)`) and `Display` (`hidden`), and
  takes it only as one word: `Key::new` trims it and refuses an empty value or
  one with whitespace inside.
- **Reads replies that came another way.** `failure_of`, `billing_of` and
  `reply_of` are the readers `ask` uses, on their own, for a reply from a
  proxy, a queue or a recording.

## The API

Everything is at the crate root, `pito_jev::…`.

### Constants

| Item | Value |
| --- | --- |
| `URL: &str` | `"https://jev-ai.pro/api/v1/systemone"`, where `Client::live()` posts |
| `MODEL: &str` | `"jev-1.14.0"`, the model to name in `body` |
| `REQUEST_DEADLINE: Duration` | 2 s, the live client's deadline over a whole call |
| `MOST_BODY: usize` | 256,000, the most bytes a body may hold |
| `MOST_QUESTIONS: usize` | 64, the most questions a call may hold |
| `MOST_REPLY: u64` | 1,048,576, the most bytes of reply read |

### Functions

| Signature | Meaning |
| --- | --- |
| `body(model: &str, state: &Value, questions: &[(&str, Question)]) -> Result<String, String>` | The request's JSON text, or why it can't be sent |
| `failure_of(status: u16, retry_after: Option<&str>, body: &str) -> Failure` | The failure a status, a `Retry-After` value and a reply body mean |
| `billing_of(header: &dyn Fn(&str) -> Option<String>) -> Billing` | The billing read from headers, each asked for by its lowercase name |
| `reply_of(body: &str) -> Result<Reply, Failure>` | A reply body read as a `Reply`, or `Malformed` |

`Value` is `serde_json::Value`.

### Types

| Type | What it is |
| --- | --- |
| `Key` | The API key. `Key::new(value: &str) -> Option<Key>` trims it and refuses an empty value or one with whitespace inside. `Clone`, `PartialEq`, `Eq`; `Debug` prints `Key(hidden)` and `Display` prints `hidden`. |
| `Question` | `instructions: String`, `yes: Option<String>`, `no: Option<String>`, `options: Vec<(String, Option<String>)>`. Yes/no while `options` is empty, else a choice. `Default`, `Clone`, `Debug`, `PartialEq`. |
| `Client` | `url: String`, `deadline: Duration`. `Client::live() -> Client` is Jev with the 2 s deadline, and `Default` is the same. `Clone`, `Debug`, `PartialEq`, `Eq`. |
| `Called` | What a call that succeeded returns: `reply: Reply`, `billing: Billing`, `latency_ms: u64`. |
| `Reply` | `model: String`, `answers: Map<String, Value>` (each answer by id, in the reply's order), `usage: Usage`. |
| `Answer` | `kind: String` (Jev's `type`), `noul: Option<f64>`, `choice: Option<String>`, `score: Option<Value>`, `legend: Option<Value>`, `probabilities: Value`, `confidence: Option<f64>`. |
| `Usage` | `input_tokens: u64`, `output_tokens: u64`. |
| `Billing` | `run_id: Option<String>`, `credits_charged: Option<f64>`, `tokens_remaining: Option<f64>`, `credits_remaining: Option<f64>`. |
| `Failure` | `BadKey`, `NoBalance`, `BadBody(String)`, `Limited(Option<u64>)`, `Unavailable(u16)`, `Unknown`, `Deadline`, `Unreached(String)`, `Malformed(String)`, `Other(u16, String)`. `Display` and `std::error::Error`. |

`Reply`, `Answer`, `Usage` and `Billing` are `Default`, `Clone`, `Debug` and
`PartialEq`; `Reply`, `Answer` and `Usage` are serde `Deserialize`. `Called`
and `Failure` are `Clone`, `Debug` and `PartialEq`.

### Methods

| Signature | Meaning |
| --- | --- |
| `Client::ask(&self, key: &Key, body: &str) -> Result<Called, Failure>` | Posts the body once and reads the answer, within the deadline |
| `Reply::answer(&self, id: &str) -> Option<Answer>` | The answer to one question, if it is there and reads |
| `Answer::yes(&self) -> Option<f64>` | The probability of yes, when it lies between 0 and 1 |
| `Answer::picked(&self) -> Option<(String, f64)>` | The chosen option with its probability, or Jev's confidence when there is none, between 0 and 1 |
| `Failure::words(&self) -> String` | The failure in one line, the same as `Display` |
| `Failure::retry_after_ms(&self) -> Option<u64>` | How long Jev asked to wait, for a `Limited` that says |

## Examples

Each block below is a doc test of this crate: the ones that would reach Jev
are only compiled, and the rest run against fixed inputs.

### A yes/no question, with what yes and no mean

```rust
use pito_jev::{MODEL, Question, body};
use serde_json::json;

let rain = Question {
    instructions: "Is it raining?".into(),
    yes: Some("drops on the window".into()),
    no: Some("a dry pavement".into()),
    ..Question::default()
};
let text = body(MODEL, &json!("a calm day"), &[("rain", rain)]).unwrap();
assert_eq!(
    text,
    r#"{"model":"jev-1.14.0","state":"a calm day","questions":{"rain":{"type":"noul","instructions":"Is it raining?","criteria":{"true":"drops on the window","false":"a dry pavement"}}}}"#
);
```

### A choice question

```rust,no_run
use pito_jev::{Client, Key, MODEL, Question, body};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let key = Key::new(&std::env::var("JEV_API_KEY")?).ok_or("JEV_API_KEY is not one word")?;
    let track = Question {
        instructions: "Which track should this train take?".into(),
        options: vec![
            ("fast".into(), Some("the short gate, in a minute".into())),
            ("full".into(), Some("every suite, in ten".into())),
            ("hold".into(), None),
        ],
        ..Question::default()
    };
    let state = json!({"train": 41, "branches": 6, "touched": ["src/jev.rs", "README.md"]});
    let called = Client::live().ask(&key, &body(MODEL, &state, &[("track", track)])?)?;
    if let Some((pick, p)) = called.reply.answer("track").and_then(|answer| answer.picked()) {
        println!("take {pick} (p = {p:.2})");
    }
    Ok(())
}
```

### Many questions in one call

```rust,no_run
use pito_jev::{Client, Key, MODEL, Question, body};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let key = Key::new(&std::env::var("JEV_API_KEY")?).ok_or("JEV_API_KEY is not one word")?;
    let state = json!({"unread": 37, "flagged": ["invoice 1182", "contract renewal"]});
    let questions = [
        (
            "urgent",
            Question {
                instructions: "Does anything need an answer today?".into(),
                ..Question::default()
            },
        ),
        (
            "triage",
            Question {
                instructions: "Should the inbox be triaged now?".into(),
                yes: Some("more than 30 are unread".into()),
                ..Question::default()
            },
        ),
        (
            "first",
            Question {
                instructions: "Which flagged mail comes first?".into(),
                options: vec![("invoice".into(), None), ("contract".into(), None)],
                ..Question::default()
            },
        ),
    ];
    let called = Client::live().ask(&key, &body(MODEL, &state, &questions)?)?;
    for (id, _) in &questions {
        match called.reply.answer(id) {
            Some(answer) => match (answer.yes(), answer.picked()) {
                (Some(p), _) => println!("{id}: yes {p:.2}"),
                (None, Some((pick, p))) => println!("{id}: {pick} {p:.2}"),
                (None, None) => println!("{id}: no usable answer"),
            },
            None => println!("{id}: no answer"),
        }
    }
    Ok(())
}
```

The caps hold before anything is sent:

```rust
use pito_jev::{MODEL, MOST_QUESTIONS, Question, body};
use serde_json::json;

let ids: Vec<String> = (0..=MOST_QUESTIONS).map(|n| format!("q{n}")).collect();
let questions: Vec<(&str, Question)> =
    ids.iter().map(|id| (id.as_str(), Question::default())).collect();
assert!(body(MODEL, &json!({}), &questions[..64]).is_ok());
assert_eq!(
    body(MODEL, &json!({}), &questions).unwrap_err(),
    "a call takes 1 to 64 questions, not 65"
);
let twice = [("a", Question::default()), ("a", Question::default())];
assert_eq!(
    body(MODEL, &json!({}), &twice).unwrap_err(),
    "the question id a is given twice"
);
assert!(body(MODEL, &json!("x".repeat(256_000)), &questions[..1]).is_err());
```

### Reading the probability

```rust
use pito_jev::{Failure, reply_of};

fn main() -> Result<(), Failure> {
    let reply = reply_of(
        r#"{
            "model": "jev-1.14.0",
            "answers": {
                "ready": {"type": "noul", "noul": 0.82, "confidence": 0.9},
                "track": {"type": "choice", "choice": "full",
                          "probabilities": {"fast": 0.3, "full": 0.7}},
                "hold": {"type": "choice", "choice": "wait", "confidence": 0.6},
                "odd": {"type": "noul", "noul": 1.7}
            },
            "usage": {"input_tokens": 61, "output_tokens": 4}
        }"#,
    )?;
    let ready = reply.answer("ready").unwrap();
    assert_eq!(ready.yes(), Some(0.82));
    assert_eq!(ready.confidence, Some(0.9));
    let track = reply.answer("track").unwrap();
    assert_eq!(track.picked(), Some(("full".to_string(), 0.7)));
    assert_eq!(track.probabilities["fast"], 0.3);
    assert_eq!(reply.answer("hold").unwrap().picked(), Some(("wait".to_string(), 0.6)));
    assert_eq!(reply.answer("odd").unwrap().yes(), None);
    assert_eq!(reply.answer("missing"), None);
    Ok(())
}
```

### Tokens and credits

```rust
use pito_jev::{Called, billing_of, reply_of};

fn cost(called: &Called) -> String {
    let usage = &called.reply.usage;
    let billing = &called.billing;
    format!(
        "{} in, {} out, {} credits, {} left, run {}",
        usage.input_tokens,
        usage.output_tokens,
        billing.credits_charged.unwrap_or_default(),
        billing.credits_remaining.map_or("?".to_string(), |left| left.to_string()),
        billing.run_id.as_deref().unwrap_or("unnamed"),
    )
}

let called = Called {
    reply: reply_of(r#"{"usage": {"input_tokens": 1234, "output_tokens": 3}}"#).unwrap(),
    billing: billing_of(&|name| match name {
        "x-jev-run-id" => Some("run-0042".to_string()),
        "x-jev-credits-charged" => Some("0.75".to_string()),
        "x-jev-tokens-remaining" => Some("soon".to_string()),
        _ => None,
    }),
    latency_ms: 140,
};
assert_eq!(called.billing.tokens_remaining, None);
assert_eq!(cost(&called), "1234 in, 3 out, 0.75 credits, ? left, run run-0042");
```

### Handling each failure

```rust
use pito_jev::{Failure, failure_of, reply_of};

fn next_step(failure: &Failure) -> String {
    match failure {
        Failure::BadKey => "stop: the key was refused".into(),
        Failure::NoBalance => "stop: top up the balance".into(),
        Failure::BadBody(said) => format!("stop: fix the body ({said})"),
        Failure::Limited(_) => match failure.retry_after_ms() {
            Some(ms) => format!("wait {ms} ms, then ask again"),
            None => "wait, then ask again".into(),
        },
        Failure::Deadline | Failure::Unknown => "it may have been charged: check first".into(),
        Failure::Unavailable(_) | Failure::Unreached(_) => "ask again later".into(),
        Failure::Malformed(why) => format!("don't trust it: {why}"),
        Failure::Other(code, said) => format!("unexpected {code}: {said}"),
    }
}

let refused = failure_of(401, None, r#"{"error": {"code": "unauthorized"}}"#);
assert_eq!(refused, Failure::BadKey);
assert_eq!(refused.to_string(), "401: Jev refused the key");
assert_eq!(next_step(&refused), "stop: the key was refused");

let limited = failure_of(429, Some("30"), "");
assert_eq!(limited, Failure::Limited(Some(30)));
assert_eq!(next_step(&limited), "wait 30000 ms, then ask again");

let bad_body = failure_of(422, None, r#"{"error": {"code": "invalid", "message": "questions is empty"}}"#);
assert_eq!(next_step(&bad_body), "stop: fix the body (invalid: questions is empty)");

assert_eq!(Failure::Deadline.to_string(), "no answer within 2 s");
assert_eq!(next_step(&Failure::Deadline), "it may have been charged: check first");

let garbled = reply_of("<html>not Jev's JSON</html>").unwrap_err();
assert!(matches!(garbled, Failure::Malformed(_)));
assert!(next_step(&garbled).starts_with("don't trust it: "));
```

In a real call, the same `match` takes the `Err` of `Client::ask`:

```rust,no_run
use pito_jev::{Client, Failure, Key};

fn ask(key: &Key, text: &str) -> Option<pito_jev::Called> {
    match Client::live().ask(key, text) {
        Ok(called) => Some(called),
        Err(Failure::Limited(secs)) => {
            eprintln!("rate limited; next call in {} s at the soonest", secs.unwrap_or(60));
            None
        }
        Err(failure) => {
            eprintln!("{failure}");
            None
        }
    }
}
```

### Giving the key

The caller decides where the key lives. From an environment variable, a
mounted secret file, or a secret store with a command line (`secret-tool` on
Linux here; `security find-generic-password -w` on macOS works the same way):

```rust,no_run
use std::process::Command;

use pito_jev::Key;

fn from_env() -> Option<Key> {
    Key::new(&std::env::var("JEV_API_KEY").ok()?)
}

fn from_file() -> Option<Key> {
    Key::new(&std::fs::read_to_string("/run/secrets/jev_api_key").ok()?)
}

fn from_store() -> Option<Key> {
    let found = Command::new("secret-tool")
        .args(["lookup", "service", "jev"])
        .output()
        .ok()?;
    Key::new(std::str::from_utf8(&found.stdout).ok()?)
}

fn main() {
    let key = from_env()
        .or_else(from_file)
        .or_else(from_store)
        .expect("a Jev API key");
    assert_eq!(key.to_string(), "hidden");
}
```

`Key::new` trims the trailing newline a file or a command leaves, and the key
never prints:

```rust
use pito_jev::Key;

let key = Key::new("  jev-example-0001\n").unwrap();
assert_eq!(format!("{key}"), "hidden");
assert_eq!(format!("{key:?}"), "Key(hidden)");
assert_eq!(Key::new("two words"), None);
assert_eq!(Key::new(""), None);
```

### Pointing at a test server

```rust,no_run
use std::time::Duration;

use pito_jev::Client;

let client = Client {
    url: "http://127.0.0.1:8472/api/v1/systemone".into(),
    deadline: Duration::from_millis(500),
};
```

The repository's examples, each runnable as `cargo run --example <name>`:

- [`ask`](examples/ask.rs): one question from the command line,
  `ask [--url URL] <STATE> <QUESTION> [OPTION[=DESCRIPTION]...]`, with the key
  in `JEV_API_KEY`; options make it a choice question.
- [`stand_in`](examples/stand_in.rs): the local stand-in on its own,
  `stand_in <PORT> <KEY>`.
- [`choice`](examples/choice.rs): a choice question, the pick and every
  option's probability.
- [`many`](examples/many.rs): three questions in one call, each answer, the
  tokens and the credits, and a 65th question refused.
- [`failures`](examples/failures.rs): ten calls that fail, from a refused key
  to a closed port, each with its words and what to do next.

The stand-in ([examples/support/stand_in.rs](examples/support/stand_in.rs))
answers `POST /api/v1/systemone` in Jev's shape with numbers made up from the
question's text, and fails on purpose under `/status/<code>`, `/slow` (2.5 s),
`/garbled` and `/huge` (over 1 MiB). It refuses a wrong key with 401, and a
body over 256,000 bytes, one that isn't JSON or one with no or over 64
questions with 422.

## Part of PITO MD

pito-jev is one of the building blocks of [PITO MD](https://pitomd.com) apps;
Dispatch asks Jev through it. The clip is recorded by
[prec](https://github.com/gmrdad82/prec) and made a GIF by
[pcod](https://github.com/gmrdad82/pito-codecs) (pito-codecs).

## Development

```sh
bin/gate
bin/gate --fast
cargo llvm-cov nextest --all-targets --ignore-filename-regex 'examples/|tests/'
```

`bin/gate` runs ShellCheck where it is installed, `cargo fmt --check`, clippy
on every target with warnings as errors, the tests (`cargo nextest run`, or
`cargo test` without nextest), the doc tests, which are this README's
examples, and the release build of the library and every example;
`bin/gate --fast` leaves the release build out. It passes before every merge
and every tag.

The tests never reach Jev. The unit tests in `src/tests.rs` cover the request
body, the readers and every failure's words; `tests/api.rs` runs calls against
small local servers (each status, a silent server, a reply that stops halfway,
a reply over 1 MiB, and that the key rides only the header); and
`tests/stand_in.rs` runs every failure and every cap against the stand-in,
including the live 2 s deadline. Every input is fixed, and the only waits are
the deadlines under test. With cargo-llvm-cov, the tests cover 100% of the
lines of `src/lib.rs` (221 of 221, on 2026-10-27).

Each release is listed in [CHANGELOG.md](CHANGELOG.md), and each tag's GitHub
Release carries its notes. The clip is the prec shot in
[render/terminal.toml](render/terminal.toml) and its tape, made a GIF by pcod;
[NOTICE](NOTICE) lists every image and where it comes from.

## Contributing

Issues and pull requests are welcome at
[github.com/gmrdad82/pito-jev](https://github.com/gmrdad82/pito-jev/issues):
a reply the library reads wrongly, with the body if you can share it (never
the key), or something it should handle. Please read the
[code of conduct](CODE_OF_CONDUCT.md) first. A change keeps `bin/gate` green
with no warnings and never sends a test to Jev. Report a security issue
privately, as [SECURITY.md](SECURITY.md) says, not in a public issue.

## Licence

<img src="assets/marks/pitomd-wordmark.svg" width="320" alt="PITO MD">

The code is MIT licensed: see [LICENSE](LICENSE), by Catalin Ilinca. The
pito-jev and PITO MD names and their logos, and the names and logos of every
PITO MD app and game, are © Catalin Ilinca, all rights reserved, and are not
covered by the MIT licence; see [TRADEMARKS.md](TRADEMARKS.md). The GIF in
`docs/images/` is a capture of this repository's examples, under the MIT
licence like the code ([NOTICE](NOTICE)). Jev is its owner's name, and this
project is not affiliated with it.
