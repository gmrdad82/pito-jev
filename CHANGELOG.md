# Changelog

## v0.1.1 — 2026-10-27

### Added

- The `choice`, `many` and `failures` examples: a choice question with every option's probability, several questions in one call with their tokens and credits, and ten calls that fail, each with what to do next. Each runs against its own local stand-in and never reaches Jev.
- The stand-in (`stand_in`) now fails on purpose under `/status/<code>`, `/slow`, `/garbled` and `/huge`, and refuses a body over 256,000 bytes or over 64 questions, so code can be tried against every failure.
- The README documents the whole API, the bounds and measured speed, with examples for each kind of question, reading answers, tokens and credits, giving the key and handling every failure; each example is a doc test.

### Fixed

- A reply over 1 MiB now fails as `Malformed("the reply is over 1 MiB")` instead of "the request failed".

## v0.1.0 — 2026-10-27

### Added

- A blocking client for Jev's systemone API: build a call of up to 64 yes/no or choice questions about a state with `body`, and send it once with `Client::ask`, within a 2 s deadline.
- Answers come back typed: the probability of yes, or the picked option with its probability, plus the tokens used and the credits charged.
- Every failure is a `Failure` that names the status in one line (a refused key, a spent balance, a refused body, rate limiting with its wait, the deadline and more).
- `Key` holds the API key and prints as `hidden`, and the key travels only in the `Authorization` header.
- The `ask` and `stand_in` examples ask a question from the command line, against Jev or a local stand-in.
