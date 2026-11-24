use dlmst::{Edge, Graph, Options, Schedule, minimum_spanning_forest};
use petgraph::{algo::min_spanning_tree, data::Element, graph::UnGraph};
use proptest::prelude::*;

fn reference(graph: &Graph) -> Vec<usize> {
    let mut g = UnGraph::<(), (i64, usize), usize>::default();
    let nodes: Vec<_> = (0..graph.vertices).map(|_| g.add_node(())).collect();
    for (id, edge) in graph.edges.iter().enumerate() {
        g.add_edge(nodes[edge.u], nodes[edge.v], (edge.weight, id));
    }
    let mut result: Vec<_> = min_spanning_tree(&g)
        .filter_map(|element| match element {
            Element::Edge { weight, .. } => Some(weight.1),
            _ => None,
        })
        .collect();
    result.sort_unstable();
    result
}

fn forced() -> Options {
    Options {
        boruvka_steps: 0,
        base_case_edges: 0,
        enumerate_preferences: false,
        ..Options::default()
    }
}

fn assert_msf(graph: &Graph, options: &Options) {
    let report = minimum_spanning_forest(graph, options).unwrap();
    assert_eq!(report.edge_ids, reference(graph));
    assert_eq!(report.components, graph.vertices - report.edge_ids.len());
    assert_eq!(
        report.total_weight,
        report
            .edge_ids
            .iter()
            .map(|&i| i128::from(graph.edges[i].weight))
            .sum::<i128>()
    );
    assert_eq!(report.weight_comparisons, report.stats.comparisons.total());
    assert_eq!(
        report.stats.rejected_edges,
        report.stats.rejected_edges_absorbed
    );
    // Independent cycle-property check using a plain DFS and the public input.
    let mut adj = vec![Vec::new(); graph.vertices];
    for &id in &report.edge_ids {
        let e = &graph.edges[id];
        adj[e.u].push((e.v, id));
        adj[e.v].push((e.u, id));
    }
    for (id, edge) in graph.edges.iter().enumerate() {
        if edge.u == edge.v {
            continue;
        }
        let mut stack = vec![(edge.u, None::<(i64, usize)>)];
        let mut seen = vec![false; graph.vertices];
        let mut path = None;
        while let Some((v, maximum)) = stack.pop() {
            if seen[v] {
                continue;
            }
            seen[v] = true;
            if v == edge.v {
                path = maximum;
                break;
            }
            for &(u, i) in &adj[v] {
                let key = (graph.edges[i].weight, i);
                stack.push((u, Some(maximum.map_or(key, |m| m.max(key)))));
            }
        }
        assert!(
            path.is_some_and(|max| max <= (edge.weight, id)),
            "cycle property failed on edge {id}"
        );
    }
}

#[test]
fn edge_cases_and_extreme_weights() {
    for graph in [
        Graph {
            vertices: 0,
            edges: vec![],
        },
        Graph {
            vertices: 12,
            edges: vec![],
        },
        Graph {
            vertices: 1,
            edges: vec![Edge {
                u: 0,
                v: 0,
                weight: i64::MIN,
            }],
        },
        Graph {
            vertices: 7,
            edges: vec![
                Edge {
                    u: 0,
                    v: 1,
                    weight: i64::MIN,
                },
                Edge {
                    u: 1,
                    v: 2,
                    weight: i64::MIN,
                },
                Edge {
                    u: 0,
                    v: 2,
                    weight: i64::MAX,
                },
                Edge {
                    u: 3,
                    v: 4,
                    weight: i64::MAX,
                },
                Edge {
                    u: 3,
                    v: 4,
                    weight: i64::MAX,
                },
                Edge {
                    u: 4,
                    v: 4,
                    weight: i64::MIN,
                },
            ],
        },
    ] {
        for options in [
            Options::default(),
            forced(),
            Options {
                max_certificate_depth: 0,
                ..forced()
            },
        ] {
            assert_msf(&graph, &options);
        }
    }
}

