/// 入库时的轻量识别。只根据整段内容打标签，不改写原文。
pub struct Detection {
    pub kind: &'static str,
    pub language: Option<&'static str>,
}

const MAX_BYTES: usize = 100_000;

pub fn classify(content: &str) -> Detection {
    let text = content.trim();
    if text.is_empty() || text.len() > MAX_BYTES {
        return Detection {
            kind: "text",
            language: None,
        };
    }
    if is_url(text) {
        return Detection {
            kind: "link",
            language: None,
        };
    }
    if is_sql(text) {
        return Detection {
            kind: "code",
            language: Some("sql"),
        };
    }
    if is_json(text) {
        return Detection {
            kind: "code",
            language: Some("json"),
        };
    }
    if let Some(language) = code_language(text) {
        return Detection {
            kind: "code",
            language: Some(language),
        };
    }
    Detection {
        kind: "text",
        language: None,
    }
}

fn is_url(text: &str) -> bool {
    if text.chars().any(char::is_whitespace) {
        return false;
    }
    let lower = text.to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("www.")
}

fn is_sql(text: &str) -> bool {
    let mut rest = text;
    loop {
        rest = rest.trim_start();
        if let Some(stripped) = rest.strip_prefix("--") {
            rest = stripped.split_once('\n').map(|(_, right)| right).unwrap_or("");
            continue;
        }
        if let Some(stripped) = rest.strip_prefix("/*") {
            let Some(end) = stripped.find("*/") else {
                return false;
            };
            rest = &stripped[end + 2..];
            continue;
        }
        break;
    }
    let first = rest
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .find(|word| !word.is_empty())
        .unwrap_or("");
    let first = first.to_ascii_uppercase();
    let starts = matches!(
        first.as_str(),
        "SELECT" | "INSERT" | "UPDATE" | "DELETE" | "WITH" | "CREATE" | "ALTER" | "DROP"
    );
    if !starts {
        return false;
    }
    let upper = rest.to_ascii_uppercase();
    ["FROM", "INTO", "SET", "TABLE", "VALUES"]
        .iter()
        .any(|word| contains_word(&upper, word))
}

fn is_json(text: &str) -> bool {
    let first = text.chars().next();
    if first != Some('{') && first != Some('[') {
        return false;
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return false;
    };
    match value {
        serde_json::Value::Object(map) => !map.is_empty(),
        serde_json::Value::Array(items) => items.len() >= 2,
        _ => false,
    }
}

fn code_language(text: &str) -> Option<&'static str> {
    let lines: Vec<&str> = text.lines().filter(|line| !line.trim().is_empty()).collect();
    if lines.is_empty() {
        return None;
    }

    let mut score = 0;
    if lines.len() >= 2 {
        score += 1;
    }
    if lines.len() >= 4 {
        score += 1;
    }
    let has_braces = text.contains('{') && text.contains('}');
    if has_braces {
        score += 2;
    }
    if text.contains("=>") {
        score += 2;
    }
    if text.contains("::") {
        score += 1;
    }
    let semicolons = lines.iter().filter(|line| line.trim_end().ends_with(';')).count();
    if semicolons >= 1 {
        score += 1;
    }
    if semicolons >= 2 {
        score += 1;
    }
    let indented = lines
        .iter()
        .filter(|line| line.starts_with("  ") || line.starts_with('\t'))
        .count();
    if indented >= 2 {
        score += 2;
    }
    if lines.iter().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("//")
            || trimmed.starts_with("/*")
            || trimmed.starts_with('#')
            || trimmed.starts_with("<!--")
    }) {
        score += 1;
    }
    if text.contains('(') && text.contains(')') {
        score += 1;
    }
    if text.contains("</") || text.contains("<!DOCTYPE") {
        score += 2;
    }

    let language = guess_language(text);
    if language != "other" {
        score += 3;
    }

    let cjk = text.chars().filter(|ch| ('\u{4E00}'..='\u{9FFF}').contains(ch)).count();
    let total = text.chars().count().max(1);
    if cjk * 2 > total {
        score -= 3;
    }

    let strong = lines.iter().any(|line| line_starts_with_keyword(line.trim_start()))
        || text.contains("</")
        || text.contains("<!DOCTYPE");
    let structural = has_braces
        || text.contains("=>")
        || semicolons >= 1
        || indented >= 2
        || text.contains('<') && text.contains('>')
        || strong;
    let passed = structural && ((strong && score >= 4) || score >= 6);
    if passed {
        Some(language)
    } else {
        None
    }
}

fn line_starts_with_keyword(line: &str) -> bool {
    const WORDS: &[&str] = &[
        "const ", "let ", "var ", "function ", "import ", "export ", "class ", "interface ",
        "def ", "fn ", "func ", "pub ", "package ", "public ", "private ",
    ];
    WORDS.iter().any(|word| line.starts_with(word)) || line.starts_with("#!")
}

