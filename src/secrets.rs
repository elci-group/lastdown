/// Strip credential-shaped substrings from text that would otherwise be
/// copied into a continuation object.
///
/// The scan is deliberately small and explicit. It exists so a tool transcript
/// cannot smuggle a live token into the next possession.
pub fn redact_secrets(input: &str) -> (String, u32) {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut index = 0;
    let mut redactions = 0;
    while index < bytes.len() {
        if let Some(end) = match_private_key(input, index) {
            out.push_str("[redacted]");
            redactions += 1;
            index = end;
            continue;
        }
        if let Some(end) = match_aws_key(bytes, index) {
            out.push_str("[redacted]");
            redactions += 1;
            index = end;
            continue;
        }
        if let Some(end) = match_bearer(input, index) {
            out.push_str("[redacted]");
            redactions += 1;
            index = end;
            continue;
        }
        if let Some(end) = match_sk_token(bytes, index) {
            out.push_str("[redacted]");
            redactions += 1;
            index = end;
            continue;
        }
        if let Some(end) = match_api_key(input, index) {
            out.push_str("[redacted]");
            redactions += 1;
            index = end;
            continue;
        }
        let ch = input[index..]
            .chars()
            .next()
            .expect("index is on a char boundary");
        out.push(ch);
        index += ch.len_utf8();
    }
    (out, redactions)
}

fn match_private_key(input: &str, index: usize) -> Option<usize> {
    const BEGIN: &str = "-----BEGIN PRIVATE KEY-----";
    const END: &str = "-----END PRIVATE KEY-----";
    let rest = input.get(index..)?;
    if !rest.starts_with(BEGIN) {
        return None;
    }
    let after = index + BEGIN.len();
    if let Some(rel) = input[after..].find(END) {
        Some(after + rel + END.len())
    } else {
        Some(input.len())
    }
}

fn match_aws_key(bytes: &[u8], index: usize) -> Option<usize> {
    let rest = bytes.get(index..)?;
    if rest.len() < 20 || &rest[..4] != b"AKIA" {
        return None;
    }
    if rest[4..20].iter().all(is_key_char) {
        Some(index + 20)
    } else {
        None
    }
}

fn match_sk_token(bytes: &[u8], index: usize) -> Option<usize> {
    let rest = bytes.get(index..)?;
    if rest.len() < 11 || &rest[..3] != b"sk-" {
        return None;
    }
    let mut end = 3;
    let mut key_chars = 0;
    while end < rest.len() && (is_key_char(&rest[end]) || rest[end] == b'-') {
        if rest[end] != b'-' {
            key_chars += 1;
        }
        end += 1;
    }
    if key_chars >= 8 {
        Some(index + end)
    } else {
        None
    }
}

fn match_bearer(input: &str, index: usize) -> Option<usize> {
    let rest = input.get(index..)?;
    let prefix_len = if rest.len() >= 7 && rest[..7].eq_ignore_ascii_case("bearer ") {
        7
    } else {
        return None;
    };
    let token: String = rest[prefix_len..]
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | '='))
        .collect();
    if token.len() >= 8 {
        Some(index + prefix_len + token.len())
    } else {
        None
    }
}

fn match_api_key(input: &str, index: usize) -> Option<usize> {
    let rest = input.get(index..)?;
    let lowered = rest.to_ascii_lowercase();
    let prefix_len = if lowered.starts_with("api_key") {
        "api_key".len()
    } else if lowered.starts_with("api-key") {
        "api-key".len()
    } else if lowered.starts_with("apikey") {
        "apikey".len()
    } else {
        return None;
    };
    let after = &rest[prefix_len..];
    let spaced = after.trim_start_matches([' ', '\t']);
    let consumed_space = after.len() - spaced.len();
    let spaced_bytes = spaced.as_bytes();
    if spaced_bytes.first().copied() != Some(b'=') && spaced_bytes.first().copied() != Some(b':') {
        return None;
    }
    let value = &spaced[1..];
    let value_trimmed = value.trim_start_matches([' ', '\t']);
    let consumed_value_space = value.len() - value_trimmed.len();
    let token_len = value_trimmed
        .chars()
        .take_while(|ch| !ch.is_whitespace())
        .map(char::len_utf8)
        .sum::<usize>();
    if token_len < 4 {
        return None;
    }
    Some(index + prefix_len + consumed_space + 1 + consumed_value_space + token_len)
}

fn is_key_char(byte: &u8) -> bool {
    byte.is_ascii_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::redact_secrets;

    #[test]
    fn redacts_known_secret_shapes() {
        let (text, count) = redact_secrets(
            "prefix AKIAIOSFODNN7EXAMPLE sk-live-abcdefghij bearer Abcd1234 api_key=zzzz tail",
        );
        assert!(count >= 4);
        assert!(!text.contains("AKIA"));
        assert!(!text.contains("sk-live"));
        assert!(!text.contains("Abcd1234"));
        assert!(!text.contains("zzzz"));
        assert!(text.contains("prefix"));
        assert!(text.contains("tail"));
    }

    #[test]
    fn leaves_ordinary_prose_intact() {
        let (text, count) = redact_secrets("the handler writes the index");
        assert_eq!(count, 0);
        assert_eq!(text, "the handler writes the index");
    }
}
