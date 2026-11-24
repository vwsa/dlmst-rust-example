//! Benchmark adapter: prepared inputs, warmups, rotating algorithm order,
//! identical tie-breaking and checked outputs. Does not change the solver.
use clap::Parser;
use dlmst::{Graph, Options, minimum_spanning_forest};
use petgraph::{
    algo::{connected_components, min_spanning_tree, min_spanning_tree_prim},
    data::Element,
    graph::UnGraph,
};
use serde_json::json;
use std::{collections::BTreeMap, fs, hint::black_box, path::PathBuf, time::Instant};

#[derive(Parser)]
struct Args {
    input: PathBuf,
    #[arg(long, default_value_t = 5)]
    repetitions: usize,
    #[arg(long, default_value_t = 1)]
    warmups: usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if args.repetitions == 0 {
        return Err("repetitions must be positive".into());
    }
    let graph: Graph = serde_json::from_str(&fs::read_to_string(args.input)?)?;
    graph.validate()?;
    let mut reference = UnGraph::<(), (i64, usize), usize>::default();
    let nodes: Vec<_> = (0..graph.vertices)
        .map(|_| reference.add_node(()))
        .collect();
    for (id, e) in graph.edges.iter().enumerate() {
        reference.add_edge(nodes[e.u], nodes[e.v], (e.weight, id));
    }
    // petgraph Prim only covers one component, so this comparison explicitly
    // requires connected graphs rather than reporting a partial forest as faster.
    if graph.vertices == 0 || connected_components(&reference) != 1 {
        return Err("comparison inputs must be nonempty and connected".into());
    }
    let options = Options::default();
    let mut expected = None;
    let mut statistics = None;
    let mut timings: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
    let names = ["dlmst_prototype", "petgraph_kruskal", "petgraph_prim"];
    for repetition in 0..args.warmups + args.repetitions {
        for offset in 0..names.len() {
            let algorithm = (offset + repetition) % names.len();
            let started = Instant::now();
            let (mut ids, stats) = match algorithm {
                0 => {
                    let report = minimum_spanning_forest(black_box(&graph), &options)?;
                    (report.edge_ids, Some(report.stats))
                }
                1 => (
                    min_spanning_tree(black_box(&reference))
                        .filter_map(edge_id)
                        .collect(),
                    None,
                ),
                _ => (
                    min_spanning_tree_prim(black_box(&reference))
                        .filter_map(edge_id)
                        .collect(),
                    None,
                ),
            };
            ids.sort_unstable();
            black_box(&ids);
            let elapsed = started.elapsed().as_secs_f64() * 1000.0;
            if let Some(expected) = &expected {
                if &ids != expected {
                    return Err(format!("{} disagrees with the reference", names[algorithm]).into());
                }
            } else {
                expected = Some(ids);
            }
            if let Some(stats) = stats {
                statistics = Some(stats);
            }
            if repetition >= args.warmups {
                timings.entry(names[algorithm]).or_default().push(elapsed);
            }
        }
    }
    println!(
        "{}",
        serde_json::to_string(&json!({
            "vertices": graph.vertices, "input_edges": graph.edges.len(),
            "repetitions": args.repetitions, "warmups": args.warmups,
            "all_match": true, "edge_ids": expected, "timings_ms": timings,
            "prototype_stats": statistics, "options": options,
        }))?
    );
    Ok(())
}

fn edge_id(element: Element<(), (i64, usize)>) -> Option<usize> {
    match element {
        Element::Edge { weight, .. } => Some(weight.1),
        _ => None,
    }
}
