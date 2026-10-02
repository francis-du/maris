//! Process selection checked without native handles or audio devices.
use anyhow::{ensure, Result};
use std::collections::{BTreeMap, BTreeSet};

fn descendants(root: u32, children: &BTreeMap<u32, Vec<u32>>) -> BTreeSet<u32> {
    let mut result = BTreeSet::new();
    let mut pending = vec![root];
    while let Some(pid) = pending.pop() {
        if result.insert(pid) {
            if let Some(next) = children.get(&pid) {
                pending.extend(next);
            }
        }
    }
    result
}

pub(super) fn selection(
    roots: &BTreeSet<u32>,
    own: u32,
    parents: &BTreeMap<u32, u32>,
) -> Result<(BTreeSet<u32>, BTreeSet<u32>)> {
    ensure!(
        parents.len() <= 65_536 && parents.contains_key(&own),
        "Windows process list is incomplete"
    );
    ensure!(
        roots.len() <= 64
            && roots
                .iter()
                .all(|pid| *pid != 0 && parents.contains_key(pid)),
        "A selected application is no longer available"
    );
    let mut children = BTreeMap::<u32, Vec<u32>>::new();
    for (&pid, &parent) in parents {
        children.entry(parent).or_default().push(pid);
    }
    let excluded = descendants(own, &children);
    let mut captured = BTreeSet::new();
    for &root in roots {
        let branch = descendants(root, &children);
        ensure!(
            branch.is_disjoint(&excluded),
            "This selection would record Maris's own output"
        );
        ensure!(
            branch.is_disjoint(&captured),
            "Select an app or its child app, not both"
        );
        captured.extend(branch);
    }
    Ok((captured, excluded))
}

#[cfg(test)]
#[path = "../../tests/unit/windows_scope.rs"]
mod tests;
