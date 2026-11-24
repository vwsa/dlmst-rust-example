use clap::{Args, Parser, Subcommand};
use dlmst::{Edge, Graph, Options, Schedule, minimum_spanning_forest};
use petgraph::{algo::min_spanning_tree, data::Element, graph::UnGraph};
use rand::{Rng, SeedableRng, rngs::SmallRng};
use serde_json::json;
use std::{
    fs,
    io::{self, Read},
    path::PathBuf,
    time::Instant,
};

#[derive(Parser)]
#[command(
    version,
    about = "Experimental DLMST certificate reproduction (no linear-time claim)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Solve a JSON graph; use '-' for stdin. IDs are positions in the input edge array.
    Solve {
        #[arg(default_value = "-")]
        input: PathBuf,
        #[command(flatten)]
        experiment: Experiment,
        /// Compare the full edge-ID set with petgraph's Kruskal implementation.
        #[arg(long)]
        verify: bool,
    },
    /// Generate a seeded multigraph and compare with petgraph.
    Demo {
        #[arg(long, default_value_t = 64)]
        vertices: usize,
        #[arg(long, default_value_t = 512)]
        edges: usize,
        #[arg(long, default_value_t = 42)]
        seed: u64,
        #[command(flatten)]
        experiment: Experiment,
    },
    /// Compare correctness, elapsed times and operation counts on seeded graphs.
    Bench {
        #[arg(long, value_delimiter = ',', default_value = "64,256,1024")]
        sizes: Vec<usize>,
        #[arg(long, default_value_t = 8)]
        edges_per_vertex: usize,
        #[arg(long, default_value_t = 3)]
        repetitions: usize,
        #[arg(long, default_value_t = 42)]
        seed: u64,
        #[command(flatten)]
        experiment: Experiment,
    },
}

#[derive(Args)]
struct Experiment {
    #[arg(long, default_value_t = 1)]
    boruvka_steps: usize,
    #[arg(long, default_value_t = 3)]
    blocks: usize,
    #[arg(long, default_value_t = 2)]
    portal_threshold: usize,
    #[arg(long, default_value_t = 8)]
    max_certificate_depth: usize,
    #[arg(long, default_value_t = 8)]
    base_case_edges: usize,
    #[arg(long, value_enum, default_value_t = Schedule::RoundRobin)]
    schedule: Schedule,
    /// Bypass bounded preference-family enumeration and query all actions exactly.
    #[arg(long)]
    exact_preferences: bool,
}

impl From<Experiment> for Options {
    fn from(e: Experiment) -> Self {
        Self {
            boruvka_steps: e.boruvka_steps,
            blocks: e.blocks,
            portal_threshold: e.portal_threshold,
            max_certificate_depth: e.max_certificate_depth,
            base_case_edges: e.base_case_edges,
            schedule: e.schedule,
            enumerate_preferences: !e.exact_preferences,
            ..Self::default()
        }
    }
}

fn reference(graph: &Graph) -> Vec<usize> {
    let mut g = UnGraph::<(), (i64, usize), usize>::default();
    let nodes: Vec<_> = (0..graph.vertices).map(|_| g.add_node(())).collect();
    for (id, edge) in graph.edges.iter().enumerate() {
        g.add_edge(nodes[edge.u], nodes[edge.v], (edge.weight, id));
    }
    let mut ids: Vec<_> = min_spanning_tree(&g)
        .filter_map(|e| match e {
            Element::Edge { weight, .. } => Some(weight.1),
            _ => None,
        })
        .collect();
    ids.sort_unstable();
    ids
}

fn generated(vertices: usize, edges: usize, seed: u64) -> Result<Graph, String> {
    if vertices == 0 && edges > 0 {
        return Err("nonempty edges require at least one vertex".into());
    }
    let mut rng = SmallRng::seed_from_u64(seed);
    Ok(Graph {
        vertices,
        edges: (0..edges)
            .map(|_| Edge {
                u: rng.random_range(0..vertices),
                v: rng.random_range(0..vertices),
                weight: rng.random_range(-1000..=1000),
            })
            .collect(),
    })
}

fn measured(graph: &Graph, options: &Options, verify: bool) -> Result<serde_json::Value, String> {
    let start = Instant::now();
    let report = minimum_spanning_forest(graph, options)?;
    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    let baseline = if verify {
        let start = Instant::now();
        let expected = reference(graph);
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        if expected != report.edge_ids {
            return Err("result disagrees with petgraph".into());
        }
        Some(json!({ "matches": true, "elapsed_ms": elapsed }))
    } else {
        None
    };
    Ok(
        json!({ "vertices": graph.vertices, "input_edges": graph.edges.len(),
        "elapsed_ms": elapsed_ms, "petgraph_reference": baseline, "result": report }),
    )
}

fn run() -> Result<(), String> {
    let value = match Cli::parse().command {
        Command::Solve {
            input,
            experiment,
            verify,
        } => {
            let text = if input.as_os_str() == "-" {
                let mut text = String::new();
                io::stdin()
                    .read_to_string(&mut text)
                    .map_err(|e| e.to_string())?;
                text
            } else {
                fs::read_to_string(input).map_err(|e| e.to_string())?
            };
            let graph: Graph = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            measured(&graph, &experiment.into(), verify)?
        }
        Command::Demo {
            vertices,
            edges,
            seed,
            experiment,
        } => {
            let graph = generated(vertices, edges, seed)?;
            let mut value = measured(&graph, &experiment.into(), true)?;
            value["seed"] = json!(seed);
            value
        }
        Command::Bench {
            sizes,
            edges_per_vertex,
            repetitions,
            seed,
            experiment,
        } => {
            if repetitions == 0 {
                return Err("repetitions must be positive".into());
            }
            let options = experiment.into();
            let mut rows = Vec::new();
            for n in sizes {
                let edges = n
                    .checked_mul(edges_per_vertex)
                    .ok_or("edge count overflow")?;
                for repetition in 0..repetitions {
                    let run_seed = seed.wrapping_add(repetition as u64);
                    let mut value = measured(&generated(n, edges, run_seed)?, &options, true)?;
                    value["seed"] = json!(run_seed);
                    value["result"].as_object_mut().unwrap().remove("edge_ids");
                    rows.push(value);
                }
            }
            json!({ "benchmark": "seeded_multigraphs", "rows": rows })
        }
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
