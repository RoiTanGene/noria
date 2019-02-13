use crate::controller::sql::mir::SqlToMirConverter;
use crate::controller::sql::query_graph::{JoinRef, QueryGraph, QueryGraphEdge};
use dataflow::ops::join::JoinType;
use mir::MirNodeRef;
use nom_sql::ConditionTree;
use std::collections::{HashMap, HashSet};


#[derive(Debug)]
struct JoinChain {
    tables: HashSet<String>,
    last_node: MirNodeRef,
}

impl JoinChain {
    pub fn merge_chain(self, other: JoinChain, last_node: MirNodeRef) -> JoinChain {
        let tables = self.tables.union(&other.tables).cloned().collect();

        JoinChain {
            tables: tables,
            last_node: last_node,
        }
    }

    pub fn has_table(&self, table: &String) -> bool {
        self.tables.contains(table)
    }
}

// Generate join nodes for the query.
// This is done by creating/merging join chains as each predicate is added.
// If a predicate's parent tables appear in a previous predicate, the
// current predicate is added to the on-going join chain of the previous
// predicate.
// If a predicate's parent tables haven't been used by any previous predicate,
// a new join chain is started for the current predicate. And we assume that
// a future predicate will bring these chains together.
pub fn make_joins(
    mir_converter: &SqlToMirConverter,
    name: &str,
    qg: &QueryGraph,
    node_for_rel: &HashMap<&str, MirNodeRef>,
    node_count: usize,
) -> Vec<MirNodeRef> {
    let mut join_nodes: Vec<MirNodeRef> = Vec::new();
    let mut join_chains = Vec::new();
    let mut node_count = node_count;

    for jref in qg.join_order.iter() {
        let (join_type, jp) = from_join_ref(jref, &qg);
        let (left_chain, second_chain) =
            pick_join_chains(&jref.src, &jref.dst, &mut join_chains, node_for_rel);

        match second_chain {
            Some(right_chain) => {
                let jn = mir_converter.make_join_node(
                    &format!("{}_n{}", name, node_count),
                    jp,
                    left_chain.last_node.clone(),
                    right_chain.last_node.clone(),
                    join_type,
                );

                // merge node chains
                let new_chain = left_chain.merge_chain(right_chain, jn.clone());
                join_chains.push(new_chain);

                node_count += 1;

                join_nodes.push(jn);
            },
            None => {
                join_chains.push(left_chain);
            }
        };
    }

    join_nodes
}

fn from_join_ref<'a>(jref: &JoinRef, qg: &'a QueryGraph) -> (JoinType, &'a ConditionTree) {
    let edge = qg.edges.get(&(jref.src.clone(), jref.dst.clone())).unwrap();
    match *edge {
        QueryGraphEdge::Join(ref jps) => (JoinType::Inner, jps.get(jref.index).unwrap()),
        QueryGraphEdge::LeftJoin(ref jps) => (JoinType::Left, jps.get(jref.index).unwrap()),
        QueryGraphEdge::GroupBy(_) => unreachable!(),
    }
}

fn pick_join_chains(
    src: &String,
    dst: &String,
    join_chains: &mut Vec<JoinChain>,
    node_for_rel: &HashMap<&str, MirNodeRef>,
) -> (JoinChain, Option<JoinChain>) {
    let left_chain = match join_chains.iter().position(|chain| chain.has_table(src)) {
        Some(idx) => join_chains.swap_remove(idx),
        None => JoinChain {
            tables: vec![src.clone()].into_iter().collect(),
            last_node: node_for_rel[src.as_str()].clone(),
        },
    };

    if left_chain.has_table(dst) {
        return (left_chain, None);
    }

    let right_chain = match join_chains.iter().position(|chain| chain.has_table(dst)) {
        Some(idx) => join_chains.swap_remove(idx),
        None => JoinChain {
            tables: vec![dst.clone()].into_iter().collect(),
            last_node: node_for_rel[dst.as_str()].clone(),
        },
    };

    (left_chain, Some(right_chain))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir::node::{MirNode, MirNodeType};
    use nom_sql::{self, ColumnSpecification, SqlType};
    use mir::MirNodeRef;
    use mir::Column;

    fn make_nodes() -> (MirNodeRef, MirNodeRef, MirNodeRef) {
        let cspec = |n: &str| -> (ColumnSpecification, Option<usize>) {
            (
                ColumnSpecification::new(nom_sql::Column::from(n), SqlType::Text),
                None,
            )
        };
        let a = MirNode::new(
            "a",
            0,
            vec![Column::from("aa"), Column::from("ab")],
            MirNodeType::Base {
                column_specs: vec![cspec("aa"), cspec("ab")],
                keys: vec![Column::from("aa")],
                adapted_over: None,
            },
            vec![],
            vec![],
        );
        let b = MirNode::new(
            "b",
            0,
            vec![Column::from("aa"), Column::from("bb")],
            MirNodeType::Base {
                column_specs: vec![cspec("ba"), cspec("bb")],
                keys: vec![Column::from("ba")],
                adapted_over: None,
            },
            vec![],
            vec![],
        );
        let c = MirNode::new(
            "c",
            0,
            vec![Column::from("aa"), Column::from("ba")],
            MirNodeType::Join {
                on_left: vec![Column::from("ab")],
                on_right: vec![Column::from("bb")],
                project: vec![Column::from("aa"), Column::from("ba")],
            },
            vec![],
            vec![],
        );
        (a, b, c)
    }

    #[test] // with just two tables reused, do the right thing
    fn pick_two_chains() {
        let mut node_for_rel: HashMap<&str, MirNodeRef> = HashMap::default();
        let (base_a, base_b, join_ab) = make_nodes();
        node_for_rel.insert("A", base_a);
        node_for_rel.insert("B", base_b);
        let mut join_chains = Vec::new();

        // no chain stuff if we're joining a table with itself
        let (left_chain, second_chain) = pick_join_chains(&"A".to_string(), &"A".to_string(), &mut join_chains, &node_for_rel);
        assert!(!second_chain.is_some());
        join_chains.push(left_chain);

        // we do need to do stuff with a newly joined table
        let (left_chain, second_chain) = pick_join_chains(&"A".to_string(), &"B".to_string(), &mut join_chains, &node_for_rel);
        match second_chain {
            Some(right_chain) => {
                let new_chain = left_chain.merge_chain(right_chain, join_ab.clone());
                join_chains.push(new_chain);
            },
            None => {
                assert!(false);
            },
        };

        // we don't need to do anything more if we join those again
        let (left_chain, second_chain) = pick_join_chains(&"A".to_string(), &"B".to_string(), &mut join_chains, &node_for_rel);
        assert!(!second_chain.is_some());
        join_chains.push(left_chain);

        // including if we join them in opposite order
        let (left_chain, second_chain) = pick_join_chains(&"B".to_string(), &"A".to_string(), &mut join_chains, &node_for_rel);
        assert!(!second_chain.is_some());
        join_chains.push(left_chain);
    }
}
