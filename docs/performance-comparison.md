# Measured MST performance comparison

## Figures and data

- [Runtime comparison, PNG](../benchmarks/figures/mst_runtime_comparison.png) / [SVG](../benchmarks/figures/mst_runtime_comparison.svg)
- [Performance gap and comparison work, PNG](../benchmarks/figures/mst_cost_analysis.png) / [SVG](../benchmarks/figures/mst_cost_analysis.svg)
- [Raw measurements and metadata](../benchmarks/results/comparison.json)
- [Summary CSV](../benchmarks/figures/summary.csv)

These are measured practical implementations. They are not a benchmark of the
published deterministic linear-time DLMST algorithm, nor a claim that any reference
is the globally fastest MST implementation. The inspected DLMST repository still
contains papers and no executable implementation.

## References being compared

| Implementation | Configuration | Notes |
| --- | --- | --- |
| Our Rust DLMST prototype | 1 thread, default options | Experimental mechanisms; weight-comparison counters enabled |
| petgraph Kruskal | 1 thread, version 0.8.3 | Standard Rust MSF reference |
| petgraph Prim | 1 thread, version 0.8.3 | All benchmark inputs are connected, as required by this implementation |
| PBBS Filter-Kruskal | 1 Parlay worker | Research benchmark implementation; two disclosed patches |
| PBBS Filter-Kruskal | 8 Parlay workers | Parallel comparison, not an equal-thread speedup comparison |

Sources: [petgraph MST implementations](https://docs.rs/petgraph/0.8.3/petgraph/algo/min_spanning_tree/index.html),
[PBBS MSF benchmark specification](https://cmuparlay.github.io/pbbsbench/benchmarks/minSpanningForest.html),
[PBBS Filter-Kruskal source](https://github.com/cmuparlay/pbbsbench/blob/3932578095bb73a2b5dbf8102ed5ebe3f5041b19/benchmarks/minSpanningForest/parallelFilterKruskal/MST.C),
[DLMST source repository](https://github.com/Louiealbp/DLMST).

## Protocol

Measured on 2026-11-24, Intel Core i7-12700F, WSL Ubuntu, Rust 1.86.0 release build,
GCC 13.3.0 with `-O3 -DNDEBUG -std=c++17 -pthread`. PBBS revision:
`3932578095bb73a2b5dbf8102ed5ebe3f5041b19`; its Parlay revision:
`f8e38c71efc209ff60cf31bce3723e74d3730832`.

- 48 connected undirected multigraphs: 2 graph families, 2 densities, 4 sizes,
  and 3 seeds (42, 43, 44).
- Vertex counts: 128, 512, 2,048 and 8,192. Edge counts: `8n` and `32n`, up to
  262,144 edges. Parallel edges are allowed; self-loops are not generated.
- Uniform family: random-parent spanning tree, then uniformly sampled endpoint
  pairs. Local family: spanning path, then edges connecting vertices at circular
  distance 1 through 16. The tree/path ensures connectivity. Edge order is shuffled.
- Weights are integers in [-1000, 1000], exactly representable in both Rust `i64`
  and PBBS `float32`. Ties use original edge IDs. Different native data layouts
  remain part of the implementation comparison.
- One warmup, then five measured repetitions per graph per implementation:
  1,200 measured solver invocations, excluding warmups.
- Parsed inputs and algorithm-specific graph representations are prepared outside
  timing. Solving and sorting the output edge IDs are timed. Solver-internal
  allocations are included; parsing, generation and serialization are excluded.
- Rust algorithm order rotates each repetition. PBBS worker-count order alternates
  by seed. Adapters run sequentially. No CPU affinity or frequency locking was used;
  system noise and the hybrid CPU can affect timings.
- Every measured/warmup result is checked against the same sorted edge-ID set.
  All five configurations matched on all 48 inputs.

Each runtime point is the median of the three per-graph timing medians. The band
spans those graph medians, not a confidence interval. Slowdown ratios are computed
on each graph against the fastest of the three single-thread references, then
summarized by their median. Ratios of separately aggregated timings can differ.

Prototype comparison counters are enabled, while references are uninstrumented.
The cost chart shows only prototype weight comparisons; its segments do not
measure elapsed-time shares. There is no cross-algorithm comparison-count claim.

The inputs are small synthetic graphs, not the official million-vertex PBBS
benchmark suite. Results do not establish a ranking across other inputs, machines,
GPU implementations, or larger parallel workloads. The timing protocol differs
from the earlier smoke benchmarks, which included reference graph construction.

## Results at 8,192 vertices

Median milliseconds:

| Graph family | Edges | Prototype | petgraph Kruskal | petgraph Prim | PBBS 1 worker | PBBS 8 workers |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Uniform, `m=8n` | 65,536 | 61.625 | 8.778 | 16.342 | 2.583 | 1.977 |
| Uniform, `m=32n` | 262,144 | 165.988 | 42.037 | 44.287 | 6.352 | 3.690 |
| Local, `m=8n` | 65,536 | 55.723 | 8.800 | 13.174 | 3.164 | 2.315 |
| Local, `m=32n` | 262,144 | 132.911 | 42.744 | 51.669 | 7.472 | 3.051 |

The paired median slowdown against the fastest measured single-thread reference
is 23.9x, 26.0x, 18.1x and 17.8x respectively. All are PBBS at these sizes. The
prototype is a correctness/mechanism experiment and is not currently competitive
with these practical references. These measurements say nothing about the runtime
of a full theorem-faithful DLMST implementation.

## PBBS patches

The algorithm is taken from the pinned upstream sources, with its copyright and
license notices retained. Only these changes are applied by `prepare_pbbs.py`:

1. Disable the internal diagnostic timer (`timer t("mst", false)`) so progress
   printing is not included in the benchmark.
2. In `common/speculative_for.h`, initialize the round size with
   `std::max(1L, maxRoundSize/4)` instead of `maxRoundSize/4`.

The second change was necessary: the unmodified source aborted on the uniform
`n=128`, `m=1024`, seed-43 input. Instrumenting the exception confirmed a remaining
batch of nine items and a round size of zero. The guard prevents a nonempty loop
from doing no work. The original exception message was restored for measurements;
the scheduling guard is marked with an asterisk on the chart. Correctness was
rechecked after patching. It has not been submitted upstream.

## Reproduce on Linux / WSL

From the project root (Python 3.12, g++, and a Rust toolchain installed):

```sh
cargo build --release --locked --bin compare
python3 benchmarks/prepare_pbbs.py
python3 benchmarks/run_comparison.py
python3 -m venv .chart-env
.chart-env/bin/pip install -r benchmarks/requirements.txt
.chart-env/bin/python benchmarks/plot_comparison.py
```

`prepare_pbbs.py` downloads only the small set of required PBBS headers/source and
the pinned Parlay archive. Downloaded dependencies and the compiled adapter stay
under `target/`; they are not project source. Each raw-data row includes an input
SHA-256 digest and the complete prototype options and statistics.
