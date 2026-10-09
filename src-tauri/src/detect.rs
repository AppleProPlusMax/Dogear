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
    if text.chars().any(|ch| ch.is_whitespace() || ch.is_control()) {
        return false;
    }
    let lower = text.to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("www.")
}

/// 能交给系统默认浏览器的地址。`www.` 补上 `https://`，其余保持原文。
pub fn browser_url(text: &str) -> Option<String> {
    let text = text.trim();
    if !is_url(text) {
        return None;
    }
    if text.to_ascii_lowercase().starts_with("www.") {
        return Some(format!("https://{text}"));
    }
    Some(text.to_string())
}

/// 代码预览。`raw` 原样返回，不改库存内容。
pub fn present_code(content: &str, language: Option<&str>, view: &str) -> String {
    match view {
        "pretty" => pretty_code(content, language),
        "compact" => compact_code(content, language),
        _ => content.to_string(),
    }
}

fn pretty_code(content: &str, language: Option<&str>) -> String {
    match language {
        Some("json") => pretty_json(content).unwrap_or_else(|| content.to_string()),
        Some("sql") => format_sql(content),
        _ => tidy_lines(content),
    }
}

fn compact_code(content: &str, language: Option<&str>) -> String {
    match language {
        Some("json") => compact_json(content).unwrap_or_else(|| collapse_ws(content)),
        Some("python") | Some("shell") | None => content.to_string(),
        _ => collapse_ws(content),
    }
}

fn pretty_json(text: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    serde_json::to_string_pretty(&value).ok()
}

fn compact_json(text: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    serde_json::to_string(&value).ok()
}

fn tidy_lines(text: &str) -> String {
    let ends = text.ends_with('\n');
    let mut body = text
        .lines()
        .map(|line| line.trim_end().replace('\t', "  "))
        .collect::<Vec<_>>()
        .join("\n");
    if ends {
        body.push('\n');
    }
    body
}

fn collapse_ws(text: &str) -> String {
    let mut out = String::new();
    let mut spaced = false;
    let mut quote: Option<char> = None;
    for ch in text.chars() {
        if let Some(mark) = quote {
            out.push(ch);
            if ch == mark {
                quote = None;
            }
            continue;
        }
        if ch == '\'' || ch == '"' || ch == '`' {
            if spaced && !out.is_empty() {
                out.push(' ');
            }
            spaced = false;
            quote = Some(ch);
            out.push(ch);
            continue;
        }
        if ch.is_whitespace() {
            spaced = !out.is_empty();
            continue;
        }
        if spaced {
            out.push(' ');
            spaced = false;
        }
        out.push(ch);
    }
    out
}

fn format_sql(text: &str) -> String {
    let compact = collapse_ws(text);
    const KEYWORDS: &[&str] = &[
        "LEFT OUTER JOIN",
        "RIGHT OUTER JOIN",
        "FULL OUTER JOIN",
        "LEFT JOIN",
        "RIGHT JOIN",
        "INNER JOIN",
        "CROSS JOIN",
        "GROUP BY",
        "ORDER BY",
        "INSERT INTO",
        "DELETE FROM",
        "SELECT",
        "FROM",
        "WHERE",
        "HAVING",
        "LIMIT",
        "OFFSET",
        "VALUES",
        "UPDATE",
        "JOIN",
        "UNION",
        "SET",
        "AND",
        "OR",
    ];
    let chars: Vec<char> = compact.chars().collect();
    let mut out = String::new();
    let mut index = 0;
    let mut quote: Option<char> = None;
    while index < chars.len() {
        let ch = chars[index];
        if let Some(mark) = quote {
            out.push(ch);
            if ch == mark {
                quote = None;
            }
            index += 1;
            continue;
        }
        if ch == '\'' || ch == '"' || ch == '`' {
            quote = Some(ch);
            out.push(ch);
            index += 1;
            continue;
        }
        if let Some(keyword) = KEYWORDS.iter().copied().find(|keyword| starts_keyword(&chars, index, keyword)) {
            if index > 0 && !out.ends_with('\n') {
                out.push('\n');
            }
            let width = keyword.chars().count();
            for offset in 0..width {
                out.push(chars[index + offset]);
            }
            index += width;
            continue;
        }
        out.push(ch);
        index += 1;
    }
    out
}

