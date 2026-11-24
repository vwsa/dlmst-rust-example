use crate::{graph::WorkEdge, solver::Context};

pub(crate) enum PathResult {
    Disconnected,
    Empty,
    Max(usize),
}

/// Conventional binary lifting. This is not the paper's Komlos comparison bound.
pub(crate) struct PathMax {
    roots: Vec<usize>,
    depth: Vec<usize>,
    up: Vec<Vec<usize>>,
    max: Vec<Vec<Option<usize>>>,
}

impl PathMax {
    pub(crate) fn new(n: usize, forest: &[WorkEdge], ctx: &mut Context<'_>) -> Self {
        let levels = (usize::BITS - n.max(1).leading_zeros()) as usize;
        let mut result = Self {
            roots: vec![usize::MAX; n],
            depth: vec![0; n],
            up: vec![vec![0; n]; levels],
            max: vec![vec![None; n]; levels],
        };
        let mut adj = vec![Vec::new(); n];
        for edge in forest {
            adj[edge.u].push((edge.v, edge.id));
            adj[edge.v].push((edge.u, edge.id));
        }
        for root in 0..n {
            if result.roots[root] != usize::MAX {
                continue;
            }
            result.roots[root] = root;
            result.up[0][root] = root;
            let mut stack = vec![root];
            while let Some(v) = stack.pop() {
                for &(u, id) in &adj[v] {
                    if result.roots[u] != usize::MAX {
                        continue;
                    }
                    result.roots[u] = root;
                    result.depth[u] = result.depth[v] + 1;
                    result.up[0][u] = v;
                    result.max[0][u] = Some(id);
                    stack.push(u);
                }
            }
        }
        for k in 1..levels {
            for v in 0..n {
                let parent = result.up[k - 1][v];
                result.up[k][v] = result.up[k - 1][parent];
                result.max[k][v] = ctx.heavier(result.max[k - 1][v], result.max[k - 1][parent]);
            }
        }
        result
    }

    pub(crate) fn query(&self, mut u: usize, mut v: usize, ctx: &mut Context<'_>) -> PathResult {
        ctx.stats.path_queries += 1;
        if self.roots[u] != self.roots[v] {
            return PathResult::Disconnected;
        }
        if u == v {
            return PathResult::Empty;
        }
        let mut answer = None;
        if self.depth[u] < self.depth[v] {
            std::mem::swap(&mut u, &mut v);
        }
        let delta = self.depth[u] - self.depth[v];
        for k in 0..self.up.len() {
            if delta & (1 << k) != 0 {
                answer = ctx.heavier(answer, self.max[k][u]);
                u = self.up[k][u];
            }
        }
        if u != v {
            for k in (0..self.up.len()).rev() {
                if self.up[k][u] != self.up[k][v] {
                    answer = ctx.heavier(answer, self.max[k][u]);
                    answer = ctx.heavier(answer, self.max[k][v]);
                    u = self.up[k][u];
                    v = self.up[k][v];
                }
            }
            answer = ctx.heavier(answer, self.max[0][u]);
            answer = ctx.heavier(answer, self.max[0][v]);
        }
        PathResult::Max(answer.expect("a nonempty connected forest path"))
    }
}
