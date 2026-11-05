/// The title as the index and the results show it. The RFC book names every page
/// `0001-private-fields - The Rust RFC Book`, which is a file name, not a title.
pub fn display_title(source: &str, title: &str) -> String {
    if source != "rfc" {
        return title.trim().to_owned();
    }
    let name = title.trim().trim_end_matches(" - The Rust RFC Book").trim();
    match name.split_once('-') {
        Some((number, slug))
            if !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()) =>
        {
            let words = slug.replace(['-', '_'], " ");
            let number = number.trim_start_matches('0');
            format!(
                "RFC {}: {}",
                if number.is_empty() { "0" } else { number },
                words.trim()
            )
        }
        _ => name.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc_titles_lose_the_file_name_form() {
        assert_eq!(
            display_title("rfc", "0001-private-fields - The Rust RFC Book"),
            "RFC 1: private fields"
        );
        assert_eq!(
            display_title("rfc", "1598-generic_associated_types - The Rust RFC Book"),
            "RFC 1598: generic associated types"
        );
        assert_eq!(display_title("rfc", "Something else"), "Something else");
    }

    #[test]
    fn other_sources_keep_their_titles() {
        assert_eq!(
            display_title("internals", " Idea: async fn in trait objects "),
            "Idea: async fn in trait objects"
        );
    }
}
