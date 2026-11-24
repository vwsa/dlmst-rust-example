# Reproduction scope and algorithm correspondence

## Acceptance criteria

1. Return the same edge IDs as an independent MSF implementation under ordering
   `(weight, original edge ID)`; preserve original IDs through every contraction.
2. Exercise nontrivial projection, lifting, multi-level rejection and filtering.
3. Verify the batch coverage invariant and the tiny estimator's Hamming-error bound.
4. Report all solver edge-weight comparisons and exact-solver substitutions.
5. Keep correctness results separate from comparison-complexity and timing claims.

## Component responsibilities

| Source | Responsibility | Paper correspondence |
| --- | --- | --- |
| `src/graph.rs` | Input validation, stable identities, projection records | Section 2 and 3.1 |
| `src/solver.rs` | Boruvka contraction, exact local MSF, safe filtering, statistics | Algorithm 2 |
| `src/certificate.rs` | Block/round execution, projection, lifting, rejection routing | Algorithm 1 |
| `src/batch.rs` | Group representatives, lazy admissibility, portals and spoke updates | Section 3.2, Lemma 3.3 |
| `src/preference.rs` | Symbolic preference families, distinguishing subsets, bounded estimation | Section 3.5 |
| `src/path.rs` | Forest path maxima used by preferences and filtering | Substitute for Lemma 2.9 |

## Explicit departures from the theorem

The paper establishes a linear comparison bound, then invokes the
Pettie-Ramachandran optimal-MST reduction to obtain a linear-time algorithm.
The reduction is **not implemented** here.

- **Schedule:** edges are assigned by local input order, round-robin by default;
  a contiguous assignment is also available. Both are independent of weights.
  The universal schedule search over all possible histories in Section 3.4 is
  not implemented. Neither policy has the paper's bad-round bound.
- **Parameters:** the proof uses depth-dependent `t = gamma*d^3`, `r = 8*t^3`,
  and quota `r^2`. We use configurable constant `t` (blocks and rounds per block)
  and `r` (portal threshold), with quota `r^2`. The default values are 3 and 2.
  They do not inherit the proof's recursion-size or error guarantees.
- **Preference estimation:** on small rounds, we enumerate all orders of the
  distinct edge identities needed by the fixed forest/spoke queries. This uses
  symbolic ranks, not input weight comparisons. We deduplicate preference vectors,
  enumerate a smallest distinguishing coordinate set for distance greater than
  `floor(round_edges/t)`, query those coordinates and choose a consistent vector.
  The resulting round error is bounded by `floor(round_edges/t)`.
  Enumeration defaults to at most 7 keys and 12 assigned edges, with hard limits
  of 256 vectors and 1,000,000 distinguishing-pair checks per round. Larger cases
  use exact action queries. This does not preserve the paper's comparison budget.
- **Path maxima:** binary lifting uses `O(n log n)` preprocessing and `O(log n)`
  per query. Komlos's linear-comparison batch algorithm is not implemented.
- **Exact MSF calls:** measured Kruskal with `petgraph::UnionFind` substitutes for
  the exact recursive MSF calls. It sorts by the shared counted comparator.
- **Termination:** small edge sets use a reported base case; no filtering progress
  triggers a reported exact solve. At the configurable certificate-depth limit,
  every edge is checked for admissibility, valid edges are solved exactly, and
  rejected edges are returned to their first invalid ancestor. Empty rounds are
  skipped because they do not alter the state.

These changes permit experiments with the certificate mechanisms but imply
neither `O(m)` comparisons nor `O(m)` time. A favorable timing curve would not
establish either theorem. The default parameters can finish many small graphs in
Boruvka or base cases; disable those with `--boruvka-steps 0 --base-case-edges 0`
to investigate the certificate directly.

## Why experimental scheduling still gives a correct MSF

The filtering argument needs a forest of actual input edges, not a good schedule.
The certificate inserts only admissible edges. When a child rejects an edge, it
is incorporated at the level where its first invalid projection was made; its
earlier history is therefore valid. Lifting restores that level's endpoints and
history. Exact local MSF calls maintain an acyclic forest and preserve threshold
connectivity.

At the root, the history is empty. For every edge outside the returned forest,
the solver discards it only if its endpoints are connected by edges no heavier
under the fixed total order. The cycle property makes that removal safe.
Poor scheduling can retain too many edges, which affects work rather than the
correctness of the final exact result.

## Measurement conventions

Every call to the shared weight comparator counts once, even for identical IDs
or equal weights. Its categories are Boruvka, exact MSF, grouped selection,
admissibility, path maximum, and spoke comparisons. Symbolic rank comparisons,
ID ordering, graph bookkeeping and allocation are not edge-weight comparisons.
Reported total weight and the independent reference are calculated after solving
and are excluded. Timings include the solver's bookkeeping and enumeration.

`exact_msf_calls` includes routine certificate merges as well as fallbacks.
`base_case_calls`, `no_progress_fallbacks` and `depth_limit_fallbacks` explain the
fallback subset. Zero fallbacks does not imply the absence of Kruskal subroutines.
`rejected_edges_propagated` counts ancestor hops, not unique edges.

## Remaining work for a theorem-faithful implementation

Implement and validate the universal schedule, the full preference machinery
without bounded substitutions, linear-comparison path maxima, the proof's
parameter regime, and the Pettie-Ramachandran reduction. The last step includes
small-instance optimal comparison-tree construction and its supporting machinery.
These are research tasks, not performance flags on the current executable.

Reference: [DLMST final paper, Sections 2-4](https://github.com/Louiealbp/DLMST/blob/0072335716b145032b418760883daed2a89809db/final.pdf).
