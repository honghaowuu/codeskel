use std::collections::{HashMap, HashSet, VecDeque};

/// Directed graph where edge A → B means "A depends on B" (A imports B).
pub struct DepGraph {
    pub nodes: HashSet<String>,
    /// adjacency list: from → set of to
    pub edges: HashMap<String, HashSet<String>>,
}

impl DepGraph {
    pub fn new() -> Self {
        Self { nodes: HashSet::new(), edges: HashMap::new() }
    }

    pub fn add_node(&mut self, id: &str) {
        self.nodes.insert(id.to_string());
        self.edges.entry(id.to_string()).or_default();
    }

    /// Add directed edge: `from` depends on `to`
    pub fn add_edge(&mut self, from: &str, to: &str) {
        // Only add edge if both nodes exist (or add `to` as node)
        self.nodes.insert(from.to_string());
        self.nodes.insert(to.to_string());
        self.edges.entry(from.to_string()).or_default().insert(to.to_string());
        self.edges.entry(to.to_string()).or_default();
    }

    /// Topological sort using Kahn's algorithm.
    /// Returns (order, cycle_pairs) where:
    /// - `order` has each node appearing AFTER all its dependencies
    /// - `cycle_pairs` are (from, to) edges that were removed to break cycles
    pub fn topo_sort(&self) -> (Vec<String>, Vec<(String, String)>) {
        // We run Kahn's on the REVERSED graph.
        // In the reversed graph, edge B→A means "A depends on B".
        // A node with zero in-degree in the reversed graph has no dependents
        // blocking it, i.e. all of its own dependencies are satisfied first.
        //
        // Equivalently: track out-degree in the original graph.
        // A node with zero out-degree depends on nothing → emit first.

        // out_degree[n] = number of unprocessed nodes that `n` depends on
        let mut out_degree: HashMap<String, usize> = self.nodes.iter()
            .map(|n| (n.clone(), 0))
            .collect();

        // adj is a mutable copy of original edges (from → deps)
        let mut adj: HashMap<String, HashSet<String>> = self.edges.iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        // rev_adj[dep] = set of nodes that depend on dep (reverse edges)
        let mut rev_adj: HashMap<String, HashSet<String>> = self.nodes.iter()
            .map(|n| (n.clone(), HashSet::new()))
            .collect();

        for (from, deps) in &self.edges {
            *out_degree.entry(from.clone()).or_insert(0) += deps.len();
            for dep in deps {
                rev_adj.entry(dep.clone()).or_default().insert(from.clone());
            }
        }

        // Seed queue with zero-out-degree nodes (no dependencies) sorted for determinism
        let mut zero_out: Vec<String> = out_degree.iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(n, _)| n.clone())
            .collect();
        zero_out.sort();
        let mut queue: VecDeque<String> = zero_out.into();

        let mut order = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();
        let mut cycle_pairs: Vec<(String, String)> = Vec::new();