fn starts_keyword(chars: &[char], index: usize, keyword: &str) -> bool {
    let word: Vec<char> = keyword.chars().collect();
    let end = index + word.len();
    if end > chars.len() {
        return false;
    }
    if index > 0 {
        let prev = chars[index - 1];
        if prev.is_ascii_alphanumeric() || prev == '_' {
            return false;
        }
    }
    if end < chars.len() {
        let next = chars[end];
        if next.is_ascii_alphanumeric() || next == '_' {
            return false;
        }
    }
    chars[index..end]
        .iter()
        .zip(word.iter())
        .all(|(got, want)| got.eq_ignore_ascii_case(want))
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

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundColor {
    pub hex: String,
    pub rgb: String,
    pub hsl: String,
}

/// 在文本或代码里找出颜色。最多 8 个，相同色值只留一次。不改原文。
pub fn find_colors(text: &str) -> Vec<FoundColor> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut index = 0;
    while index < bytes.len() && found.len() < 8 {
        if let Some((color, next)) = color_at(bytes, index) {
            if !found.iter().any(|item: &FoundColor| item.hex == color.hex) {
                found.push(color);
            }
            index = next.max(index + 1);
            continue;
        }
        index += 1;
    }
    found
}

fn color_at(bytes: &[u8], index: usize) -> Option<(FoundColor, usize)> {
    if bytes[index] == b'#' {
        return hex_at(bytes, index);
    }
    css_fn_at(bytes, index)
}

fn hex_at(bytes: &[u8], index: usize) -> Option<(FoundColor, usize)> {
    let mut end = index + 1;
    while end < bytes.len() && bytes[end].is_ascii_hexdigit() {
        end += 1;
    }
    let len = end - (index + 1);
    if !matches!(len, 3 | 4 | 6 | 8) {
        return None;
    }
    if matches!(len, 3 | 4) && !short_hex_ok(bytes, index) {
        return None;
    }
    let digits = &bytes[index + 1..end];
    let (r, g, b, a) = match len {
        3 => (doubled(digits[0]), doubled(digits[1]), doubled(digits[2]), 255),
        4 => (
            doubled(digits[0]),
            doubled(digits[1]),
            doubled(digits[2]),
            doubled(digits[3]),
        ),
        6 => (pair(digits[0], digits[1]), pair(digits[2], digits[3]), pair(digits[4], digits[5]), 255),
        _ => (
            pair(digits[0], digits[1]),
            pair(digits[2], digits[3]),
            pair(digits[4], digits[5]),
            pair(digits[6], digits[7]),
        ),
    };
    Some((paint(r, g, b, a), end))
}

/// 三位、四位十六进制太容易撞上「#123」这种编号，只在看起来像赋值时才算。
fn short_hex_ok(bytes: &[u8], hash_at: usize) -> bool {
    let mut index = hash_at;
    while index > 0 && bytes[index - 1].is_ascii_whitespace() {
        index -= 1;
    }
    if index == 0 {
        return true;
    }
    matches!(bytes[index - 1], b':' | b'=' | b'"' | b'\'' | b'(' | b',' | b'[' | b'{')
}

fn css_fn_at(bytes: &[u8], index: usize) -> Option<(FoundColor, usize)> {
    if index > 0 && is_ident_byte(bytes[index - 1]) {
        return None;
    }
    let rest = &bytes[index..];
    let (hsl, name_len) = if starts_ci(rest, b"hsla") {
        (true, 4)
    } else if starts_ci(rest, b"hsl") {
        (true, 3)
    } else if starts_ci(rest, b"rgba") {
        (false, 4)
    } else if starts_ci(rest, b"rgb") {
        (false, 3)
    } else {
        return None;
    };
    let mut cursor = skip_ws(bytes, index + name_len);
    if cursor >= bytes.len() || bytes[cursor] != b'(' {
        return None;
    }
    cursor += 1;
    let (first, cursor) = number_at(bytes, cursor)?;
    let cursor = separator(bytes, cursor)?;
    let (second, cursor) = number_at(bytes, cursor)?;
    let cursor = separator(bytes, cursor)?;
    let (third, mut cursor) = number_at(bytes, cursor)?;
    cursor = skip_ws(bytes, cursor);
    let (alpha, mut cursor) = if cursor < bytes.len() && (bytes[cursor] == b',' || bytes[cursor] == b'/') {
        let (alpha, cursor) = number_at(bytes, skip_ws(bytes, cursor + 1))?;
        (Some(alpha), cursor)
    } else {
        (None, cursor)
    };
    cursor = skip_ws(bytes, cursor);
    if cursor >= bytes.len() || bytes[cursor] != b')' {
        return None;
    }
    let color = if hsl {
        from_hsl(first, second, third, alpha)?
    } else {
        from_rgb(first, second, third, alpha)?
    };
    Some((color, cursor + 1))
}

