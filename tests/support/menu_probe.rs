//! Native menu construction/locale review only. No capture, routing or user-state writes.
fn main() -> anyhow::Result<()> {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let review = maris::desktop::menu_review()?;
        std::fs::create_dir_all(".maris-review")?;
        let text = serde_json::to_string_pretty(&review)?;
        std::fs::write(".maris-review/menu-review.json", &text)?;
        println!("{text}");
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    anyhow::bail!("Native menu construction review requires macOS or Windows")
}