        while order.len() < self.nodes.len() {
            if let Some(node) = queue.pop_front() {
                if visited.contains(&node) { continue; }
                order.push(node.clone());
                visited.insert(node.clone());

                // For each node that depends on `node`, reduce its out_degree
                let mut dependents: Vec<String> = rev_adj.get(&node)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|n| !visited.contains(n))
                    .collect();
                dependents.sort();
                for dependent in dependents {
                    if let Some(deg) = out_degree.get_mut(&dependent) {
                        if *deg > 0 { *deg -= 1; }
                        if *deg == 0 {
                            queue.push_back(dependent);
                        }
                    }
                }
            } else {
                // Queue empty but not all nodes visited: cycle exists.
                // Find unvisited nodes sorted for determinism.
                let mut unvisited: Vec<String> = self.nodes.iter()
                    .filter(|n| !visited.contains(*n))
                    .cloned()
                    .collect();
                if unvisited.is_empty() { break; }
                unvisited.sort();

                // Pick the node with the highest number of unvisited outgoing edges
                // (most dependencies still pending) as the cycle breaker.
                let breaker = unvisited.iter().max_by_key(|n| {
                    adj.get(*n).map(|deps| {
                        deps.iter().filter(|d| !visited.contains(*d)).count()
                    }).unwrap_or(0)
                }).cloned().unwrap_or_else(|| unvisited[0].clone());

                // Remove one edge from breaker → an unvisited dependency
                let target = adj.get(&breaker)
                    .and_then(|deps| {
                        let mut unvisited_deps: Vec<String> = deps.iter()
                            .filter(|d| !visited.contains(*d))
                            .cloned()
                            .collect();
                        unvisited_deps.sort();
                        unvisited_deps.into_iter().next()
                    });

                if let Some(target) = target {
                    cycle_pairs.push((breaker.clone(), target.clone()));
                    // Remove the forward edge breaker → target
                    if let Some(deps) = adj.get_mut(&breaker) {
                        deps.remove(&target);
                    }
                    // Remove the reverse edge target → breaker
                    if let Some(dependents) = rev_adj.get_mut(&target) {
                        dependents.remove(&breaker);
                    }
                    // Reduce out_degree of breaker (one fewer unmet dependency)
                    if let Some(deg) = out_degree.get_mut(&breaker) {
                        if *deg > 0 { *deg -= 1; }
                        if *deg == 0 && !visited.contains(&breaker) {
                            queue.push_back(breaker);
                        }
                    }
                } else {
                    // Breaker has no outgoing unvisited edges; just emit it
                    order.push(breaker.clone());
                    visited.insert(breaker);
                }
            }
        }

        (order, cycle_pairs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_simple_topo() {
        let mut g = DepGraph::new();
        g.add_node("A"); g.add_node("B"); g.add_node("C");
        g.add_edge("C", "B"); // C depends on B
        g.add_edge("B", "A"); // B depends on A
        let (order, cycles) = g.topo_sort();
        assert!(cycles.is_empty(), "no cycles expected");
        assert_eq!(order.len(), 3);
        let pos: HashMap<_, _> = order.iter().enumerate().map(|(i,n)|(n.clone(),i)).collect();
        // A before B (B depends on A)
        assert!(pos["A"] < pos["B"], "A must come before B");
        // B before C (C depends on B)
        assert!(pos["B"] < pos["C"], "B must come before C");
    }

    #[test]
    fn test_cycle_detection() {
        let mut g = DepGraph::new();
        g.add_node("A"); g.add_node("B");
        g.add_edge("A", "B");
        g.add_edge("B", "A");
        let (order, cycles) = g.topo_sort();
        assert!(!cycles.is_empty(), "should detect cycle");
        assert_eq!(order.len(), 2, "all nodes should appear in output");
    }

    #[test]
    fn test_independent_nodes() {
        let mut g = DepGraph::new();
        g.add_node("X"); g.add_node("Y"); g.add_node("Z");
        let (order, cycles) = g.topo_sort();
        assert!(cycles.is_empty());
        assert_eq!(order.len(), 3);
    }

    #[test]
    fn test_diamond_dependency() {
        //   D
        //  / \
        // B   C
        //  \ /
        //   A
        // D depends on B and C, both depend on A
        let mut g = DepGraph::new();
        g.add_edge("D", "B");
        g.add_edge("D", "C");
        g.add_edge("B", "A");
        g.add_edge("C", "A");
        let (order, cycles) = g.topo_sort();
        assert!(cycles.is_empty(), "diamond has no cycle");
        assert_eq!(order.len(), 4);
        let pos: HashMap<_, _> = order.iter().enumerate().map(|(i,n)|(n.clone(),i)).collect();
        assert!(pos["A"] < pos["B"]);
        assert!(pos["A"] < pos["C"]);
        assert!(pos["B"] < pos["D"] || pos["C"] < pos["D"]);
    }

    #[test]
    fn test_single_node() {
        let mut g = DepGraph::new();
        g.add_node("A");
        let (order, cycles) = g.topo_sort();
        assert!(cycles.is_empty());
        assert_eq!(order, vec!["A"]);
    }
}
