use std::collections::{HashMap, HashSet};

use daggy::petgraph::Direction;
use daggy::petgraph::visit::IntoNeighborsDirected;
use daggy::petgraph::visit::Topo;
use daggy::{Dag, NodeIndex};

use super::diagnostics::CompositionIssue;
use super::model::Registration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DependencyEdge;

pub type RegistrationDag = Dag<Registration, DependencyEdge>;

pub fn build_graph(
    registrations: &[Registration],
    dependencies: &[(u32, u32)],
) -> Result<RegistrationDag, CompositionIssue> {
    let mut dag: RegistrationDag = Dag::new();
    let mut node_by_registration = HashMap::new();
    let span_by_id: HashMap<u32, _> =
        registrations.iter().map(|registration| (registration.id, registration.span)).collect();

    for registration in registrations {
        let node = dag.add_node(registration.clone());
        node_by_registration.insert(registration.id, node);
    }

    for (from, to) in dependencies {
        let from_node = node_by_registration.get(from).copied().ok_or_else(|| {
            CompositionIssue::UnknownRegistrationId { registration_id: *from, span: span_by_id.get(from).copied() }
        })?;
        let to_node = node_by_registration.get(to).copied().ok_or_else(|| CompositionIssue::UnknownRegistrationId {
            registration_id: *to,
            span: span_by_id.get(to).copied(),
        })?;
        if creates_cycle(&dag, from_node, to_node) {
            return Err(CompositionIssue::DependencyCycle {
                from_id: *from,
                to_id: *to,
                span: span_by_id.get(from).copied(),
            });
        }
        dag.add_edge(from_node, to_node, DependencyEdge).ok(); // cycle already checked above
    }

    Ok(dag)
}

pub fn topo_registration_order(dag: &RegistrationDag) -> Vec<u32> {
    let mut topo = Topo::new(dag);
    let mut order = Vec::new();
    while let Some(node) = topo.next(dag) {
        order.push(dag[node].id);
    }
    order
}

fn creates_cycle(dag: &RegistrationDag, from: NodeIndex, to: NodeIndex) -> bool {
    if from == to {
        return true;
    }
    let mut stack = vec![to];
    let mut seen = HashSet::new();
    while let Some(node) = stack.pop() {
        if !seen.insert(node) {
            continue;
        }
        if node == from {
            return true;
        }
        for child in dag.neighbors_directed(node, Direction::Outgoing) {
            stack.push(child);
        }
    }
    false
}
