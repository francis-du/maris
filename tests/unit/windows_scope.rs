use super::*;
fn processes() -> BTreeMap<u32, u32> {
    BTreeMap::from([
        (10, 0),
        (20, 10),
        (21, 20),
        (30, 10),
        (31, 30),
        (32, 31),
        (40, 10),
    ])
}
#[test]
fn capture_cannot_include_maris_through_a_parent_application() {
    for pid in [10, 20, 21] {
        assert!(
            selection(&BTreeSet::from([pid]), 20, &processes()).is_err(),
            "unsafe root {pid}"
        );
    }
}
#[test]
fn selecting_both_a_parent_and_child_cannot_double_the_same_audio() {
    for roots in [[30, 31], [30, 32], [31, 32]] {
        assert!(selection(&BTreeSet::from(roots), 20, &processes()).is_err());
    }
}
#[test]
fn independent_apps_keep_their_children_and_exclude_maris() {
    let (captured, excluded) = selection(&BTreeSet::from([30, 40]), 20, &processes()).unwrap();
    assert_eq!(captured, BTreeSet::from([30, 31, 32, 40]));
    assert_eq!(excluded, BTreeSet::from([20, 21]));
}
#[test]
fn global_scope_uses_only_the_self_exclusion_set() {
    let (captured, excluded) = selection(&BTreeSet::new(), 20, &processes()).unwrap();
    assert!(captured.is_empty());
    assert_eq!(excluded, BTreeSet::from([20, 21]));
}
#[test]
fn incomplete_or_exited_processes_do_not_become_a_valid_scope() {
    assert!(selection(&BTreeSet::from([999]), 20, &processes()).is_err());
    assert!(selection(&BTreeSet::from([0]), 20, &processes()).is_err());
    assert!(selection(&BTreeSet::from([30]), 999, &processes()).is_err());
}
#[test]
fn a_cyclic_unrelated_snapshot_remains_bounded() {
    let mut parents = processes();
    parents.extend([(90, 91), (91, 90)]);
    assert_eq!(
        selection(&BTreeSet::from([30]), 20, &parents).unwrap().0,
        BTreeSet::from([30, 31, 32])
    );
}
