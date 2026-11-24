use crate::{
    batch::Spoke,
    graph::WorkEdge,
    path::{PathMax, PathResult},
    solver::{Context, Phase},
};
use std::collections::{BTreeMap, BTreeSet};

/// True means project. Missing spokes and coincident portals always mean find.
pub(crate) fn actions(
    n: usize,
    assigned: &[WorkEdge],
    forest: &[WorkEdge],
    spokes: &[Option<Spoke>],
    ctx: &mut Context<'_>,
) -> Vec<bool> {
    if ctx.options.enumerate_preferences {
        if let Some(estimate) = enumerate(n, assigned, forest, spokes, ctx) {
            return estimate;
        }
    }
    ctx.stats.exact_preference_rounds += 1;
    let paths = PathMax::new(n, forest, ctx);
    assigned
        .iter()
        .map(|e| exact(e, spokes, &paths, ctx))
        .collect()
}

fn exact(
    edge: &WorkEdge,
    spokes: &[Option<Spoke>],
    paths: &PathMax,
    ctx: &mut Context<'_>,
) -> bool {
    let (Some(a), Some(b)) = (spokes[edge.u], spokes[edge.v]) else {
        return false;
    };
    if a.portal == b.portal {
        return false;
    }
    match paths.query(edge.u, edge.v, ctx) {
        PathResult::Disconnected => true,
        PathResult::Empty => false,
        PathResult::Max(max) => {
            let heavier = if ctx.cmp(a.edge, b.edge, Phase::Spoke).is_ge() {
                a.edge
            } else {
                b.edge
            };
            ctx.cmp(max, heavier, Phase::Path).is_gt()
        }
    }
}

enum Rule {
    Find,
    Project,
    Compare {
        spokes: [usize; 2],
        path: Vec<usize>,
    },
}

/// Section 3.5 on bounded instances. The family is built from symbolic total
/// orders, never by reading actual weights. A minimum distinguishing subset is
/// exhaustively searched, then only its actual preferences are queried.
fn enumerate(
    n: usize,
    assigned: &[WorkEdge],
    forest: &[WorkEdge],
    spokes: &[Option<Spoke>],
    ctx: &mut Context<'_>,
) -> Option<Vec<bool>> {
    if assigned.len() > ctx.options.preference_edge_limit {
        return None;
    }
    let mut adj = vec![Vec::new(); n];
    for e in forest {
        adj[e.u].push((e.v, e.id));
        adj[e.v].push((e.u, e.id));
    }
    let mut keys = BTreeSet::new();
    let mut rules = Vec::new();
    for edge in assigned {
        let (Some(a), Some(b)) = (spokes[edge.u], spokes[edge.v]) else {
            rules.push(Rule::Find);
            continue;
        };
        if a.portal == b.portal {
            rules.push(Rule::Find);
            continue;
        }
        let mut parent = vec![None; n];
        parent[edge.u] = Some((edge.u, usize::MAX));
        let mut stack = vec![edge.u];
        while let Some(v) = stack.pop() {
            if v == edge.v {
                break;
            }
            for &(u, id) in &adj[v] {
                if parent[u].is_none() {
                    parent[u] = Some((v, id));
                    stack.push(u);
                }
            }
        }
        if parent[edge.v].is_none() {
            rules.push(Rule::Project);
            continue;
        }
        let mut path = Vec::new();
        let mut v = edge.v;
        while v != edge.u {
            let (u, id) = parent[v].unwrap();
            path.push(id);
            v = u;
        }
        keys.extend([a.edge, b.edge]);
        keys.extend(path.iter().copied());
        if keys.len() > ctx.options.preference_key_limit {
            return None;
        }
        rules.push(Rule::Compare {
            spokes: [a.edge, b.edge],
            path,
        });
    }
    let labels: BTreeMap<_, _> = keys.iter().enumerate().map(|(i, &id)| (id, i)).collect();
    for rule in &mut rules {
        if let Rule::Compare { spokes, path } = rule {
            for id in spokes {
                *id = labels[id];
            }
            for id in path {
                *id = labels[id];
            }
        }
    }
    ctx.stats.preference_enumerations += 1;
    let mut family = BTreeSet::new();
    let mut permutation: Vec<_> = (0..keys.len()).collect();
    orders(&mut permutation, 0, &mut |permutation| {
        ctx.stats.preference_permutations += 1;
        let mut rank = vec![0; permutation.len()];
        for (i, &key) in permutation.iter().enumerate() {
            rank[key] = i;
        }
        let mut vector = 0u16;
        for (i, rule) in rules.iter().enumerate() {
            let project = match rule {
                Rule::Find => false,
                Rule::Project => true,
                Rule::Compare { spokes, path } => {
                    let threshold = rank[spokes[0]].max(rank[spokes[1]]);
                    path.iter().any(|&key| rank[key] > threshold)
                }
            };
            if project {
                vector |= 1 << i;
            }
        }
        family.insert(vector);
    });
    let family: Vec<_> = family.into_iter().collect();
    ctx.stats.preference_vectors += family.len() as u64;
    // A resource bound on research enumeration, not an algorithmic claim.
    if family.len() > 256 {
        ctx.stats.preference_budget_fallbacks += 1;
        return None;
    }
    let allowed_errors = assigned.len() / ctx.options.blocks;
    let mut differences = BTreeSet::new();
    for (i, &a) in family.iter().enumerate() {
        for &b in &family[..i] {
            let difference = a ^ b;
            if difference.count_ones() as usize > allowed_errors {
                differences.insert(difference);
            }
        }
    }
    let mut checks = 0;
    let mut sample = None;
    'search: for size in 0..=assigned.len() {
        for mask in 0u32..(1u32 << assigned.len()) {
            if mask.count_ones() as usize != size {
                continue;
            }
            ctx.stats.distinguishing_subsets_checked += 1;
            let mut separates = true;
            for &difference in &differences {
                checks += 1;
                ctx.stats.distinguishing_pair_checks += 1;
                if checks > 1_000_000 {
                    ctx.stats.preference_budget_fallbacks += 1;
                    return None;
                }
                if (mask as u16 & difference) == 0 {
                    separates = false;
                    break;
                }
            }
            if separates {
                sample = Some(mask as u16);
                break 'search;
            }
        }
    }
    let sample = sample.expect("querying every coordinate separates all vectors");
    let mut observed = 0u16;
    if sample != 0 {
        let paths = PathMax::new(n, forest, ctx);
        for (i, edge) in assigned.iter().enumerate() {
            if sample & (1 << i) != 0 {
                ctx.stats.sampled_preference_queries += 1;
                if exact(edge, spokes, &paths, ctx) {
                    observed |= 1 << i;
                }
            }
        }
    }
    let estimate = family
        .into_iter()
        .find(|a| a & sample == observed)
        .expect("the actual edge order is one of the symbolic total orders");
    Some(
        (0..assigned.len())
            .map(|i| estimate & (1 << i) != 0)
            .collect(),
    )
}

