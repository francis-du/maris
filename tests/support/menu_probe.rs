//! Native menu construction/locale review only. No audio capture, routing or user-state writes.
fn main() -> anyhow::Result<()> {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let args: Vec<_> = std::env::args().skip(1).collect();
        let review = if args.is_empty() {
            maris::desktop::menu_review()?
        } else if args == ["--event-loop"] {
            maris::desktop::event_loop_review()?
        } else if args == ["--capture"] {
            #[cfg(target_os = "macos")]
            {
                maris::desktop::menu_capture_review()?
            }
            #[cfg(not(target_os = "macos"))]
            anyhow::bail!("Native menu capture requires macOS")
        } else {
            anyhow::bail!("Unsupported native menu review argument")
        };
        std::fs::create_dir_all(".maris-review")?;
        let text = serde_json::to_string_pretty(&review)?;
        std::fs::write(".maris-review/menu-review.json", &text)?;
        println!("{text}");
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    anyhow::bail!("Native menu construction review requires macOS or Windows")
}
