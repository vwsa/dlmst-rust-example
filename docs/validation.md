# Validation and measured results

Recorded on 2026-11-24 using Rust/Cargo 1.86.0 in WSL Ubuntu 24.04, x86-64,
on an Intel Core i7-12700F. Dependencies are recorded in `Cargo.lock`.

## Correctness and tooling

The following checks passed:

```sh
PROPTEST_RNG_SEED=20261124 cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo fmt --check
cargo build --release --locked
```

Ten named tests cover:

- All 729 four-vertex simple graphs with each edge absent, weight -1 or weight 0.
  Each result is compared to `petgraph`, checked by the cycle property, and its
  total weight is checked against exhaustive subset enumeration.
- 256 generated multigraphs across schedules, block counts, portal thresholds and
  Boruvka settings, with repeated-run identity of both results and counters.
- 256 generated small multigraphs with bounded preference enumeration enabled.
- Empty graphs, isolated vertices, loops, parallel edges, equal weights, negative
  weights and both extremes of `i64`; totals use `i128`.
- Group selection with invalid low-weight representatives and cached rejections.
- 180 sequential batch checks across 90 seeded graphs, checking the portal bound,
  declared-edge bound, and supported-or-valid-projection invariant.
- A constructed two-level projection: four projected-edge occurrences, two levels
  of recursion, one rejection propagated through an intermediate level and
  absorbed at its origin, with no depth fallback.
- All 120 orders of a five-key preference example. Estimates satisfy the promised
  per-round Hamming-error bound, and the example exercises both queried
  coordinates and nonzero estimation errors.
- Invalid graph/configuration errors and the JSON command-line interface.

These are finite correctness checks, not a proof of asymptotic bounds.

## Release smoke benchmarks

Each row reports the median of three independently generated multigraphs, with
seeds 42, 43 and 44, `m = 8n`, independently sampled endpoints, and integer weights
uniformly sampled from -1000 through 1000. Loops, duplicates and equal weights are
permitted. Every run matched the reference edge-ID set.

The comparison is this instrumented implementation against `petgraph`'s
uninstrumented Kruskal routine. Reference timing includes construction of its
graph representation. Solver timing includes its working-graph construction,
enumeration and bookkeeping. Neither includes input generation or JSON output.
Runs were sequential; no explicit warmup or statistical confidence interval was
used. These measurements show the current prototype's cost, not competitive
benchmark performance or an asymptotic result.

### Default parameters

| Vertices | Input edges | Prototype median, ms | Reference median, ms | Prototype median comparisons |
| ---: | ---: | ---: | ---: | ---: |
| 64 | 512 | 0.215 | 0.047 | 7,414 |
| 256 | 2,048 | 1.134 | 0.195 | 41,288 |
| 1,024 | 8,192 | 5.486 | 0.977 | 200,605 |
| 4,096 | 32,768 | 27.480 | 4.896 | 943,014 |

Across these 12 runs: 7,260 projected-edge occurrences, 277 rejections and 113,801
filtered edges. All runs used a no-progress exact fallback at the end; none used
a certificate-depth fallback. Routine local exact MSF calls were also used.

Raw data: [default results](../benchmarks/results/default.json).

### Certificate-focused parameters

Here `--boruvka-steps 0 --base-case-edges 0` forces certificate execution.

| Vertices | Input edges | Prototype median, ms | Reference median, ms | Prototype median comparisons |
| ---: | ---: | ---: | ---: | ---: |
| 64 | 512 | 0.535 | 0.043 | 14,134 |
| 256 | 2,048 | 2.464 | 0.187 | 75,740 |
| 1,024 | 8,192 | 12.321 | 0.896 | 370,324 |

Across these nine runs: 1,632 projected-edge occurrences, 240 rejections and
28,156 filtered edges. Each used a final no-progress fallback; there were no
certificate-depth fallbacks.

Raw data: [certificate results](../benchmarks/results/certificate.json).

The separate [64-vertex demo](../benchmarks/results/demo.json), seed 42, reports
29 projections, 25 lifted forest edges, one rejected/absorbed edge and 438 filtered
edges. It makes 24 exact MSF calls, including one no-progress fallback. Its tiny
preference families happen to be singletons; the dedicated 120-order unit test
exercises nontrivial distinguishing subsets and approximation.

## Reproduce

From the repository root, after `cargo build --release --locked`:

```sh
target/release/dlmst bench --sizes 64,256,1024,4096 --repetitions 3
target/release/dlmst bench --sizes 64,256,1024 --repetitions 3 \
  --boruvka-steps 0 --base-case-edges 0
target/release/dlmst demo --vertices 64 --edges 512 --seed 42 \
  --boruvka-steps 0 --base-case-edges 0
```

On native Windows use `target/release/dlmst.exe`; native Windows compilation has
not been tested in this workspace.

The implementation and tests occupy 1,911 physical Rust source lines (including
blank lines and comments), excluding dependencies, lockfile and documentation.
This is smaller than the original 2,500-4,000 line estimate because packages and
standard-library selection are reused, and the universal schedule and full
linear-time reduction remain outside this prototype.
