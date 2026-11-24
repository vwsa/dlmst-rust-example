use crate::{
    certificate::Rejected,
    graph::WorkEdge,
    solver::{Context, Phase},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Spoke {
    pub portal: usize,
    pub edge: usize,
}

pub(crate) struct BatchResult {
    pub declared: Vec<WorkEdge>,
    pub rejected: Vec<Rejected>,
    pub portals: Vec<usize>,
}

#[derive(Clone, Copy, Debug)]
struct Candidate {
    index: usize,
    group: usize,
}

struct Selection<'a, 'b> {
    edges: &'a [WorkEdge],
    cache: BTreeMap<usize, Option<usize>>,
    ctx: &'a mut Context<'b>,
}

impl Selection<'_, '_> {
    fn admissible(&mut self, candidate: Candidate) -> bool {
        let edge = &self.edges[candidate.index];
        if let Some(failure) = self.cache.get(&edge.id) {
            self.ctx.stats.admissibility_cache_hits += 1;
            return failure.is_none();
        }
        let failure = self.ctx.first_invalid(edge);
        if failure.is_some() {
            self.ctx.stats.rejected_edges += 1;
        }
        self.cache.insert(edge.id, failure);
        failure.is_none()
    }

    fn cmp(&mut self, a: Candidate, b: Candidate) -> std::cmp::Ordering {
        self.ctx.cmp(
            self.edges[a.index].id,
            self.edges[b.index].id,
            Phase::Selection,
        )
    }

    /// Lemma 3.3: select minimum admissible representatives without checking
    /// admissibility on every edge. std's deterministic introselect partitions
    /// the lower half; no full weight sort is used here.
    fn select(&mut self, mut items: Vec<Candidate>, quota: usize) -> Vec<Candidate> {
        if quota == 0 || items.is_empty() {
            return Vec::new();
        }
        if items.len() <= quota {
            let mut best = BTreeMap::<usize, Candidate>::new();
            for item in items {
                if self.admissible(item)
                    && best
                        .get(&item.group)
                        .is_none_or(|&old| self.cmp(item, old).is_lt())
                {
                    best.insert(item.group, item);
                }
            }
            return best.into_values().collect();
        }
        let mid = items.len() / 2;
        items.select_nth_unstable_by(mid, |a, b| self.cmp(*a, *b));
        let upper = items.split_off(mid);
        let lower = items;
        let mut passed = BTreeSet::new();
        for &item in &lower {
            if !passed.contains(&item.group) && self.admissible(item) {
                passed.insert(item.group);
                if passed.len() == quota {
                    return self.select(lower, quota);
                }
            }
        }
        let mut answer = Vec::new();
        for group in &passed {
            answer.extend(
                self.select(
                    lower
                        .iter()
                        .copied()
                        .filter(|e| e.group == *group)
                        .collect(),
                    1,
                ),
            );
        }
        answer.extend(
            self.select(
                upper
                    .into_iter()
                    .filter(|e| !passed.contains(&e.group))
                    .collect(),
                quota - passed.len(),
            ),
        );
        answer
    }
}

pub(crate) fn process_batch(
    n: usize,
    edges: &[WorkEdge],
    spokes: &mut [Option<Spoke>],
    ctx: &mut Context<'_>,
) -> BatchResult {
    ctx.stats.batch_calls += 1;
    let r = ctx.options.portal_threshold;
    let quota = r * r;
    let mut incident = vec![Vec::new(); n];
    for (index, edge) in edges.iter().enumerate() {
        incident[edge.u].push(Candidate {
            index,
            group: edge.v,
        });
        incident[edge.v].push(Candidate {
            index,
            group: edge.u,
        });
    }
    let mut selection = Selection {
        edges,
        cache: BTreeMap::new(),
        ctx,
    };
    let selected: Vec<_> = incident
        .into_iter()
        .map(|items| selection.select(items, quota))
        .collect();
    let mut declared = BTreeMap::new();
    for set in &selected {
        for item in set {
            let edge = &edges[item.index];
            declared.insert(edge.id, edge.clone());
        }
    }
    let rejected = edges
        .iter()
        .filter_map(|edge| {
            selection
                .cache
                .get(&edge.id)
                .copied()
                .flatten()
                .map(|first_invalid| Rejected {
                    edge: edge.clone(),
                    first_invalid,
                })
        })
        .collect();
    let ctx = selection.ctx;
    ctx.stats.declared_edges += declared.len() as u64;

    // Greedy hitting set from Section 3.2. This direct implementation scans
    // incidences; its wall-clock complexity is not the theorem's target.
    let mut uncovered: BTreeSet<_> = (0..n).filter(|&v| selected[v].len() == quota).collect();
    let mut portals = Vec::new();
    loop {
        let mut count = vec![0; n];
        for &v in &uncovered {
            for item in &selected[v] {
                count[item.group] += 1;
            }
        }
        let Some(portal) = (0..n).find(|&p| count[p] >= r) else {
            break;
        };
        portals.push(portal);
        uncovered.retain(|&v| !selected[v].iter().any(|e| e.group == portal));
    }
    while let Some(&v) = uncovered.first() {
        let portal = selected[v].iter().map(|e| e.group).min().unwrap();
        portals.push(portal);
        uncovered.retain(|&u| !selected[u].iter().any(|e| e.group == portal));
    }
    ctx.stats.portals_selected += portals.len() as u64;
    for v in 0..n {
        if selected[v].len() != quota {
            continue;
        }
        let (portal, candidate) = portals
            .iter()
            .find_map(|&p| {
                selected[v]
                    .iter()
                    .find(|item| item.group == p)
                    .map(|item| (p, edges[item.index].id))
            })
            .expect("every full neighbor set is covered");
        if spokes[v].is_none_or(|old| ctx.cmp(candidate, old.edge, Phase::Spoke).is_lt()) {
            spokes[v] = Some(Spoke {
                portal,
                edge: candidate,
            });
            ctx.stats.spoke_updates += 1;
        }
    }
    BatchResult {
        declared: declared.into_values().collect(),
        rejected,
        portals,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Edge, Graph, Options, Stats, graph::Projection};

    #[test]
    fn batches_cover_or_validly_project_every_edge() {
        use crate::path::{PathMax, PathResult};
        use rand::{Rng, SeedableRng, rngs::SmallRng};
        let mut rng = SmallRng::seed_from_u64(20261124);
        for r in 1..=3 {
            for _ in 0..30 {
                let n = rng.random_range(4..24);
                let graph = Graph {
                    vertices: n,
                    edges: (0..180)
                        .map(|_| {
                            let u = rng.random_range(0..n);
                            let v = (u + rng.random_range(1..n)) % n;
                            Edge {
                                u,
                                v,
                                weight: rng.random_range(-8..=8),
                            }
                        })
                        .collect(),
                };
                let options = Options {
                    portal_threshold: r,
                    ..Options::default()
                };
                let mut ctx = Context {
                    graph: &graph,
                    options: &options,
                    stats: Stats::default(),
                };
                let work = graph.working_copy();
                let mut spokes = vec![None; n];
                let mut forest = Vec::new();
                for batch in work.edges.chunks(90) {
                    let update = process_batch(n, batch, &mut spokes, &mut ctx);
                    assert!(update.rejected.is_empty());
                    assert!(update.portals.len() <= 2 * n / r);
                    assert!(update.declared.len() <= r * r * n);
                    forest.extend(update.declared);
                    forest = ctx.exact_msf(n, forest);
                    let paths = PathMax::new(n, &forest, &mut ctx);
                    for edge in batch {
                        let supported = match paths.query(edge.u, edge.v, &mut ctx) {
                            PathResult::Max(id) => ctx.cmp(id, edge.id, Phase::Path).is_le(),
                            PathResult::Empty => true,
                            PathResult::Disconnected => false,
                        };
                        if !supported {
                            for v in [edge.u, edge.v] {
                                let spoke =
                                    spokes[v].expect("unsupported edge must have both spokes");
                                assert!(ctx.cmp(spoke.edge, edge.id, Phase::Spoke).is_le());
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn grouped_selection_skips_invalid_minima_and_caches_failures() {
        let graph = Graph {
            vertices: 5,
            edges: (0..12)
                .map(|i| Edge {
                    u: 0,
                    v: i % 4 + 1,
                    weight: i as i64,
                })
                .collect(),
        };
        let options = Options::default();
        for quota in 0..=6 {
            let mut ctx = Context {
                graph: &graph,
                options: &options,
                stats: Stats::default(),
            };
            let mut edges = graph.working_copy().edges;
            for e in edges.iter_mut().filter(|e| e.id % 3 == 0) {
                e.trail.push(Projection {
                    level: 0,
                    spokes: [11, 11],
                });
            }
            let items: Vec<_> = edges
                .iter()
                .enumerate()
                .map(|(index, e)| Candidate { index, group: e.v })
                .collect();
            let mut select = Selection {
                edges: &edges,
                cache: BTreeMap::new(),
                ctx: &mut ctx,
            };
            let mut actual: Vec<_> = select
                .select(items.clone(), quota)
                .iter()
                .map(|e| edges[e.index].id)
                .collect();
            actual.sort_unstable();
            let expected: Vec<_> = [1, 2, 4, 7].into_iter().take(quota).collect();
            assert_eq!(actual, expected, "quota {quota}");
            let again = select.select(items, quota);
            assert_eq!(again.len(), expected.len());
            if quota > 0 {
                assert!(select.ctx.stats.admissibility_cache_hits > 0);
            }
        }
    }
}