fn guess_language(text: &str) -> &'static str {
    let mut scores = [
        ("html", 0),
        ("css", 0),
        ("rust", 0),
        ("go", 0),
        ("java", 0),
        ("python", 0),
        ("shell", 0),
        ("ts", 0),
        ("js", 0),
    ];
    let add = |scores: &mut [(&str, i32)], name: &str, amount: i32| {
        if let Some(slot) = scores.iter_mut().find(|(lang, _)| *lang == name) {
            slot.1 += amount;
        }
    };

    if text.contains("<!DOCTYPE") || text.contains("<html") || text.contains("<div") || text.contains("<span") || text.contains("</")
    {
        add(&mut scores, "html", 3);
    }
    if text.contains('{') && text.contains('}') && text.contains(':') && text.contains(';') && !contains_word(text, "function") && !contains_word(text, "const") && !contains_word(text, "fn")
    {
        add(&mut scores, "css", 2);
    }
    if contains_word(text, "fn") || text.contains("println!") || text.contains("let mut") || contains_word(text, "impl")
    {
        add(&mut scores, "rust", 3);
    }
    if text.contains("package ") || text.contains(":=") || (contains_word(text, "func") && text.contains('{')) {
        add(&mut scores, "go", 3);
    }
    if text.contains("public class") || text.contains("public static") || text.contains("System.out") {
        add(&mut scores, "java", 3);
    }
    if contains_word(text, "def") || contains_word(text, "elif") || contains_word(text, "lambda") || text.contains("self.")
    {
        add(&mut scores, "python", 3);
    }
    if text.starts_with("#!") || contains_word(text, "fi") || contains_word(text, "esac") {
        add(&mut scores, "shell", 3);
    }
    if contains_word(text, "echo") && (text.contains("then") || text.contains("fi") || text.starts_with("#!")) {
        add(&mut scores, "shell", 2);
    }
    if contains_word(text, "interface")
        || contains_word(text, "implements")
        || text.contains(": string")
        || text.contains(": number")
        || text.contains(": boolean")
    {
        add(&mut scores, "ts", 3);
    }
    if contains_word(text, "function")
        || contains_word(text, "const")
        || contains_word(text, "let")
        || text.contains("=>")
        || contains_word(text, "export")
    {
        add(&mut scores, "js", 2);
    }

    scores
        .into_iter()
        .max_by_key(|(_, score)| *score)
        .filter(|(_, score)| *score > 0)
        .map(|(lang, _)| lang)
        .unwrap_or("other")
}

fn contains_word(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(index, _)| {
        let bytes = text.as_bytes();
        let before = index == 0 || !is_ident(bytes[index - 1]);
        let after_at = index + word.len();
        let after = after_at >= bytes.len() || !is_ident(bytes[after_at]);
        before && after
    })
}

fn is_ident(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(test)]
mod tests {
    use super::classify;

    #[test]
    fn javascript_object_is_code() {
        let found = classify(
            "const gridOptions = {\n  columns: [\n    { type: 'seq', width: 70 },\n  ]\n}\n",
        );
        assert_eq!(found.kind, "code");
        assert_eq!(found.language, Some("js"));
    }

    #[test]
    fn typescript_interface_is_ts() {
        let found = classify("interface User {\n  name: string;\n  age: number;\n}\n");
        assert_eq!(found.kind, "code");
        assert_eq!(found.language, Some("ts"));
    }

    #[test]
    fn python_function_is_code() {
        let found = classify("def hello():\n    return 1\n");
        assert_eq!(found.kind, "code");
        assert_eq!(found.language, Some("python"));
    }

    #[test]
    fn rust_fn_is_code() {
        let found = classify("fn main() {\n    println!(\"hi\");\n}\n");
        assert_eq!(found.kind, "code");
        assert_eq!(found.language, Some("rust"));
    }

    #[test]
    fn sql_select_is_code() {
        let found = classify("SELECT id, name FROM users WHERE active = 1;");
        assert_eq!(found.kind, "code");
        assert_eq!(found.language, Some("sql"));
    }

    #[test]
    fn lone_select_stays_text() {
        assert_eq!(classify("select").kind, "text");
    }

    #[test]
    fn json_object_is_code() {
        let found = classify("{\"name\":\"Ada\",\"year\":1815}");
        assert_eq!(found.kind, "code");
        assert_eq!(found.language, Some("json"));
    }

    #[test]
    fn short_json_array_stays_text() {
        assert_eq!(classify("[1]").kind, "text");
    }

    #[test]
    fn html_fragment_is_code() {
        let found = classify("<div class=\"panel\">\n  <span>折角</span>\n</div>\n");
        assert_eq!(found.kind, "code");
        assert_eq!(found.language, Some("html"));
    }

    #[test]
    fn url_stays_link() {
        assert_eq!(classify("https://example.com/a").kind, "link");
    }

    #[test]
    fn chinese_sentence_stays_text() {
        assert_eq!(classify("今天天气不错，我们下午三点开会。").kind, "text");
    }

    #[test]
    fn prose_with_return_stays_text() {
        assert_eq!(classify("Please return the book tomorrow.").kind, "text");
    }
}
