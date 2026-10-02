//! Explicit heartbeats for a synthetic engine expected to remain live during an action.
//! Expiry and future-timestamp cases must keep their original raw fixture input.
use maris::{analysis, store::Store};
use serde_json::{json, Value};

pub fn refresh(store: &Store) -> Value {
    let mut runtime: Value =
        maris::store::read_json(&store.directory.join("runtime.json")).unwrap();
    // Preserve the session, device, binding, measurement and applied revisions.
    runtime["updated_at_ms"] = json!(analysis::now_ms());
    store.write_json("runtime.json", &runtime).unwrap();
    runtime
}
