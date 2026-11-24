use serde::{Deserialize, Serialize};

/// Undirected edge. Its position in `Graph::edges` is its stable ID and tie breaker.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Edge {
    pub u: usize,
    pub v: usize,
    pub weight: i64,
}

/// Vertices are numbered `0..vertices`. Parallel edges and loops are accepted.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Graph {
    pub vertices: usize,
    pub edges: Vec<Edge>,
}

impl Graph {
    pub fn validate(&self) -> Result<(), String> {
        for (id, edge) in self.edges.iter().enumerate() {
            if edge.u >= self.vertices || edge.v >= self.vertices {
                return Err(format!(
                    "edge {id} has an endpoint outside 0..{}",
                    self.vertices
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn working_copy(&self) -> WorkGraph {
        WorkGraph {
            n: self.vertices,
            edges: self
                .edges
                .iter()
                .enumerate()
                .filter(|(_, e)| e.u != e.v)
                .map(|(id, e)| WorkEdge {
                    id,
                    u: e.u,
                    v: e.v,
                    trail: Vec::new(),
                })
                .collect(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Projection {
    pub level: usize,
    pub spokes: [usize; 2],
}

#[derive(Clone, Debug)]
pub(crate) struct WorkEdge {
    pub id: usize,
    pub u: usize,
    pub v: usize,
    pub trail: Vec<Projection>,
}

#[derive(Clone, Debug)]
pub(crate) struct WorkGraph {
    pub n: usize,
    pub edges: Vec<WorkEdge>,
}
