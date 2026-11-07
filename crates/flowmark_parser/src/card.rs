//! The review card (SPEC §5): the first `/** ... */` block.

use crate::model::{Card, CardField, CardItem, Span};

/// Card fields in their required order, and whether each is required.
pub const FIELDS: &[(&str, bool)] = &[
    ("access", true),
    ("input", true),
    ("reads", false),
    ("writes", true),
    ("effects", true),
    ("output", true),
    ("config", false),
    ("shared", false),
    ("tests", false),
];

const EFFECT_KINDS: &[&str] = &["event", "queue", "mail", "notify", "external"];

/// Card problems: (line, message).
pub type CardErrors = Vec<(u32, String)>;

/// Parses the raw `/** ... */` text. `first_line` is the line of `/**`.
pub fn parse_card(raw: &str, span: Span) -> (Card, CardErrors) {
    let mut errors = Vec::new();
    let inner = raw.trim_start_matches("/**").trim_end_matches("*/");

    let mut title = String::new();
    let mut summary = Vec::new();
    let mut fields: Vec<CardField> = Vec::new();
    // 0 = title, 1 = summary, 2 = fields
    let mut stage = 0;

    for (i, line) in inner.split('\n').enumerate() {
        let line_no = span.start_line + i as u32;
        let content = strip_star(line);
        if content.is_empty() {
            if stage == 1 && !summary.is_empty() {
                stage = 2;
            }
            continue;
        }
        if let Some((name, value)) = split_field(content) {
            stage = 2;
            fields.push(CardField {
                name,
                line: line_no,
                raw: value,
                items: Vec::new(),
            });
            continue;
        }
        match stage {
            0 => {
                title = content.to_string();
                stage = 1;
            }
            1 => summary.push(content.to_string()),
            _ => match fields.last_mut() {
                // A continuation line of the previous field.
                Some(f) => {
                    f.raw.push(' ');
                    f.raw.push_str(content);
                }
                None => errors.push((line_no, format!("unexpected text in the card: `{content}`"))),
            },
        }
    }

    for f in &mut fields {
        f.items = parse_items(&f.raw);
    }

    validate(&fields, span, &mut errors);
    (
        Card {
            span,
            title,
            summary,
            fields,
        },
        errors,
    )
}

fn strip_star(line: &str) -> &str {
    let t = line.trim();
    let t = t.strip_prefix('*').unwrap_or(t);
    t.trim()
}

/// `access:   public` → ("access", "public"). Field names are lowercase words.
fn split_field(content: &str) -> Option<(String, String)> {
    let (name, value) = content.split_once(':')?;
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_lowercase()) {
        return None;
    }
    Some((name.to_string(), value.trim().to_string()))
}

