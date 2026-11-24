use crate::{
    batch::{Spoke, process_batch},
    graph::{Projection, WorkEdge, WorkGraph},
    preference::actions,
    solver::{Context, Schedule},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub(crate) struct Rejected {
    pub edge: WorkEdge,
    pub first_invalid: usize,
}

pub(crate) struct Certificate {
    pub forest: Vec<WorkEdge>,
    pub rejected: Vec<Rejected>,
}

pub(crate) fn certificate(graph: &WorkGraph, level: usize, ctx: &mut Context<'_>) -> Certificate {
    let rounds = ctx.options.blocks * ctx.options.blocks;
    let schedule: Vec<_> = (0..graph.edges.len())
        .map(|i| match ctx.options.schedule {
            Schedule::RoundRobin => i % rounds,
            Schedule::Contiguous => {
                ((i as u128 * rounds as u128) / graph.edges.len().max(1) as u128) as usize
            }
        })
        .collect();
    run(graph, level, &schedule, ctx)
}

fn run(graph: &WorkGraph, level: usize, schedule: &[usize], ctx: &mut Context<'_>) -> Certificate {
    ctx.stats.certificate_calls += 1;
    ctx.stats.max_certificate_depth = ctx.stats.max_certificate_depth.max(level);
    let mut result = Certificate {
        forest: Vec::new(),
        rejected: Vec::new(),
    };
    if graph.edges.is_empty() {
        return result;
    }
    if level >= ctx.options.max_certificate_depth {
        ctx.stats.depth_limit_fallbacks += 1;
        let mut valid = Vec::new();
        for edge in &graph.edges {
            if let Some(first_invalid) = ctx.first_invalid(edge) {
                ctx.stats.rejected_edges += 1;
                result.rejected.push(Rejected {
                    edge: edge.clone(),
                    first_invalid,
                });
            } else {
                valid.push(edge.clone());
            }
        }
        result.forest = ctx.exact_msf(graph.n, valid);
        return result;
    }
    let t = ctx.options.blocks;
    let mut buckets = vec![Vec::new(); t * t];
    for (&round, edge) in schedule.iter().zip(&graph.edges) {
        buckets[round].push(edge.clone());
    }
    let original: BTreeMap<_, _> = graph.edges.iter().map(|e| (e.id, e)).collect();
    let mut spokes: Vec<Option<Spoke>> = vec![None; graph.n];
    let mut portals = BTreeSet::new();
    for block in 0..t {
        let mut batch = Vec::new();
        for round in 0..t {
            let assigned = &buckets[block * t + round];
            if assigned.is_empty() {
                continue;
            }
            let projects = actions(graph.n, assigned, &result.forest, &spokes, ctx);
            let portal_labels: BTreeMap<_, _> =
                portals.iter().enumerate().map(|(i, &p)| (p, i)).collect();
            let mut child = WorkGraph {
                n: portals.len(),
                edges: Vec::new(),
            };
            for (edge, project) in assigned.iter().zip(projects) {
                if !project {
                    batch.push(edge.clone());
                    continue;
                }
                let a = spokes[edge.u].expect("project needs two spokes");
                let b = spokes[edge.v].expect("project needs two spokes");
                assert_ne!(a.portal, b.portal);
                let mut projected = edge.clone();
                projected.u = portal_labels[&a.portal];
                projected.v = portal_labels[&b.portal];
                projected.trail.push(Projection {
                    level,
                    spokes: [a.edge, b.edge],
                });
                child.edges.push(projected);
                ctx.stats.projected_edges += 1;
            }
            if !child.edges.is_empty() {
                let child_result = certificate(&child, level + 1, ctx);
                let mut merge = std::mem::take(&mut result.forest);
                for edge in child_result.forest {
                    merge.push(original[&edge.id].clone());
                    ctx.stats.lifted_forest_edges += 1;
                }
                for rejected in child_result.rejected {
                    let lifted = original[&rejected.edge.id].clone();
                    if rejected.first_invalid == level {
                        merge.push(lifted);
                        ctx.stats.rejected_edges_absorbed += 1;
                    } else {
                        assert!(rejected.first_invalid < level);
                        result.rejected.push(Rejected {
                            edge: lifted,
                            first_invalid: rejected.first_invalid,
                        });
                        ctx.stats.rejected_edges_propagated += 1;
                    }
                }
                result.forest = ctx.exact_msf(graph.n, merge);
            }
        }
        if !batch.is_empty() {
            let update = process_batch(graph.n, &batch, &mut spokes, ctx);
            result.forest.extend(update.declared);
            result.forest = ctx.exact_msf(graph.n, result.forest);
            result.rejected.extend(update.rejected);
            portals.extend(update.portals);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Edge, Graph, Options, Stats};

    #[test]
    fn invalid_projection_returns_through_two_levels_to_its_origin() {
        // First block establishes four disjoint pairs and their spokes. The
        // second block projects three edges into one child. That child forms
        // two pairs before projecting the final, inadmissible edge again.
        let graph = Graph {
            vertices: 8,
            edges: vec![
                Edge {
                    u: 0,
                    v: 1,
                    weight: 10,
                },
                Edge {
                    u: 2,
                    v: 3,
                    weight: 10,
                },
                Edge {
                    u: 4,
                    v: 5,
                    weight: 10,
                },
                Edge {
                    u: 6,
                    v: 7,
                    weight: 10,
                },
                Edge {
                    u: 0,
                    v: 2,
                    weight: 20,
                },
                Edge {
                    u: 4,
                    v: 6,
                    weight: 20,
                },
                Edge {
                    u: 0,
                    v: 4,
                    weight: 1,
                },
            ],
        };
        let options = Options {
            blocks: 2,
            portal_threshold: 1,
            enumerate_preferences: false,
            ..Options::default()
        };
        let mut ctx = Context {
            graph: &graph,
            options: &options,
            stats: Stats::default(),
        };
        let result = run(&graph.working_copy(), 0, &[0, 0, 0, 0, 2, 2, 2], &mut ctx);
        assert!(result.rejected.is_empty());
        let mut ids: Vec<_> = result.forest.iter().map(|e| e.id).collect();
        ids.sort_unstable();
        assert_eq!(ids, (0..7).collect::<Vec<_>>());
        assert_eq!(ctx.stats.projected_edges, 4);
        assert_eq!(ctx.stats.max_certificate_depth, 2);
        assert_eq!(ctx.stats.rejected_edges, 1);
        assert_eq!(ctx.stats.rejected_edges_propagated, 1);
        assert_eq!(ctx.stats.rejected_edges_absorbed, 1);
        assert!(ctx.stats.lifted_forest_edges >= 2);
        assert_eq!(ctx.stats.depth_limit_fallbacks, 0);
        assert!(result.forest.iter().all(|e| e.trail.is_empty()));
    }
}
