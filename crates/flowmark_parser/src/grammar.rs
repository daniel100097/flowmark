//! The comment grammar (SPEC §7.1) and ref targets (§8.1). Pure string parsing.
//!
//! ```text
//! comment   = "//" [ level ] [ sigil | tag ] text
//! level     = "L1" | "L2" | "L3"
//! sigil     = "!" | "!?" | "?"            followed by a space
//! tag       = "tested:" | "used by:"
//! text      = free text, may contain refs anywhere
//! ref       = "@" SPACE [ target ] [ SPACE quote ]
//! ```

use crate::model::{Sigil, Tag, Target};

#[derive(Debug, Default, PartialEq)]
pub struct ParsedComment {
    /// The level number as written (also invalid ones like `0` or `7`).
    pub level: Option<u8>,
    pub sigil: Option<Sigil>,
    pub tag: Option<Tag>,
    /// Prose without marker, sigil, tag and refs.
    pub text: String,
    pub refs: Vec<ParsedRef>,
    pub errors: Vec<String>,
}

#[derive(Debug, PartialEq)]
pub struct ParsedRef {
    pub raw: String,
    pub target: Option<String>,
    pub quote: Option<String>,
}

/// Parses the body of a `//` comment (the text after `//`).
pub fn parse_comment(body: &str) -> ParsedComment {
    let mut out = ParsedComment::default();
    let mut rest = body.trim_start();

    // level
    if let Some(after) = rest.strip_prefix('L') {
        let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
        let tail = &after[digits.len()..];
        if !digits.is_empty() && (tail.is_empty() || tail.starts_with(char::is_whitespace)) {
            let n: u32 = digits.parse().unwrap_or(u32::MAX);
            if (1..=3).contains(&n) {
                out.level = Some(n as u8);
            } else {
                out.errors
                    .push(format!("level marker L{digits} is not one of L1, L2, L3"));
            }
            rest = tail.trim_start();
        }
    }

    // sigil or tag
    let word_end = |s: &str, n: usize| s.len() == n || s[n..].starts_with(char::is_whitespace);
    if rest.starts_with("!?") && word_end(rest, 2) {
        out.sigil = Some(Sigil::PossibleRisk);
        rest = rest[2..].trim_start();
    } else if rest.starts_with('!') && word_end(rest, 1) {
        out.sigil = Some(Sigil::Risk);
        rest = rest[1..].trim_start();
    } else if rest.starts_with('?') && word_end(rest, 1) {
        out.sigil = Some(Sigil::Unsure);
        rest = rest[1..].trim_start();
    } else if let Some(r) = rest.strip_prefix("tested:") {
        out.tag = Some(Tag::Tested);
        rest = r.trim_start();
    } else if let Some(r) = rest.strip_prefix("used by:") {
        out.tag = Some(Tag::UsedBy);
        rest = r.trim_start();
    }

    // refs anywhere in the rest
    let mut text = String::new();
    let chars: Vec<(usize, char)> = rest.char_indices().collect();
    let mut i = 0;
    let mut copied_until = 0;
    while i < chars.len() {
        let (pos, c) = chars[i];
        let at_word_start = i == 0 || chars[i - 1].1.is_whitespace();
        let followed_by_space = chars.get(i + 1).is_none_or(|(_, n)| n.is_whitespace());
        if c == '@' && at_word_start && followed_by_space {
            text.push_str(&rest[copied_until..pos]);
            let (parsed, consumed_to) = parse_ref(rest, pos, &mut out.errors);
            out.refs.push(parsed);
            copied_until = consumed_to;
            while i < chars.len() && chars[i].0 < consumed_to {
                i += 1;
            }
            continue;
        }
        i += 1;
    }
    text.push_str(&rest[copied_until..]);
    out.text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    out
}