/// Splits a field value on `,` and `→` outside parentheses.
pub fn parse_items(raw: &str) -> Vec<CardItem> {
    if raw.trim() == "none" {
        return Vec::new();
    }
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();
    for c in raw.chars() {
        match c {
            '(' => {
                depth += 1;
                current.push(c);
            }
            ')' => {
                depth -= 1;
                current.push(c);
            }
            ',' | '→' if depth == 0 => parts.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    parts.push(current);
    parts
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .map(parse_item)
        .collect()
}

fn parse_item(s: &str) -> CardItem {
    let mut text = s.trim().to_string();
    let mut risk = false;
    let mut note = None;
    // Strip `!` and `( note )` from the end, in either order.
    loop {
        if let Some(t) = text.strip_suffix('!') {
            risk = true;
            text = t.trim_end().to_string();
        } else if text.ends_with(')') {
            match text.rfind('(') {
                Some(open) => {
                    note = Some(text[open + 1..text.len() - 1].trim().to_string());
                    text = text[..open].trim_end().to_string();
                }
                None => break,
            }
        } else {
            break;
        }
    }
    let (kind, name) = match text.split_once(char::is_whitespace) {
        Some((k, rest)) if EFFECT_KINDS.contains(&k) => {
            (Some(k.to_string()), rest.trim().to_string())
        }
        _ => (None, text.clone()),
    };
    CardItem {
        text,
        kind,
        name,
        note,
        risk,
    }
}

fn validate(fields: &[CardField], span: Span, errors: &mut CardErrors) {
    let order = |name: &str| FIELDS.iter().position(|(n, _)| *n == name);
    let mut last: Option<usize> = None;
    let mut seen = std::collections::HashSet::new();
    for f in fields {
        let Some(pos) = order(&f.name) else {
            errors.push((f.line, format!("unknown card field `{}`", f.name)));
            continue;
        };
        if !seen.insert(f.name.clone()) {
            errors.push((f.line, format!("card field `{}` appears twice", f.name)));
            continue;
        }
        if last.is_some_and(|l| pos < l) {
            errors.push((
                f.line,
                format!(
                    "card field `{}` is out of order (order: {})",
                    f.name,
                    order_list()
                ),
            ));
        }
        last = Some(pos);
        if f.raw.is_empty() {
            errors.push((
                f.line,
                format!("card field `{}` is empty; write `none`", f.name),
            ));
        }
    }
    for (name, required) in FIELDS {
        if *required && !seen.contains(*name) {
            errors.push((
                span.start_line,
                format!("card is missing the required field `{name}`"),
            ));
        }
    }
}

fn order_list() -> String {
    FIELDS
        .iter()
        .map(|(n, _)| *n)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARD: &str = "/**
 * Register user
 * Public signup. Creates the user and their address in one transaction,
 * then sends a welcome mail through the queue.
 *
 * access:   public (registerUser), system (sendWelcomeMail)
 * input:    SignupInput, raw request !
 * writes:   User, Address (1 transaction)
 * effects:  event UserRegistered → mail WelcomeMail (queued)
 * output:   201 UserResponse
 * shared:   createUser, also used by admin user creation
 * tests:    happy path only
 */";

    fn span() -> Span {
        Span {
            start: 0,
            end: CARD.len() as u32,
            start_line: 2,
            end_line: 14,
        }
    }

    #[test]
    fn parses_the_example_card() {
        let (card, errors) = parse_card(CARD, span());
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(card.title, "Register user");
        assert_eq!(card.summary.len(), 2);
        let names: Vec<_> = card.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "access", "input", "writes", "effects", "output", "shared", "tests"
            ]
        );
        assert_eq!(card.fields[0].line, 7);

        let access = &card.fields[0].items;
        assert_eq!(
            (access[0].name.as_str(), access[0].note.as_deref()),
            ("public", Some("registerUser"))
        );
        let input = &card.fields[1].items;
        assert!(input[1].risk && input[1].name == "raw request");
        let writes = &card.fields[2].items;
        assert_eq!(
            writes.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(),
            ["User", "Address"]
        );
        let effects = &card.fields[3].items;
        assert_eq!(effects[0].kind.as_deref(), Some("event"));
        assert_eq!(effects[1].name, "WelcomeMail");
        assert_eq!(effects[1].note.as_deref(), Some("queued"));
    }

    #[test]
    fn validation() {
        let raw = "/**\n * T\n *\n * input: x\n * access: y\n * effects: none\n * colour: red\n */";
        let (_, errors) = parse_card(
            raw,
            Span {
                start: 0,
                end: 0,
                start_line: 1,
                end_line: 8,
            },
        );
        let messages: Vec<_> = errors.iter().map(|(_, m)| m.as_str()).collect();
        assert!(
            messages
                .iter()
                .any(|m| m.contains("`access` is out of order")),
            "{messages:?}"
        );
        assert!(
            messages
                .iter()
                .any(|m| m.contains("unknown card field `colour`"))
        );
        assert!(
            messages
                .iter()
                .any(|m| m.contains("missing the required field `writes`"))
        );
        assert!(
            messages
                .iter()
                .any(|m| m.contains("missing the required field `output`"))
        );
    }

    #[test]
    fn none_has_no_items() {
        assert!(parse_items("none").is_empty());
    }
}
