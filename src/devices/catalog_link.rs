//! Destinations shared by the pinned generated catalog and its build-time pack.
pub fn destination(after: &str) -> Option<&str> {
    // Each generated list line ends in its link delimiter. Real upstream directory
    // names contain both balanced parentheses and literal unmatched closing ones.
    // Parsing at the first balanced close silently truncates those actual filenames.
    after.strip_suffix(')')
}
