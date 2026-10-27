# Changelog

## v0.1.0 — 2026-10-27

### Added

- A blocking client for Jev's systemone API: build a call of up to 64 yes/no or choice questions about a state with `body`, and send it once with `Client::ask`, within a 2 s deadline.
- Answers come back typed: the probability of yes, or the picked option with its probability, plus the tokens used and the credits charged.
- Every failure is a `Failure` that names the status in one line (a refused key, a spent balance, a refused body, rate limiting with its wait, the deadline and more).
- `Key` holds the API key and prints as `hidden`, and the key travels only in the `Authorization` header.
- The `ask` and `stand_in` examples ask a question from the command line, against Jev or a local stand-in.
