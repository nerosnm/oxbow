pub mod ast;

#[doc(hidden)]
pub use generated::oxbow;

mod generated {
    #![allow(clippy::all)]
    #![allow(unused_extern_crates)]
    #![allow(missing_docs)]
    #![allow(dead_code)]

    use lalrpop_util::lalrpop_mod;

    lalrpop_mod!(pub oxbow);
}

#[cfg(test)]
mod tests {
    use crate::parse::ast::Quote;

    use super::oxbow::QuoteParser;

    #[test]
    fn quote_simple() {
        let input = r#"quote @nerosnm "hi hello there""#;
        let expected = Quote::Add {
            username: "nerosnm".into(),
            key: None,
            text: r#"hi hello there"#.into(),
        };

        let actual = QuoteParser::new()
            .parse(input)
            .expect("valid input parses successfully");

        assert_eq!(actual, expected);
    }

    #[test]
    fn quote_tricky() {
        let input = r#"quote @nerosnm "i quote @nerosnm as having #said: 'hi'""#;
        let expected = Quote::Add {
            username: "nerosnm".into(),
            key: None,
            text: r#"i quote @nerosnm as having #said: 'hi'"#.into(),
        };

        let actual = QuoteParser::new()
            .parse(input)
            .expect("valid input parses successfully");

        assert_eq!(actual, expected);
    }

    #[test]
    fn quote_keyword_start() {
        let input = r#"quote @nerosnm #test-quote "this is a test quote""#;
        let expected = Quote::Add {
            username: "nerosnm".into(),
            key: Some("test-quote".into()),
            text: r#"this is a test quote"#.into(),
        };

        let actual = QuoteParser::new()
            .parse(input)
            .expect("valid input parses successfully");

        assert_eq!(actual, expected);
    }

    #[test]
    fn quote_keyword_end() {
        let input = r#"quote @nerosnm "this is a test quote" #testquote"#;
        let expected = Quote::Add {
            username: "nerosnm".into(),
            key: Some("testquote".into()),
            text: r#"this is a test quote"#.into(),
        };

        let actual = QuoteParser::new()
            .parse(input)
            .expect("valid input parses successfully");

        assert_eq!(actual, expected);
    }
}
