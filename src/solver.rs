use crate::{
    Graph,
    certificate::certificate,
    graph::{WorkEdge, WorkGraph},
    path::{PathMax, PathResult},
};
use petgraph::unionfind::UnionFind;
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, collections::BTreeMap};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Schedule {
    #[default]
    RoundRobin,
    Contiguous,
}

/// Small experimental parameters, deliberately different from the proof's constants.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    pub boruvka_steps: usize,
    pub blocks: usize,
    pub portal_threshold: usize,
    pub max_certificate_depth: usize,
    pub base_case_edges: usize,
    pub schedule: Schedule,
    /// Exhaustive preference families are restricted to small instances; larger
    /// instances use exact preferences and record that substitution.
    pub enumerate_preferences: bool,
    pub preference_key_limit: usize,
    pub preference_edge_limit: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            boruvka_steps: 1,
            blocks: 3,
            portal_threshold: 2,
            max_certificate_depth: 8,
            base_case_edges: 8,
            schedule: Schedule::RoundRobin,
            enumerate_preferences: true,
            preference_key_limit: 7,
            preference_edge_limit: 12,
        }
    }
}

impl Options {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if !(1..=64).contains(&self.blocks) {
            return Err("blocks must be in 1..=64 (there are blocks squared rounds)".into());
        }
        if self.portal_threshold == 0
            || self
                .portal_threshold
                .checked_mul(self.portal_threshold)
                .is_none()
        {
            return Err("portal_threshold must be positive and its square must fit usize".into());
        }
        if self.max_certificate_depth > 64 {
            return Err("max_certificate_depth must be at most 64".into());
        }
        if self.preference_key_limit > 8 || self.preference_edge_limit > 16 {
            return Err("enumeration limits must be at most 8 keys and 16 edges".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
pub struct ComparisonCounts {
    pub boruvka: u64,
    pub exact_msf: u64,
    pub selection: u64,
    pub admissibility: u64,
    pub path: u64,
    pub spoke: u64,
}

impl ComparisonCounts {
    pub fn total(&self) -> u64 {
        self.boruvka + self.exact_msf + self.selection + self.admissibility + self.path + self.spoke
    }
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
pub struct Stats {
    pub comparisons: ComparisonCounts,
    pub msf_calls: u64,
    pub boruvka_phases: u64,
    pub contracted_edges: u64,
    pub certificate_calls: u64,
    pub max_certificate_depth: usize,
    pub batch_calls: u64,
    pub declared_edges: u64,
    pub portals_selected: u64,
    pub spoke_updates: u64,
    pub projected_edges: u64,
    pub lifted_forest_edges: u64,
    pub rejected_edges: u64,
    pub rejected_edges_absorbed: u64,
    pub rejected_edges_propagated: u64,
    pub admissibility_tests: u64,
    pub admissibility_cache_hits: u64,
    pub path_queries: u64,
    pub filtered_edges: u64,
    pub ignored_self_loops: u64,
    /// Includes local exact MSF calls in the certificate and explicit fallback calls.
    pub exact_msf_calls: u64,
    pub exact_msf_input_edges: u64,
    pub base_case_calls: u64,
    pub no_progress_fallbacks: u64,
    pub depth_limit_fallbacks: u64,
    pub preference_enumerations: u64,
    pub preference_permutations: u64,
    pub preference_vectors: u64,
    pub distinguishing_subsets_checked: u64,
    pub distinguishing_pair_checks: u64,
    pub preference_budget_fallbacks: u64,
    pub sampled_preference_queries: u64,
    pub exact_preference_rounds: u64,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub mode: &'static str,
    pub edge_ids: Vec<usize>,
    pub total_weight: i128,
    pub components: usize,
    pub weight_comparisons: u64,
    pub options: Options,
    pub stats: Stats,
}

#[derive(Clone, Copy)]
pub(crate) enum Phase {
    Boruvka,
    Exact,
    Selection,
    Admissibility,
    Path,
    Spoke,
}

pub(crate) struct Context<'a> {
    pub graph: &'a Graph,
    pub options: &'a Options,
    pub stats: Stats,
}

impl Context<'_> {
    /// All solver reads of edge weights pass through this function. Every call
    /// counts once, including ties; IDs break equal-weight ties consistently.
    pub(crate) fn cmp(&mut self, a: usize, b: usize, phase: Phase) -> Ordering {
        let counts = &mut self.stats.comparisons;
        *match phase {
            Phase::Boruvka => &mut counts.boruvka,
            Phase::Exact => &mut counts.exact_msf,
            Phase::Selection => &mut counts.selection,
            Phase::Admissibility => &mut counts.admissibility,
            Phase::Path => &mut counts.path,
            Phase::Spoke => &mut counts.spoke,
        } += 1;
        self.graph.edges[a]
            .weight
            .cmp(&self.graph.edges[b].weight)
            .then(a.cmp(&b))
    }

    pub(crate) fn heavier(&mut self, a: Option<usize>, b: Option<usize>) -> Option<usize> {
        match (a, b) {
            (Some(a), Some(b)) => Some(if self.cmp(a, b, Phase::Path).is_ge() {
                a
            } else {
                b
            }),
            (a, b) => a.or(b),
        }
    }

    pub(crate) fn first_invalid(&mut self, edge: &WorkEdge) -> Option<usize> {
        self.stats.admissibility_tests += 1;
        for entry in &edge.trail {
            for spoke in entry.spokes {
                if self.cmp(spoke, edge.id, Phase::Admissibility).is_gt() {
                    return Some(entry.level);
                }
            }
        }
        None
    }

    pub(crate) fn exact_msf(&mut self, n: usize, mut edges: Vec<WorkEdge>) -> Vec<WorkEdge> {
        edges.sort_unstable_by_key(|e| e.id);
        edges.dedup_by_key(|e| e.id);
        self.stats.exact_msf_calls += 1;
        self.stats.exact_msf_input_edges += edges.len() as u64;
        edges.sort_unstable_by(|a, b| self.cmp(a.id, b.id, Phase::Exact));
        let mut sets = UnionFind::<usize>::new(n);
        edges.into_iter().filter(|e| sets.union(e.u, e.v)).collect()
    }
}

/// Returns an exact MSF, using DLMST-inspired certificates to safely filter edges.
/// Correctness does not require the experimental schedule to achieve good shrinkage.
pub fn minimum_spanning_forest(graph: &Graph, options: &Options) -> Result<Report, String> {
    graph.validate()?;
    options.validate()?;
    let mut ctx = Context {
        graph,
        options,
        stats: Stats::default(),
    };
    let work = graph.working_copy();
    ctx.stats.ignored_self_loops = (graph.edges.len() - work.edges.len()) as u64;
    let mut edge_ids = solve(work, &mut ctx);
    edge_ids.sort_unstable();
    Ok(Report {
        mode: "experimental_certificate_not_linear_time",
        total_weight: edge_ids
            .iter()
            .map(|&id| i128::from(graph.edges[id].weight))
            .sum(),
        components: graph.vertices - edge_ids.len(),
        weight_comparisons: ctx.stats.comparisons.total(),
        edge_ids,
        options: options.clone(),
        stats: ctx.stats,
    })
}

fn solve(mut graph: WorkGraph, ctx: &mut Context<'_>) -> Vec<usize> {
    // Iterate the outer MSF recursion so an unlucky experimental schedule cannot
    // consume one stack frame per edge. Certificate recursion has its own bound.
    let mut answer = Vec::new();
    loop {
        ctx.stats.msf_calls += 1;
        if graph.edges.is_empty() {
            return answer;
        }
        if graph.edges.len() <= ctx.options.base_case_edges {
            ctx.stats.base_case_calls += 1;
            answer.extend(ctx.exact_msf(graph.n, graph.edges).iter().map(|e| e.id));
            return answer;
        }
        for _ in 0..ctx.options.boruvka_steps {
            if graph.edges.is_empty() {
                break;
            }
            graph = boruvka(graph, &mut answer, ctx);
        }
        if graph.edges.is_empty() {
            return answer;
        }
        let result = certificate(&graph, 0, ctx);
        assert!(
            result.rejected.is_empty(),
            "root edges have no projection history"
        );
        let paths = PathMax::new(graph.n, &result.forest, ctx);
        let retained: std::collections::BTreeSet<_> = result.forest.iter().map(|e| e.id).collect();
        let old_len = graph.edges.len();
        graph.edges.retain(|e| {
            if retained.contains(&e.id) {
                return true;
            }
            let supported = match paths.query(e.u, e.v, ctx) {
                PathResult::Disconnected => false,
                PathResult::Empty => true,
                PathResult::Max(id) => ctx.cmp(id, e.id, Phase::Path).is_le(),
            };
            !supported
        });
        ctx.stats.filtered_edges += (old_len - graph.edges.len()) as u64;
        if graph.edges.len() == old_len {
            ctx.stats.no_progress_fallbacks += 1;
            answer.extend(ctx.exact_msf(graph.n, graph.edges).iter().map(|e| e.id));
            return answer;
        }
    }
}

fn boruvka(graph: WorkGraph, answer: &mut Vec<usize>, ctx: &mut Context<'_>) -> WorkGraph {
    ctx.stats.boruvka_phases += 1;
    let mut cheapest: Vec<Option<usize>> = vec![None; graph.n];
    for (i, edge) in graph.edges.iter().enumerate() {
        for v in [edge.u, edge.v] {
            if cheapest[v]
                .is_none_or(|j| ctx.cmp(edge.id, graph.edges[j].id, Phase::Boruvka).is_lt())
            {
                cheapest[v] = Some(i);
            }
        }
    }
    let mut sets = UnionFind::<usize>::new(graph.n);
    for i in cheapest.into_iter().flatten() {
        let edge = &graph.edges[i];
        if sets.union(edge.u, edge.v) {
            answer.push(edge.id);
            ctx.stats.contracted_edges += 1;
        }
    }
    let mut labels = BTreeMap::new();
    let mut edges = Vec::new();
    for mut edge in graph.edges {
        let u = sets.find_mut(edge.u);
        let v = sets.find_mut(edge.v);
        if u == v {
            continue;
        }
        let next = labels.len();
        edge.u = *labels.entry(u).or_insert(next);
        let next = labels.len();
        edge.v = *labels.entry(v).or_insert(next);
        edges.push(edge);
    }
    WorkGraph {
        n: labels.len(),
        edges,
    }
}
