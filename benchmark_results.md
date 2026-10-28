# Benchmark results

Lastdown benchmark suite (`lastdown benchmark elci/fixtures`) against the
working tree at `4fb417f` (v0.1.5) with the suite harness added, on
2026-10-28, linux x86_64. The simulator is deterministic and network-free;
the ledger is bit-identical across runs (pinned by `tests/benchmark.rs`).

## Headline

| Claim | Verdict | n | Wins/Losses/Ties | Median delta | 95% CI | Holm p |
| --- | --- | --- | --- | --- | --- | --- |
| `transfer_advantage` | **supported** | 33 | 27 / 0 / 6 | +4000 tokens | [2000, 4000] | < 0.0001 |
| `amnesia_advantage` | **supported** | 33 | 27 / 0 / 6 | +2 gates re-verified | [1, 2] | < 0.0001 |

- **Transfer advantage.** Across all 33 paired variants, the Lastdown arm
  wasted zero units in every run. The summary-restart arm wasted a median
  of 4000 tokens re-verifying gates its prose summary dropped; the naive
  cold restart wasted a median of 4000 against recovery-capable boundaries
  and 8000 against exhaustion boundaries. The sign test is exact and
  two-sided over 27 non-tied pairs: 27 wins, 0 losses, p < 0.0001.
- **Amnesia advantage.** The typed package loses no completed work. The
  summary arm re-verified a median of 2 gates per boundary; the Lastdown
  receiver re-verified none.

## Guardrails (all hold)

- Lastdown turnovers: **0** across 33 runs.
- Lastdown replay verification: **ok** in every run; summary replays ok.
- Scored rate: Lastdown **0.82**, summary **0.82** (no regression), naive
  **0.45**.
- Every committed package passed the leak scan and canonical re-validation
  (enforced by the harness, not assumed).

## Per-fixture summary

Units of waste by arm (Lastdown / summary / naive), token scale:

| Fixture | Variants | Lastdown | Summary | Naive |
| --- | --- | --- | --- | --- |
| context_exhaustion | 3 | 0 | 4000 | 8000 |
| financial_budget | 3 | 0 | 4000 | 8000 |
| malformed_response | 3 | 0 | 0–4000 | 0–4000 |
| partial_tool | 3 | 0 | 2000–6000 | 2000–6000 |
| provider_loss | 3 | 0 | 0–4000 | 0–4000 |
| punt_loop | 3 | 0 | 0 | 0 |
| quota_limit | 3 | 0 | 2000–4000 | 0 |
| rate_limit | 3 | 0 | 0–4000 | 0–4000 |
| slow_provider | 3 | 0 | 4000 | 8000 |
| token_budget | 3 | 0 | 4000 | 8000 |
| uncommitted_changes | 3 | 0 | 4000 | 8000 |

The six ties are variants where the 1024-byte summary budget covered the
whole boundary context (`fault0`/`base` boundaries with one committed gate)
or where no drive scores (`punt_loop`); ties are excluded from the sign
test and reported as ties, not wins.

Two boundary shapes explain the naive arm's spread: exhaustion-class
boundaries (context, budget, slow provider, uncommitted changes) destroy
the whole context, so the cold restart repeats every gate; fault-class
boundaries only kill the tail, so summary and naive coincide wherever the
summary keeps nothing to recover. Lastdown is insensitive to the shape: the
committed package carries the field out of either boundary intact.

## Full ledger