/// Parses a ref starting at the `@` at byte `at`. Returns the ref and the byte offset after it.
fn parse_ref(s: &str, at: usize, errors: &mut Vec<String>) -> (ParsedRef, usize) {
    let mut p = at + 1;
    p = skip_ws(s, p);
    let mut target = None;
    let mut quote = None;
    if p < s.len() && !s[p..].starts_with('"') {
        let end = s[p..].find(char::is_whitespace).map_or(s.len(), |e| p + e);
        target = Some(s[p..end].to_string());
        p = end;
        let q = skip_ws(s, p);
        if s[q..].starts_with('"') {
            p = q;
        }
    }
    if s[p..].starts_with('"') {
        let (q, end) = parse_quote(s, p, errors);
        quote = Some(q);
        p = end;
    }
    if target.is_none() && quote.is_none() {
        errors.push("`@` without a target or a quote".to_string());
    }
    let raw = s[at..p].trim_end().to_string();
    (ParsedRef { raw, target, quote }, p)
}

/// Parses a `"..."` quote at `start` (which is `"`). Supports `\"` and `\\`.
fn parse_quote(s: &str, start: usize, errors: &mut Vec<String>) -> (String, usize) {
    let mut out = String::new();
    let mut chars = s[start + 1..].char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some((_, n @ ('"' | '\\'))) => out.push(n),
                Some((_, n)) => {
                    out.push('\\');
                    out.push(n);
                }
                None => out.push('\\'),
            },
            '"' => return (out, start + 1 + i + 1),
            _ => out.push(c),
        }
    }
    errors.push("unterminated quote in ref".to_string());
    (out, s.len())
}

fn skip_ws(s: &str, mut p: usize) -> usize {
    while let Some(c) = s[p..].chars().next() {
        if !c.is_whitespace() {
            break;
        }
        p += c.len_utf8();
    }
    p
}

/// File extensions that make a ref target a path (§8.1): `UserService.php` is a path,
/// `UserResource.toArray` is a symbol.
const PATH_EXTENSIONS: &[&str] = &[
    "php", "ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs", "py", "rb", "go", "rs", "java",
    "kt", "kts", "scala", "cs", "fs", "vb", "swift", "m", "mm", "c", "h", "cc", "cpp", "cxx",
    "hpp", "ex", "exs", "erl", "hrl", "hs", "ml", "mli", "clj", "cljs", "dart", "lua", "pl", "pm",
    "sh", "bash", "zsh", "sql", "yaml", "yml", "json", "toml", "xml", "html", "htm", "vue",
    "svelte", "astro", "graphql", "gql", "proto", "tf", "md", "txt", "ini", "env", "cfg", "conf",
    "gradle", "r", "jl", "zig", "nim", "v", "sol", "twig", "erb", "haml", "jinja", "j2",
];

/// Splits a ref target into path and canonical symbol.
pub fn parse_target(raw: &str) -> Target {
    let (path, symbol) = match raw.split_once('#') {
        Some((p, s)) => (Some(p), Some(s)),
        None if is_path(raw) => (Some(raw), None),
        None => (None, Some(raw)),
    };
    Target {
        raw: raw.to_string(),
        path: path.filter(|p| !p.is_empty()).map(str::to_string),
        symbol: symbol.filter(|s| !s.is_empty()).map(canonical_symbol),
    }
}

fn is_path(raw: &str) -> bool {
    if raw.contains('/') {
        return true;
    }
    match raw.rsplit_once('.') {
        Some((_, ext)) => PATH_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()),
        None => false,
    }
}

