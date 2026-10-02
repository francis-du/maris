//! Destinations shared by the pinned generated catalog and its build-time pack.
pub fn destination(after: &str) -> Option<&str> {
    // Each generated list line ends in its link delimiter. Real upstream directory
    // names contain both balanced parentheses and literal unmatched closing ones.
    // Parsing at the first balanced close silently truncates those actual filenames.
    let path = after.strip_suffix(')')?;
    // The pinned index spells this one mode with a lowercase p, but its actual
    // Git tree uses Passive in both the directory and ParametricEQ filename.
    // Keep build keys and runtime acquisition URLs identical on every platform.
    Some(match path {
        "./crinacle/711%20in-ear/Samsung%20Galaxy%20Buds2%20Pro%20(passive%20mode)" => {
            "./crinacle/711%20in-ear/Samsung%20Galaxy%20Buds2%20Pro%20(Passive%20mode)"
        }
        _ => path,
    })
}