fn orders(values: &mut [usize], at: usize, visit: &mut impl FnMut(&[usize])) {
    if at == values.len() {
        visit(values);
        return;
    }
    for i in at..values.len() {
        values.swap(at, i);
        orders(values, at + 1, visit);
        values.swap(at, i);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Edge, Graph, Options, Stats};

    #[test]
    fn estimates_obey_hamming_bound_for_every_ground_order() {
        let mut graph = Graph {
            vertices: 4,
            edges: vec![
                Edge {
                    u: 0,
                    v: 1,
                    weight: 0,
                },
                Edge {
                    u: 1,
                    v: 2,
                    weight: 1,
                },
                Edge {
                    u: 2,
                    v: 3,
                    weight: 2,
                },
                Edge {
                    u: 0,
                    v: 2,
                    weight: 3,
                },
                Edge {
                    u: 1,
                    v: 3,
                    weight: 4,
                },
            ],
        };
        let options = Options {
            blocks: 2,
            ..Options::default()
        };
        let work = graph.working_copy();
        let forest = &work.edges[..3];
        let spokes = [
            Some(Spoke { portal: 1, edge: 0 }),
            Some(Spoke { portal: 3, edge: 4 }),
            Some(Spoke { portal: 0, edge: 3 }),
            Some(Spoke { portal: 2, edge: 2 }),
        ];
        let mut queried = 0;
        let mut approximate = 0;
        orders(&mut [0, 1, 2, 3, 4], 0, &mut |permutation| {
            for (rank, &id) in permutation.iter().enumerate() {
                graph.edges[id].weight = rank as i64;
            }
            let mut ctx = Context {
                graph: &graph,
                options: &options,
                stats: Stats::default(),
            };
            let estimate = enumerate(4, &work.edges, forest, &spokes, &mut ctx).unwrap();
            queried += ctx.stats.sampled_preference_queries;
            let paths = PathMax::new(4, forest, &mut ctx);
            let truth: Vec<_> = work
                .edges
                .iter()
                .map(|e| exact(e, &spokes, &paths, &mut ctx))
                .collect();
            let errors = estimate
                .iter()
                .zip(truth)
                .filter(|(a, b)| **a != *b)
                .count();
            assert!(errors <= work.edges.len() / options.blocks);
            approximate += errors;
        });
        assert!(queried > 0);
        assert!(
            approximate > 0,
            "exercise genuinely approximate preferences"
        );
    }
}
