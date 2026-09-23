/// Map a Pest snake_case rule name to a PascalCase callable (`lower_run` → `ParseLowerRun`).
pub fn rule_name_to_callable(rule_name: &str) -> String {
    let mut parts = rule_name.split('_').filter(|part| !part.is_empty());
    let Some(first) = parts.next() else {
        return "Parse".to_string();
    };
    let mut pascal = String::new();
    pascal.push_str(&capitalize_ascii(first));
    for part in parts {
        pascal.push_str(&capitalize_ascii(part));
    }
    format!("Parse{pascal}")
}

fn capitalize_ascii(text: &str) -> String {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut out = String::new();
    out.extend(first.to_uppercase());
    out.extend(chars);
    out
}
