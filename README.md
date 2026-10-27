<p align="center"><img src="docs/images/ask.gif" width="800" alt="A terminal: the ask example puts a yes/no question to a local stand-in for Jev and prints yes 0.14 with its confidence, the model, the tokens and the credits; then a choice question between fast and full prints picked full 0.55; then, with a wrong key, it prints 401: Jev refused the key."></p>

# pito-jev

[![Version](https://img.shields.io/github/v/tag/gmrdad82/pito-jev)](https://github.com/gmrdad82/pito-jev/tags)

**A small Rust client for Jev's systemone API.** Describe a state, ask up to 64 yes/no or choice questions about it, and get each answer back as a probability, together with the tokens used and the credits charged. One blocking call, a 2 s deadline, no retries, and every failure a typed value. It's part of [PITO MD](https://pitomd.com) and is not made by or affiliated with Jev.

| | |
|---|---|
| Library code | **320 lines** of Rust in one file |
| Dependencies | **3** direct (ureq with rustls, serde, serde_json), no OpenSSL or other system library |
| A call | at most **2 s**, at most **64** questions, a body of at most **256,000** bytes and a reply of at most **1 MiB** |

Tested on Linux; written for Windows and macOS too, untested there.

Got questions or doubts? Ping me on Discord: https://discord.gg/XsNcn3kYzn

## Install and first use

pito-jev isn't on crates.io. Add it from this repository, pinned to a [tag](https://github.com/gmrdad82/pito-jev/tags); the line is the same on Linux, Windows and macOS:

```toml
[dependencies]
pito-jev = { git = "https://github.com/gmrdad82/pito-jev", tag = "v0.1.0" }
```

```rust
use pito_jev::{Client, Key, MODEL, Question, body};
use serde_json::json;

let key = Key::new(&my_jev_api_key).expect("one word");
let ready = Question {
    instructions: "Is this train ready to merge?".into(),
    yes: Some("every branch is reviewed and the gate is green".into()),
    ..Question::default()
};
let text = body(MODEL, &json!("train 41: 6 branches, gate green"), &[("ready", ready)])?;
let called = Client::live().ask(&key, &text)?;
let yes = called.reply.answer("ready").and_then(|answer| answer.yes());
```

`yes` is the probability, from 0 to 1, that the answer is yes. The library never looks for a key by itself: it reads no environment variable, file or keyring, and takes the `Key` you give it. A `Key` prints as `hidden` in `Debug` and `Display`, so it can't leak into a log, and it only ever travels in the `Authorization` header. The package is `pito-jev` and the code says `use pito_jev::…`; it needs Rust 1.97 or newer.

To watch it work without a Jev account, the repository's two examples run a local stand-in that answers in Jev's shape, as in the clip above (its numbers are made up, not Jev's):

```sh
git clone https://github.com/gmrdad82/pito-jev && cd pito-jev
cargo build --release --examples
target/release/examples/stand_in 8472 demo-key &
export JEV_API_KEY=demo-key
target/release/examples/ask --url http://127.0.0.1:8472/api/v1/systemone 'a calm day' 'Is it raining?'
```

Leave out `--url` and set `JEV_API_KEY` to a real key to ask Jev itself. `kill %1` stops the stand-in.

## What it does

### Questions

A `Question` is a yes/no question unless it has `options`:

- **Yes/no** (Jev's `noul` type): `instructions`, and optionally what `yes` and `no` mean. The answer's `yes()` is the probability of yes, kept only when it lies between 0 and 1.
- **Choice:** `options` lists each option's key and an optional description. The answer's `picked()` is the chosen key with its probability, or with Jev's confidence when the reply gives no probability for it.

`body(model, &state, &questions)` builds the request: the model (`MODEL` is `jev-1.14.0`), the state as any JSON value, and the questions under their ids, in the order given. It refuses no questions, more than 64, an id given twice, or a body over 256,000 bytes, before anything is sent.

### One call, bounded

`Client::live()` posts to `https://jev-ai.pro/api/v1/systemone` with a 2 s deadline over the whole call, connecting and reading included. A reply is read up to 1 MiB, and anything longer is refused as malformed. A call is made exactly once and never retried, so a call that might have been charged is never charged twice; whether to try again is the caller's choice, and `retry_after_ms()` says how long Jev asked to wait. `Client { url, deadline }` points it elsewhere, such as at a test server.

A successful call returns `Called`: the `reply` (the model, each answer by id, and the input and output tokens), the `billing` read from Jev's `X-Jev-*` headers (the run id, credits charged, tokens and credits remaining; a header that doesn't read is left out), and the `latency_ms`.

### Failures

Every failure is a `Failure`, and `words()` (or `Display`) says it in one line that never holds the key:

| Failure | When |
|---|---|
| `BadKey` | 401: Jev refused the key |
| `NoBalance` | 402: the balance is spent |
| `BadBody(said)` | 422: Jev refused the body, with what it said, up to 200 characters; never send it again unchanged |
| `Limited(secs)` | 429: rate limited, with the `Retry-After` seconds when Jev gives them |
| `Unavailable(code)` | 502 or 503 |
| `Unknown` | 504: the call's outcome is unknown |
| `Deadline` | no answer within the deadline |
| `Unreached(why)` | the host wasn't found or the connection failed |
| `Malformed(why)` | the reply isn't Jev's JSON, or is over 1 MiB |
| `Other(code, said)` | any other status |

`failure_of`, `billing_of` and `reply_of` are the same readers on their own, for a reply that arrived some other way.

The tests run every call against local servers: every status above, a silent server past the deadline, a closed port, a reply that isn't JSON or is over the cap, and checks that the key rides only the header.

[`examples/ask.rs`](examples/ask.rs) asks one question from the command line: `ask [--url URL] <STATE> <QUESTION> [OPTION[=DESCRIPTION]...]`, with the key in `JEV_API_KEY`; options make it a choice question. [`examples/stand_in.rs`](examples/stand_in.rs) is the local stand-in: `stand_in <PORT> <KEY>`.

## Part of PITO MD

pito-jev is one of the building blocks of [PITO MD](https://pitomd.com) apps; Dispatch asks Jev through it. The clip is recorded by [prec](https://github.com/gmrdad82/prec) and made a GIF by [pcod](https://github.com/gmrdad82/pito-codecs).

## Contributing

Issues are welcome at [github.com/gmrdad82/pito-jev/issues](https://github.com/gmrdad82/pito-jev/issues): a reply the library reads wrongly, with the body if you can share it (never the key), or something it should handle. Pull requests aren't taken. Please read the [code of conduct](CODE_OF_CONDUCT.md) first, and report a security issue privately as [SECURITY.md](SECURITY.md) says.

## Build and test

```
cargo build --release
bin/gate
```

`bin/gate` runs ShellCheck where it is installed, rustfmt, clippy with warnings as errors, the tests, the doctests and a release build of the library and examples; it passes before every merge and every tag. Each tag's GitHub Release carries the [CHANGELOG](CHANGELOG.md)'s notes. The clip is the prec shot in `render/terminal.toml`, and [NOTICE](NOTICE) lists every image and where it comes from.

## Licence

<img src="assets/marks/pitomd-wordmark.svg" width="320" alt="PITO MD">

The code is MIT licensed ([LICENSE](LICENSE)), © Catalin Ilinca. The pito-jev and PITO MD names and their logos are © Catalin Ilinca, all rights reserved, and are not covered by the MIT licence ([TRADEMARKS.md](TRADEMARKS.md)). Jev is its owner's name, and this project is not affiliated with it.
