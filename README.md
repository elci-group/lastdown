# Lastdown

Lastdown treats quota, context, and provider boundaries as field position.
An incumbent keeps a continuation reserve it is not allowed to spend. When the
next ordinary cycle would enter that reserve, Lastdown freezes the field,
commits a typed continuation, and releases possession.

A fresh context resumes from the committed object. The field ledger records
observations, assertions, evidence, contradictions, corroborations, and
supersessions, and it keeps both sides of a contradiction. Blind scouting
hands each candidate an unattributed projection. The coordinator, which is
ordinary code, turns nominal distance into effective distance and assigns one
route from a family table. Scalar gate distance remains the authoritative
field distance. A compact integer vector is only a size comparison.

## Run

```text
cargo test
cargo run --bin lastdown -- bench elci/fixtures/token_budget.json
cargo run --bin lastdown -- sim elci/fixtures/provider_loss.json
cargo run --bin lastdown -- ablate elci/fixtures
cargo run --bin lastdown -- prove elci/fixtures
cargo run --bin lastdown -- jumbotron elci/fixtures/token_budget.json
cargo run --bin lastdown -- handoff elci/fixtures/token_budget.json /tmp/token_budget.continuation.json
cargo run --bin lastdown -- resume elci/fixtures/token_budget.json /tmp/token_budget.continuation.json
```

`handoff` stops at the committed punt and writes that package. `resume` loads
the file in a fresh process. The resume request supplies the receiver route.

Fixtures under `elci/fixtures/` are replayable exhaustion, fault, and
punt-loop cases. `ablate` prints lost resources for Lastdown and the baseline,
then the reserve, amnesia, and calibration gaps.

## Proof ledger

`prove` prints one row per fixture and three claims. `reserve_conservation`
is supported when Lastdown loses less scarce resource than the exhaustion
baseline, records no more context losses, and records no turnover. Wall-clock
time and provider switches are absent from the simulator and print as `-`.

`punt_advantage` stays unmeasured. On `token_budget` the ledger also records
the in-memory resume of the committed package. That receiver runs the same
steps. A summarise-and-restart arm and an orchestrator-switch arm are not in
the harness.

`amnesia_advantage` stays unmeasured. `isolation_clean` and `isolation_raw`
are the poisoned-field hit counts from `ablate`. A clean packet is a leak
scan. It is not a comparison of receiver selection.

## Jumbotron

`jumbotron` runs the bench and puts the drive on a stadium board: block
letters name the outcome, the objective rolls on the wire, and the budget
and quota bars grow to the final numbers. A terminal gets a short frame
loop; a pipe gets one deterministic frame. Rendering uses the local
`form3` crate by path, so its zero-dependency ANSI engine stays outside
the registry dependency set.

## Reserve

Ordinary work may spend `remaining - reserve_hold`. Continuation work may
spend the hold, and only while a punt or field goal is being sealed. A
provider boundary records a punt when the hold is intact. It does not record
a turnover. Fixtures lock the reserve so the punt point stays exact. An
unlocked budget follows `estimate_reserve` as the field grows.

## Quota

An InGauge probe or status snapshot can ride along on a fixture under `ingauge`.
Lastdown reads the tightest integer token window. A missing or fractional
measurement stays disengaged and leaves the drive on its token budget. When a
window is present, that remaining amount is the account quota: ordinary work
may spend `remaining - reserve`, and a later possession in the same campaign
keeps what the account has left. `lastdown quota <snapshot.json>` prints the
reading.

## Continuation

The package magic is `LASTDOWN/1`. Integrity covers the typed sections.
The inspection summary is not part of the hash and is not authoritative.
Restricted artifacts are refused when the destination is a foreign trust
domain. Credential-shaped text is stripped before the package is stored.
Ledger bytes at rest are sealed with HMAC-SHA256 over a SHA-256 keystream.
The caller supplies the key.

## Scouting and routing

Candidates receive evidence, gates, and contradictions. They do not receive
predecessor distance, rankings, coordinator notes, or model keys. A response
outside the requested range, or one that contradicts itself, is rejected.
Effective distance keeps every adjustment: historical error, variance,
calibration confidence, recovery cost, reliability, domain fit, and capability
headroom. Ties break on an explicit priority, then on the route id. Calibration
is stored per versioned model key, task class, and toolset.

## Encoding

Continuation bytes, replay checks, and telemetry stay in Rust. Canonical JSON
with section hashes is the measured object a fresh context resumes from. The
Tov toolchain is a separate language implementation and is not on that path.
The toungsten directory does not currently publish a library, so the vector
module is a fixed integer projection and resume does not read it.