/// `UserService::create` / `UserService->create` → `UserService.create`.
pub fn canonical_symbol(s: &str) -> String {
    s.replace("::", ".").replace("->", ".")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(target: Option<&str>, quote: Option<&str>) -> (Option<String>, Option<String>) {
        (target.map(str::to_string), quote.map(str::to_string))
    }
    fn refs(p: &ParsedComment) -> Vec<(Option<String>, Option<String>)> {
        p.refs
            .iter()
            .map(|r| (r.target.clone(), r.quote.clone()))
            .collect()
    }

    #[test]
    fn plain_text() {
        let p = parse_comment(" required, max 255");
        assert_eq!(p.text, "required, max 255");
        assert!(p.refs.is_empty() && p.sigil.is_none() && p.level.is_none());
    }

    #[test]
    fn ref_then_prose() {
        let p = parse_comment(" @ StoreUserRequest.rules fails with 422");
        assert_eq!(refs(&p), vec![r(Some("StoreUserRequest.rules"), None)]);
        assert_eq!(p.text, "fails with 422");
    }

    #[test]
    fn ref_with_quote() {
        let p = parse_comment(r#" @ routes/api.php "Route::post('/users'""#);
        assert_eq!(
            refs(&p),
            vec![r(Some("routes/api.php"), Some("Route::post('/users'"))]
        );
        assert_eq!(p.text, "");
    }

    #[test]
    fn quote_only_with_level() {
        let p = parse_comment(r#" L3 @ "Str::lower(""#);
        assert_eq!(p.level, Some(3));
        assert_eq!(refs(&p), vec![r(None, Some("Str::lower("))]);
    }

    #[test]
    fn risk_with_trailing_ref() {
        let p = parse_comment(r#" ! mass-assignable: can set role=admin @ "protected $fillable""#);
        assert_eq!(p.sigil, Some(Sigil::Risk));
        assert_eq!(p.text, "mass-assignable: can set role=admin");
        assert_eq!(refs(&p), vec![r(None, Some("protected $fillable"))]);
    }

    #[test]
    fn sigils() {
        assert_eq!(
            parse_comment(" !? may fire twice").sigil,
            Some(Sigil::PossibleRisk)
        );
        assert_eq!(parse_comment(" ? not sure").sigil, Some(Sigil::Unsure));
        // `!important` is not a sigil: no space after it.
        assert_eq!(parse_comment(" !important").sigil, None);
    }

    #[test]
    fn tags() {
        let p = parse_comment(
            r#" used by: admin user creation @ AdminUserController.store "createUser(""#,
        );
        assert_eq!(p.tag, Some(Tag::UsedBy));
        assert_eq!(p.text, "admin user creation");
        assert_eq!(
            refs(&p),
            vec![r(Some("AdminUserController.store"), Some("createUser("))]
        );
        assert_eq!(
            parse_comment(" tested: @ a.php \"x\"").tag,
            Some(Tag::Tested)
        );
    }

    #[test]
    fn not_refs() {
        let p = parse_comment(" mail a@b.com and @param x");
        assert!(p.refs.is_empty());
        assert_eq!(p.text, "mail a@b.com and @param x");
    }

    #[test]
    fn several_refs_and_escapes() {
        let p = parse_comment(r#" @ A.b @ C.d "say \"hi\"""#);
        assert_eq!(
            refs(&p),
            vec![r(Some("A.b"), None), r(Some("C.d"), Some("say \"hi\""))]
        );
    }

    #[test]
    fn errors() {
        assert!(!parse_comment(" @ ").errors.is_empty());
        assert!(!parse_comment(r#" @ "open"#).errors.is_empty());
        assert!(!parse_comment(" L0 x").errors.is_empty());
        // "Layer" is not a level marker.
        assert_eq!(parse_comment(" Layer 2").level, None);
    }

    #[test]
    fn targets() {
        let t = parse_target("app/Services/UserService.php#UserService::create");
        assert_eq!(t.path.as_deref(), Some("app/Services/UserService.php"));
        assert_eq!(t.symbol.as_deref(), Some("UserService.create"));
        assert_eq!(
            parse_target("UserService.php").path.as_deref(),
            Some("UserService.php")
        );
        let s = parse_target("UserResource.toArray");
        assert_eq!(
            (s.path, s.symbol.as_deref()),
            (None, Some("UserResource.toArray"))
        );
        assert_eq!(parse_target("Foo->bar").symbol.as_deref(), Some("Foo.bar"));
    }
}