```text
suite fixture=context_exhaustion variant=base lastdown_waste=0 summary_waste=4000 naive_waste=8000 lastdown_scored=true summary_scored=true naive_scored=false summary_duplicates=2 lastdown_bytes=2948 summary_bytes=6424 naive_bytes=0
suite fixture=context_exhaustion variant=attempt900 lastdown_waste=0 summary_waste=4000 naive_waste=8000 lastdown_scored=true summary_scored=true naive_scored=false summary_duplicates=2 lastdown_bytes=2948 summary_bytes=6424 naive_bytes=0
suite fixture=context_exhaustion variant=attempt1100 lastdown_waste=0 summary_waste=4000 naive_waste=8000 lastdown_scored=true summary_scored=true naive_scored=false summary_duplicates=2 lastdown_bytes=2948 summary_bytes=6424 naive_bytes=0
suite fixture=financial_budget variant=base lastdown_waste=0 summary_waste=4000 naive_waste=8000 lastdown_scored=true summary_scored=true naive_scored=false summary_duplicates=2 lastdown_bytes=2946 summary_bytes=6420 naive_bytes=0
suite fixture=financial_budget variant=attempt900 lastdown_waste=0 summary_waste=4000 naive_waste=8000 lastdown_scored=true summary_scored=true naive_scored=false summary_duplicates=2 lastdown_bytes=2946 summary_bytes=6420 naive_bytes=0
suite fixture=financial_budget variant=attempt1100 lastdown_waste=0 summary_waste=4000 naive_waste=8000 lastdown_scored=true summary_scored=true naive_scored=false summary_duplicates=2 lastdown_bytes=2946 summary_bytes=6420 naive_bytes=0
suite fixture=malformed_response variant=base lastdown_waste=0 summary_waste=2000 naive_waste=2000 lastdown_scored=true summary_scored=true naive_scored=true summary_duplicates=1 lastdown_bytes=2419 summary_bytes=5011 naive_bytes=0
suite fixture=malformed_response variant=fault0 lastdown_waste=0 summary_waste=0 naive_waste=0 lastdown_scored=true summary_scored=true naive_scored=true summary_duplicates=0 lastdown_bytes=2067 summary_bytes=4134 naive_bytes=0
suite fixture=malformed_response variant=fault2 lastdown_waste=0 summary_waste=4000 naive_waste=4000 lastdown_scored=true summary_scored=true naive_scored=true summary_duplicates=2 lastdown_bytes=2946 summary_bytes=6420 naive_bytes=0
suite fixture=partial_tool variant=base lastdown_waste=0 summary_waste=4000 naive_waste=4000 lastdown_scored=true summary_scored=true naive_scored=true summary_duplicates=2 lastdown_bytes=2940 summary_bytes=6409 naive_bytes=0
suite fixture=partial_tool variant=fault0 lastdown_waste=0 summary_waste=2000 naive_waste=2000 lastdown_scored=true summary_scored=true naive_scored=true summary_duplicates=1 lastdown_bytes=2413 summary_bytes=4999 naive_bytes=0
suite fixture=partial_tool variant=fault2 lastdown_waste=0 summary_waste=6000 naive_waste=6000 lastdown_scored=true summary_scored=true naive_scored=true summary_duplicates=3 lastdown_bytes=3250 summary_bytes=7178 naive_bytes=0
suite fixture=provider_loss variant=base lastdown_waste=0 summary_waste=2000 naive_waste=2000 lastdown_scored=true summary_scored=true naive_scored=true summary_duplicates=1 lastdown_bytes=2414 summary_bytes=5001 naive_bytes=0
suite fixture=provider_loss variant=fault0 lastdown_waste=0 summary_waste=0 naive_waste=0 lastdown_scored=true summary_scored=true naive_scored=true summary_duplicates=0 lastdown_bytes=2062 summary_bytes=4124 naive_bytes=0
suite fixture=provider_loss variant=fault2 lastdown_waste=0 summary_waste=4000 naive_waste=4000 lastdown_scored=true summary_scored=true naive_scored=true summary_duplicates=2 lastdown_bytes=2941 summary_bytes=6410 naive_bytes=0
suite fixture=punt_loop variant=base lastdown_waste=0 summary_waste=0 naive_waste=0 lastdown_scored=false summary_scored=false naive_scored=false summary_duplicates=0 lastdown_bytes=1754 summary_bytes=3508 naive_bytes=0
suite fixture=punt_loop variant=attempt900 lastdown_waste=0 summary_waste=0 naive_waste=0 lastdown_scored=false summary_scored=false naive_scored=false summary_duplicates=0 lastdown_bytes=1754 summary_bytes=3508 naive_bytes=0
suite fixture=punt_loop variant=attempt1100 lastdown_waste=0 summary_waste=0 naive_waste=0 lastdown_scored=false summary_scored=false naive_scored=false summary_duplicates=0 lastdown_bytes=1754 summary_bytes=3508 naive_bytes=0
suite fixture=quota_limit variant=base lastdown_waste=0 summary_waste=2000 naive_waste=0 lastdown_scored=false summary_scored=false naive_scored=true summary_duplicates=1 lastdown_bytes=2427 summary_bytes=5027 naive_bytes=0
suite fixture=quota_limit variant=quota-500 lastdown_waste=0 summary_waste=2000 naive_waste=0 lastdown_scored=false summary_scored=false naive_scored=true summary_duplicates=1 lastdown_bytes=2427 summary_bytes=5027 naive_bytes=0
suite fixture=quota_limit variant=quota+500 lastdown_waste=0 summary_waste=4000 naive_waste=0 lastdown_scored=false summary_scored=false naive_scored=true summary_duplicates=2 lastdown_bytes=2757 summary_bytes=5843 naive_bytes=0
suite fixture=rate_limit variant=base lastdown_waste=0 summary_waste=2000 naive_waste=2000 lastdown_scored=true summary_scored=true naive_scored=true summary_duplicates=1 lastdown_bytes=2411 summary_bytes=4995 naive_bytes=0
suite fixture=rate_limit variant=fault0 lastdown_waste=0 summary_waste=0 naive_waste=0 lastdown_scored=true summary_scored=true naive_scored=true summary_duplicates=0 lastdown_bytes=2059 summary_bytes=4118 naive_bytes=0
suite fixture=rate_limit variant=fault2 lastdown_waste=0 summary_waste=4000 naive_waste=4000 lastdown_scored=true summary_scored=true naive_scored=true summary_duplicates=2 lastdown_bytes=2938 summary_bytes=6404 naive_bytes=0
suite fixture=slow_provider variant=base lastdown_waste=0 summary_waste=4000 naive_waste=8000 lastdown_scored=true summary_scored=true naive_scored=false summary_duplicates=2 lastdown_bytes=2943 summary_bytes=6414 naive_bytes=0
suite fixture=slow_provider variant=attempt900 lastdown_waste=0 summary_waste=4000 naive_waste=8000 lastdown_scored=true summary_scored=true naive_scored=false summary_duplicates=2 lastdown_bytes=2943 summary_bytes=6414 naive_bytes=0
suite fixture=slow_provider variant=attempt1100 lastdown_waste=0 summary_waste=4000 naive_waste=8000 lastdown_scored=true summary_scored=true naive_scored=false summary_duplicates=2 lastdown_bytes=2943 summary_bytes=6414 naive_bytes=0
suite fixture=token_budget variant=base lastdown_waste=0 summary_waste=4000 naive_waste=8000 lastdown_scored=true summary_scored=true naive_scored=false summary_duplicates=2 lastdown_bytes=2940 summary_bytes=6408 naive_bytes=0
suite fixture=token_budget variant=attempt900 lastdown_waste=0 summary_waste=4000 naive_waste=8000 lastdown_scored=true summary_scored=true naive_scored=false summary_duplicates=2 lastdown_bytes=2940 summary_bytes=6408 naive_bytes=0
suite fixture=token_budget variant=attempt1100 lastdown_waste=0 summary_waste=4000 naive_waste=8000 lastdown_scored=true summary_scored=true naive_scored=false summary_duplicates=2 lastdown_bytes=2940 summary_bytes=6408 naive_bytes=0
suite fixture=uncommitted_changes variant=base lastdown_waste=0 summary_waste=4000 naive_waste=8000 lastdown_scored=true summary_scored=true naive_scored=false summary_duplicates=2 lastdown_bytes=3353 summary_bytes=7643 naive_bytes=0
suite fixture=uncommitted_changes variant=attempt900 lastdown_waste=0 summary_waste=4000 naive_waste=8000 lastdown_scored=true summary_scored=true naive_scored=false summary_duplicates=2 lastdown_bytes=3353 summary_bytes=7643 naive_bytes=0
suite fixture=uncommitted_changes variant=attempt1100 lastdown_waste=0 summary_waste=4000 naive_waste=8000 lastdown_scored=true summary_scored=true naive_scored=false summary_duplicates=2 lastdown_bytes=3353 summary_bytes=7643 naive_bytes=0
suite guardrails lastdown_turnovers=0 lastdown_replay_ok=true scored_rates lastdown=0.82 summary=0.82 naive=0.45
suite claim=transfer_advantage verdict=supported reason=waste_vs_summary_and_naive n=33 wins=27 losses=0 ties=6 median_delta=4000 ci95=[2000,4000] p=0.0000 holm_p=0.0000
suite claim=amnesia_advantage verdict=supported reason=duplicates_vs_summary n=33 wins=27 losses=0 ties=6 median_delta=2 ci95=[1,2] p=0.0000 holm_p=0.0000
```

## Method and scope

Arms, metric, seeds, statistics, and the pre-declared decision rules are in
`docs/benchmark.md`. In brief: three arms (typed-continuation resume,
1024-byte summary restart, naive cold restart) over deterministic jitter
variants of every fixture; one waste formula computed by the harness for
every arm; paired exact sign tests with a fixed-seed bootstrap interval and
Holm correction. Wall-clock and provider-switch costs stay outside the
simulator and outside this report. `lastdown prove` keeps its honest
`unmeasured` verdicts; promotion requires this suite to hold across
releases.

## Reproduce

```bash
cargo build --offline
cargo run --offline --bin lastdown -- benchmark elci/fixtures
```
