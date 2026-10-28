# The Lastdown benchmark suite

A SOTA-grade benchmark for a mechanism whose value claims are specific:
a committed continuation, resumed by a fresh receiver, loses less scarce
resource than the alternatives at the same provider boundary, and preserves
more working context than a lossy summary. `lastdown prove` deliberately
leaves `punt_advantage` and `amnesia_advantage` unmeasured because its
receiver runs the same steps and no differential receiver exists. The
benchmark suite (`lastdown benchmark [dir]`) is the instrument that measures
those claims for real.

## Arms

Every variant of every fixture runs under three arms:

| Arm | Boundary behaviour | What the receiver gets |
| --- | --- | --- |
| `lastdown` | Freeze, commit typed package, release possession | The full typed continuation: position, evidence, contradictions, gates |
| `summary` | Same boundary, same commit, but the package is rewritten as a byte-capped prose summary | Objective plus the oldest gates that fit the budget; the tail is dropped |
| `naive` | Run until the boundary kills the context, restart cold | Nothing |

The summary arm is the differential receiver `prove` lacks. Its budget is
`SUMMARY_BUDGET_BYTES = 1024` — one screen of prose — and per-gate cost is
the package's own canonical bytes per committed gate, so the summary keeps
fewer gates as typed packages grow denser. The suite reports a budget
sensitivity sweep at 512 / 1024 / 2048 so no conclusion rests on the
constant.

## Metric

Primary metric: **waste** — units spent that produced no completed gate.

```
waste = total_units_consumed − Σ cost(completed gates at drive end)
```

One formula, applied identically to every arm by the harness, not by the
arms themselves. A scored drive with no repeats scores waste 0. A repeated
gate adds its cost. A dead context donates all of its spend. Secondary
metrics: summary-recovered duplicates (gates the summary dropped and the
receiver re-ran), scored rate, context losses, continuation bytes.

## Seeds

Fixtures lock their numbers, so the suite jitters in memory and keeps the
files untouched. Each fixture derives a deterministic variant family:

- fault fixtures: the boundary step moves to `k−1`, `k`, `k+1`
- quota fixtures: the token window moves by −500 / 0 / +500
- plain fixtures: `min_attempt_ppm` scales by 0.9 / 1.0 / 1.1

Every variant of a task runs under every arm, giving paired observations.

## Statistics

- Per-variant paired deltas (restart arm minus `lastdown`).
- Win/loss/tally with an exact two-sided sign test (ties dropped).
- Median delta with a 95% bootstrap confidence interval, 10,000 resamples
  from a fixed-seed xorshift generator — no floats in the seeding path, no
  platform drift, bit-identical ledgers across runs.
- Holm step-down correction across the two claim tests.
- Claims print `n`, wins, median, CI, raw and adjusted p.

## Decision rules (pre-declared)

`transfer_advantage` is **supported** only when: no `lastdown` run turns
the ball over, every replay verifies, the scored rate does not regress,
and waste is significantly lower than both restart arms (sign p ≤ 0.05
after Holm, positive median). Significant in the wrong direction prints
**unsupported**; under-powered or non-significant prints **unmeasured**.

`amnesia_advantage` is **supported** only when the same guardrails hold and
the summary arm's re-verified duplicates are significantly greater than
zero-cost recovery under `lastdown`.

A claim the suite cannot support stays unmeasured. Promotion into
`lastdown prove` requires the suite to hold across releases; `prove` keeps
its honest `unmeasured` until then.

## Threats to validity

- The provider boundary is simulated: wall-clock and provider-switch costs
  stay outside the ledger (the same honesty `prove` prints as `-`).
- Tasks are scripted single-script drives; no open-ended agent behaviour.
- The summary model is a declared approximation of prose handoff, swept
  for budget sensitivity rather than trusted blindly.
- Sign tests ignore effect magnitude; the median and CI are printed
  alongside for exactly that reason.