struct Num {
    value: f64,
    percent: bool,
}

fn from_rgb(r: Num, g: Num, b: Num, alpha: Option<Num>) -> Option<FoundColor> {
    Some(paint(channel(r)?, channel(g)?, channel(b)?, alpha_channel(alpha)?))
}

fn from_hsl(h: Num, s: Num, l: Num, alpha: Option<Num>) -> Option<FoundColor> {
    if h.percent || !s.percent || !l.percent {
        return None;
    }
    if !(0.0..=360.0).contains(&h.value) || !(0.0..=100.0).contains(&s.value) || !(0.0..=100.0).contains(&l.value) {
        return None;
    }
    let (r, g, b) = hsl_to_rgb(h.value, s.value / 100.0, l.value / 100.0);
    Some(paint(r, g, b, alpha_channel(alpha)?))
}

fn channel(num: Num) -> Option<u8> {
    if num.percent {
        if !(0.0..=100.0).contains(&num.value) {
            return None;
        }
        return Some((num.value / 100.0 * 255.0).round() as u8);
    }
    if !(0.0..=255.0).contains(&num.value) {
        return None;
    }
    Some(num.value.round() as u8)
}

fn alpha_channel(alpha: Option<Num>) -> Option<u8> {
    let Some(num) = alpha else {
        return Some(255);
    };
    if num.percent {
        if !(0.0..=100.0).contains(&num.value) {
            return None;
        }
        return Some((num.value / 100.0 * 255.0).round() as u8);
    }
    if !(0.0..=1.0).contains(&num.value) {
        return None;
    }
    Some((num.value * 255.0).round() as u8)
}

fn paint(r: u8, g: u8, b: u8, a: u8) -> FoundColor {
    let (hue, sat, light) = to_hsl(r, g, b);
    if a == 255 {
        FoundColor {
            hex: format!("#{r:02X}{g:02X}{b:02X}"),
            rgb: format!("rgb({r}, {g}, {b})"),
            hsl: format!("hsl({hue}, {sat}%, {light}%)"),
        }
    } else {
        let alpha = alpha_text(a);
        FoundColor {
            hex: format!("#{r:02X}{g:02X}{b:02X}{a:02X}"),
            rgb: format!("rgba({r}, {g}, {b}, {alpha})"),
            hsl: format!("hsla({hue}, {sat}%, {light}%, {alpha})"),
        }
    }
}

fn alpha_text(alpha: u8) -> String {
    let mut text = format!("{:.2}", alpha as f64 / 255.0);
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}

