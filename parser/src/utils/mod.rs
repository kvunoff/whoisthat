pub mod inbound_generator;

pub fn url_decode_str(value: &str) -> Option<String> {
    return urlencoding::decode(value)
        .ok()
        .map(|decoded| decoded.into_owned());
}

pub fn url_decode(value: Option<String>) -> Option<String> {
    return value.and_then(|s| {
        urlencoding::decode(&s)
            .ok()
            .map(|decoded| decoded.into_owned())
    });
}

pub fn parse_raw_json(input: &str) -> Option<serde_json::Value> {
    serde_json::from_str::<serde_json::Value>(input)
        .ok()
        .and_then(|v| match v {
            serde_json::Value::Object(_) => Some(v),
            _ => None,
        })
}

pub fn get_parameter_value(query: &Vec<(&str, &str)>, param: &str) -> Option<String> {
    let param = query
        .iter()
        .find(|q| String::from(q.0) == String::from(param))
        .map(|q| q.1.to_string());
    
    match param {
        Some(param) if param.is_empty() => None,
        Some(param) if !param.is_empty() => Some(param),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod url_decode_str_tests {
        use super::*;

        #[test]
        fn decodes_plain_string() {
            assert_eq!(url_decode_str("hello"), Some("hello".to_string()));
        }

        #[test]
        fn decodes_percent_encoded_spaces() {
            assert_eq!(url_decode_str("hello%20world"), Some("hello world".to_string()));
        }

        #[test]
        fn decodes_multiple_encoded_chars() {
            assert_eq!(url_decode_str("a%20b%2Fc%3Dd"), Some("a b/c=d".to_string()));
        }

        #[test]
        fn passes_through_unencoded() {
            assert_eq!(url_decode_str("hello+world"), Some("hello+world".to_string()));
        }

        #[test]
        fn decodes_cyrillic() {
            assert_eq!(url_decode_str("%D0%BF%D1%80%D0%B8%D0%B2%D0%B5%D1%82"), Some("привет".to_string()));
        }

        #[test]
        fn passes_through_incomplete_encoding() {
            assert_eq!(url_decode_str("hello%2"), Some("hello%2".to_string()));
        }

        #[test]
        fn passes_through_invalid_hex_encoding() {
            assert_eq!(url_decode_str("hello%ZZ"), Some("hello%ZZ".to_string()));
        }

        #[test]
        fn decodes_empty_string() {
            assert_eq!(url_decode_str(""), Some("".to_string()));
        }

        #[test]
        fn decodes_only_encoded_chars() {
            assert_eq!(url_decode_str("%41%42%43"), Some("ABC".to_string()));
        }
    }

    mod url_decode_tests {
        use super::*;

        #[test]
        fn returns_none_for_none_input() {
            assert_eq!(url_decode(None), None);
        }

        #[test]
        fn delegates_to_url_decode_str_for_some() {
            assert_eq!(url_decode(Some("hello%20world".to_string())), Some("hello world".to_string()));
        }

        #[test]
        fn passes_through_invalid_encoding_in_some() {
            assert_eq!(url_decode(Some("hello%ZZ".to_string())), Some("hello%ZZ".to_string()));
        }
    }

    mod parse_raw_json_tests {
        use super::*;

        #[test]
        fn parses_valid_object() {
            let result = parse_raw_json(r#"{"key": "value"}"#);
            assert!(result.is_some());
            let val = result.unwrap();
            assert_eq!(val["key"], "value");
        }

        #[test]
        fn returns_none_for_array() {
            assert_eq!(parse_raw_json(r#"["item1", "item2"]"#), None);
        }

        #[test]
        fn returns_none_for_string() {
            assert_eq!(parse_raw_json(r#""just a string""#), None);
        }

        #[test]
        fn returns_none_for_null() {
            assert_eq!(parse_raw_json("null"), None);
        }

        #[test]
        fn returns_none_for_invalid_json() {
            assert_eq!(parse_raw_json("{invalid"), None);
        }

        #[test]
        fn parses_nested_object() {
            let result = parse_raw_json(r#"{"outer": {"inner": 42}}"#);
            assert!(result.is_some());
            let val = result.unwrap();
            assert_eq!(val["outer"]["inner"], 42);
        }

        #[test]
        fn returns_none_for_empty_string() {
            assert_eq!(parse_raw_json(""), None);
        }
    }

    mod get_parameter_value_tests {
        use super::*;

        #[test]
        fn finds_existing_param() {
            let query = vec![("sni", "example.com"), ("type", "ws")];
            assert_eq!(get_parameter_value(&query, "sni"), Some("example.com".to_string()));
        }

        #[test]
        fn returns_none_for_missing_param() {
            let query = vec![("sni", "example.com")];
            assert_eq!(get_parameter_value(&query, "flow"), None);
        }

        #[test]
        fn returns_none_for_empty_value() {
            let query = vec![("sni", "")];
            assert_eq!(get_parameter_value(&query, "sni"), None);
        }

        #[test]
        fn finds_among_multiple_params() {
            let query = vec![("a", "1"), ("b", "2"), ("c", "3")];
            assert_eq!(get_parameter_value(&query, "b"), Some("2".to_string()));
        }

        #[test]
        fn handles_empty_vec() {
            let query: Vec<(&str, &str)> = vec![];
            assert_eq!(get_parameter_value(&query, "anything"), None);
        }
    }
}