#[test]
fn invalid_input_and_parameters_return_errors() {
    assert!(
        minimum_spanning_forest(
            &Graph {
                vertices: 0,
                edges: vec![Edge {
                    u: 0,
                    v: 0,
                    weight: 1
                }]
            },
            &Options::default()
        )
        .is_err()
    );
    let g = Graph {
        vertices: 0,
        edges: vec![],
    };
    for options in [
        Options {
            blocks: 0,
            ..Options::default()
        },
        Options {
            blocks: 65,
            ..Options::default()
        },
        Options {
            portal_threshold: 0,
            ..Options::default()
        },
        Options {
            portal_threshold: usize::MAX,
            ..Options::default()
        },
        Options {
            max_certificate_depth: 65,
            ..Options::default()
        },
        Options {
            preference_key_limit: 9,
            ..Options::default()
        },
        Options {
            preference_edge_limit: 17,
            ..Options::default()
        },
    ] {
        assert!(minimum_spanning_forest(&g, &options).is_err());
    }
}

#[test]
fn exhaust_all_four_vertex_graphs_with_two_weights() {
    let pairs = [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)];
    for mut code in 0..3usize.pow(6) {
        let mut edges = Vec::new();
        for (u, v) in pairs {
            match code % 3 {
                0 => (),
                value => edges.push(Edge {
                    u,
                    v,
                    weight: value as i64 - 2,
                }),
            }
            code /= 3;
        }
        let graph = Graph { vertices: 4, edges };
        let report = minimum_spanning_forest(&graph, &forced()).unwrap();
        assert_msf(&graph, &forced());
        // Independently enumerate all subsets with the required connectivity.
        let mut best = None;
        for mask in 0u64..(1 << graph.edges.len()) {
            if mask.count_ones() as usize != report.edge_ids.len() {
                continue;
            }
            let mut labels: Vec<_> = (0..4).collect();
            let mut weight = 0i128;
            let mut acyclic = true;
            for (id, e) in graph.edges.iter().enumerate() {
                if mask & (1 << id) == 0 {
                    continue;
                }
                let (a, b) = (labels[e.u], labels[e.v]);
                if a == b {
                    acyclic = false;
                    break;
                }
                for label in &mut labels {
                    if *label == b {
                        *label = a;
                    }
                }
                weight += i128::from(e.weight);
            }
            if acyclic && graph.edges.iter().all(|e| labels[e.u] == labels[e.v]) {
                best = Some(best.map_or(weight, |w: i128| w.min(weight)));
            }
        }
        assert_eq!(Some(report.total_weight), best);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]
    #[test]
    fn multigraphs_match_reference(
        n in 1usize..32,
        raw in prop::collection::vec((0usize..32, 0usize..32, -20i64..21), 0..220),
        blocks in 1usize..5,
        threshold in 1usize..4,
        boruvka in 0usize..3,
        contiguous in any::<bool>(),
    ) {
        let graph = Graph { vertices: n, edges: raw.into_iter().map(|(u,v,weight)| Edge { u: u%n, v: v%n, weight }).collect() };
        let options = Options { blocks, portal_threshold: threshold, boruvka_steps: boruvka,
            schedule: if contiguous { Schedule::Contiguous } else { Schedule::RoundRobin }, ..forced() };
        assert_msf(&graph, &options);
        let a = minimum_spanning_forest(&graph, &options).unwrap();
        let b = minimum_spanning_forest(&graph, &options).unwrap();
        prop_assert_eq!(a.edge_ids, b.edge_ids);
        prop_assert_eq!(a.stats, b.stats);
    }

    #[test]
    fn bounded_enumeration_preserves_msf(
        n in 2usize..10,
        raw in prop::collection::vec((0usize..10, 0usize..10, -4i64..5), 1..45),
    ) {
        let graph = Graph { vertices: n, edges: raw.into_iter().map(|(u,v,weight)| Edge { u: u%n, v: v%n, weight }).collect() };
        assert_msf(&graph, &Options { enumerate_preferences: true, preference_key_limit: 5,
            portal_threshold: 1, ..forced() });
    }
}