fn to_hsl(r: u8, g: u8, b: u8) -> (u16, u16, u16) {
    let r = r as f64 / 255.0;
    let g = g as f64 / 255.0;
    let b = b as f64 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let light = (max + min) / 2.0;
    let delta = max - min;
    if delta == 0.0 {
        return (0, 0, (light * 100.0).round() as u16);
    }
    let sat = delta / (1.0 - (2.0 * light - 1.0).abs());
    let hue = if max == r {
        let mut turn = (g - b) / delta;
        if turn < 0.0 {
            turn += 6.0;
        }
        turn
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    let hue = (hue * 60.0).round() as u16 % 360;
    (hue, (sat * 100.0).round() as u16, (light * 100.0).round() as u16)
}

fn hsl_to_rgb(hue: f64, sat: f64, light: f64) -> (u8, u8, u8) {
    let hue = if hue >= 360.0 { 0.0 } else { hue };
    let chroma = (1.0 - (2.0 * light - 1.0).abs()) * sat;
    let x = chroma * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let m = light - chroma / 2.0;
    let (r, g, b) = match hue {
        h if h < 60.0 => (chroma, x, 0.0),
        h if h < 120.0 => (x, chroma, 0.0),
        h if h < 180.0 => (0.0, chroma, x),
        h if h < 240.0 => (0.0, x, chroma),
        h if h < 300.0 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    (
        ((r + m) * 255.0).round() as u8,
        ((g + m) * 255.0).round() as u8,
        ((b + m) * 255.0).round() as u8,
    )
}

fn number_at(bytes: &[u8], index: usize) -> Option<(Num, usize)> {
    let index = skip_ws(bytes, index);
    let mut end = index;
    if end < bytes.len() && (bytes[end] == b'+' || bytes[end] == b'-') {
        end += 1;
    }
    let start_digits = end;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    if end < bytes.len() && bytes[end] == b'.' {
        end += 1;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
    }
    if end == start_digits || (end == start_digits + 1 && bytes[start_digits] == b'.') {
        return None;
    }
    let value: f64 = std::str::from_utf8(&bytes[index..end]).ok()?.parse().ok()?;
    let percent = end < bytes.len() && bytes[end] == b'%';
    if percent {
        end += 1;
    }
    Some((Num { value, percent }, end))
}

fn separator(bytes: &[u8], index: usize) -> Option<usize> {
    let next = skip_ws(bytes, index);
    if next > index {
        if next < bytes.len() && bytes[next] == b',' {
            return Some(skip_ws(bytes, next + 1));
        }
        return Some(next);
    }
    if index < bytes.len() && bytes[index] == b',' {
        return Some(skip_ws(bytes, index + 1));
    }
    None
}

fn skip_ws(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    index
}

fn starts_ci(bytes: &[u8], prefix: &[u8]) -> bool {
    bytes.len() >= prefix.len()
        && bytes[..prefix.len()]
            .iter()
            .zip(prefix)
            .all(|(got, want)| got.to_ascii_lowercase() == *want)
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn doubled(digit: u8) -> u8 {
    let value = hex_val(digit);
    value * 16 + value
}

fn pair(high: u8, low: u8) -> u8 {
    hex_val(high) * 16 + hex_val(low)
}

fn hex_val(digit: u8) -> u8 {
    match digit {
        b'0'..=b'9' => digit - b'0',
        b'a'..=b'f' => digit - b'a' + 10,
        _ => digit - b'A' + 10,
    }
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
    use super::{browser_url, classify, find_colors, present_code};

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
    fn browser_url_keeps_http_and_prefixes_www() {
        assert_eq!(
            browser_url("https://example.com/a?q=1"),
            Some("https://example.com/a?q=1".into())
        );
        assert_eq!(
            browser_url("www.example.com"),
            Some("https://www.example.com".into())
        );
        assert_eq!(browser_url("not a url"), None);
        assert_eq!(browser_url("javascript:alert(1)"), None);
    }

    #[test]
    fn json_views_keep_key_order_and_original() {
        let raw = "{\"b\":1,\"a\":[2, 3]}";
        let pretty = present_code(raw, Some("json"), "pretty");
        assert!(pretty.contains('\n'));
        assert!(pretty.find("\"b\"").unwrap() < pretty.find("\"a\"").unwrap());
        let compact = present_code(raw, Some("json"), "compact");
        assert_eq!(compact, "{\"b\":1,\"a\":[2,3]}");
        assert_eq!(present_code(raw, Some("json"), "raw"), raw);
    }

    #[test]
    fn colors_inside_text_and_code() {
        let found = find_colors("{\"accent\":\"#5B6CFF\"}");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].hex, "#5B6CFF");
        assert_eq!(found[0].rgb, "rgb(91, 108, 255)");
        assert_eq!(found[0].hsl, "hsl(234, 100%, 68%)");
        assert_eq!(find_colors("主题色 rgb(91, 108, 255)").len(), 1);
        assert!(find_colors("订单 #123 已发出").is_empty());
        assert!(find_colors("rgb(300, 0, 0)").is_empty());
        let many = find_colors("#112233 #445566 rgb(1, 2, 3) hsl(200, 50%, 40%)");
        assert_eq!(many.len(), 4);
        assert_eq!(many[0].hex, "#112233");
        assert_eq!(many[1].hex, "#445566");
    }

    #[test]
    fn sql_compact_is_one_line_and_pretty_breaks_keywords() {
        let raw = "select id from users where id = 1";
        let compact = present_code(raw, Some("sql"), "compact");
        assert!(!compact.contains('\n'));
        let pretty = present_code(raw, Some("sql"), "pretty");
        assert!(pretty.contains("\nfrom") || pretty.contains("\nFROM") || pretty.to_ascii_lowercase().contains("\nfrom"));
        assert!(pretty.to_ascii_lowercase().contains("\nwhere"));
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
