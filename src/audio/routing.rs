#[cfg(not(target_os = "macos"))]
use crate::control::store::Store;
#[cfg(not(target_os = "macos"))]
use anyhow::Result;

pub struct RoutePlan {
    pub input: String,
    pub output: String,
}

#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod platform;
#[cfg(target_os = "macos")]
pub use platform::{activate, plan, restore, RouteGuard};

#[cfg(not(target_os = "macos"))]
pub struct RouteGuard;
#[cfg(not(target_os = "macos"))]
pub fn plan(_output: Option<&str>) -> Result<RoutePlan> {
    anyhow::bail!("Automatic system routing is currently implemented only for macOS. Run 'maris doctor' for this platform's virtual-device setup, then use 'maris run --input INPUT --output OUTPUT' or 'maris tui'.")
}
#[cfg(not(target_os = "macos"))]
pub fn activate(_store: &Store, _plan: &RoutePlan) -> Result<RouteGuard> {
    anyhow::bail!("Automatic routing is unavailable on this platform")
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn restore(_store: &Store) -> Result<()> {
    anyhow::bail!(
        "Maris did not change system routing on this platform; restore it in system audio settings"
    )
}
