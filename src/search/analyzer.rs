use tantivy::tokenizer::{LowerCaser, RegexTokenizer, TextAnalyzer};

pub const ANALYZER_NAME: &str = "rust";
/// Bump when tokenization changes: an index built with another version is not opened.
pub const ANALYZER_VERSION: &str = "rust-v1";

/// Words (letters, digits, `_`) plus the symbols that carry meaning in Rust text: `?`, `!`,
/// `'` (lifetimes), `::`, `->`, `=>` and `..`. A prose tokenizer would drop `?` and `!Send`;
/// stop words are kept because they are part of identifiers and code. No stemming.
pub fn analyzer() -> TextAnalyzer {
    let tokenizer = RegexTokenizer::new(r"[\p{L}\p{N}_]+|::|->|=>|\.\.=?|[?!']")
        .expect("the analyzer pattern is valid");
    TextAnalyzer::builder(tokenizer).filter(LowerCaser).build()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(text: &str) -> Vec<String> {
        let mut analyzer = analyzer();
        let mut stream = analyzer.token_stream(text);
        let mut out = Vec::new();
        while stream.advance() {
            out.push(stream.token().text.clone());
        }
        out
    }

    #[test]
    fn rust_symbols_and_identifiers_survive() {
        assert_eq!(tokens("Try"), ["try"]);
        assert_eq!(tokens("FromResidual"), ["fromresidual"]);
        assert_eq!(tokens("async fn"), ["async", "fn"]);
        assert_eq!(tokens("?"), ["?"]);
        assert_eq!(tokens("T: !Send"), ["t", "!", "send"]);
        assert_eq!(tokens("RFC 3058"), ["rfc", "3058"]);
        assert_eq!(tokens("&'a str"), ["'", "a", "str"]);
        assert_eq!(tokens("std::io::Error"), ["std", "::", "io", "::", "error"]);
        assert_eq!(tokens("snake_case GAT"), ["snake_case", "gat"]);
        assert_eq!(tokens("Привет, Rust"), ["привет", "rust"]);
    }
}
