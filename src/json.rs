pub fn string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::string;

    #[test]
    fn quotes_and_escapes() {
        assert_eq!(string("plain"), "\"plain\"");
        assert_eq!(string("a\"b\\c"), "\"a\\\"b\\\\c\"");
        assert_eq!(string("line\nbreak\ttab"), "\"line\\nbreak\\ttab\"");
        assert_eq!(string("bell\u{07}"), "\"bell\\u0007\"");
        assert_eq!(string("esc\u{1b}[0m"), "\"esc\\u001b[0m\"");
        assert_eq!(string("del\u{7f}"), "\"del\\u007f\"");
    }

    #[test]
    fn keeps_non_ascii() {
        assert_eq!(string("café"), "\"café\"");
    }
}
