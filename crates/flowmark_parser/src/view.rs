//! Text rendering of a document at a detail level (SPEC §9).
//!
//! - L0: the card, the list of risks and the diagnostics.
//! - L1–L3: lines at or below the level. Hidden blocks become one placeholder line that says what is
//!   inside (§9.4), hidden risks are shown under the nearest visible line, and calls to hidden helpers
//!   get the helper's badges. Refs are left out (a viewer shows them as links).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;

use crate::model::*;
use crate::text::LineIndex;

pub fn render(doc: &Document, source: &str, level: u8) -> String {
    if level == 0 {
        render_card(doc)
    } else {
        Renderer::new(doc, source, level).render()
    }
}

// ─── L0 ─────────────────────────────────────────────────────────────────────

fn render_card(doc: &Document) -> String {
    let mut out = String::new();
    if let Some(card) = &doc.card {
        let _ = writeln!(out, "{}", card.title);
        for s in &card.summary {
            let _ = writeln!(out, "{s}");
        }
        out.push('\n');
        for f in &card.fields {
            let _ = writeln!(out, "{:<10}{}", format!("{}:", f.name), f.raw);
        }
    }
    let lines = |items: Vec<(String, String)>, out: &mut String| {
        let width = items
            .iter()
            .map(|(t, _)| t.chars().count())
            .max()
            .unwrap_or(0)
            .min(76);
        for (text, target) in items {
            let pad = width.saturating_sub(text.chars().count());
            let _ = writeln!(out, "  {text}{}  → {target}", " ".repeat(pad));
        }
    };
    if !doc.risks.is_empty() {
        out.push_str("\nrisks\n");
        let items = doc
            .risks
            .iter()
            .map(|r| {
                (
                    truncate(&format!("{} {}", r.kind.as_str(), r.text), 76),
                    r.owner.clone().unwrap_or_default(),
                )
            })
            .collect();
        lines(items, &mut out);
    }
    if !doc.diagnostics.is_empty() {
        out.push_str("\nchecker\n");
        let items = doc
            .diagnostics
            .iter()
            .map(|d| {
                let mark = if d.severity == Severity::Error {
                    "✗"
                } else {
                    "⚠"
                };
                (
                    truncate(&format!("{mark} {} {}", d.code, d.message), 76),
                    format!("line {}", d.line),
                )
            })
            .collect();
        lines(items, &mut out);
    }
    out
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    }
}

// ─── L1–L3 ──────────────────────────────────────────────────────────────────

struct Renderer<'a> {
    doc: &'a Document,
    source: &'a str,
    idx: LineIndex,
    level: u8,
    children: Vec<Vec<usize>>,
    /// Top-level functions by name.
    functions: HashMap<&'a str, usize>,
    /// Comments by start line.
    comments_at: BTreeMap<u32, Vec<usize>>,
}

struct Placeholder {
    units: Vec<usize>,
}

impl<'a> Renderer<'a> {
    fn new(doc: &'a Document, source: &'a str, level: u8) -> Self {
        let mut children = vec![Vec::new(); doc.units.len()];
        for u in &doc.units {
            if let Some(p) = u.parent {
                children[p].push(u.id);
            }
        }
        let functions = doc
            .functions
            .iter()
            .map(|f| (f.name.as_str(), f.unit))
            .collect();
        let mut comments_at: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
        for c in &doc.comments {
            comments_at.entry(c.span.start_line).or_default().push(c.id);
        }
        Renderer {
            doc,
            source,
            idx: LineIndex::new(source),
            level,
            children,
            functions,
            comments_at,
        }
    }

    fn hidden(&self, u: usize) -> bool {
        self.doc.units[u].level > self.level
    }

    fn subtree(&self, u: usize) -> Vec<usize> {
        let mut out = vec![u];
        let mut i = 0;
        while i < out.len() {
            out.extend(self.children[out[i]].iter().copied());
            i += 1;
        }
        out
    }

    fn render(&self) -> String {
        let doc = self.doc;
        let n = self.idx.line_count();
        let mut out: Vec<String> = Vec::new();

        // Visible units whose inside is completely hidden render as one line (`type User = { … 5 fields }`).
        let mut collapsed: HashMap<u32, usize> = HashMap::new();
        for u in &doc.units {
            if !self.hidden(u.id)
                && u.span.end_line > u.span.start_line + 1
                && self.interior_hidden(u.id)
            {
                collapsed.entry(u.span.start_line).or_insert(u.id);
            }
        }
        let mut skip_until = 0;

        // Hidden blocks whose parent is visible, grouped into runs of adjacent siblings.
        let called: HashSet<&str> = doc.calls.iter().map(|c| c.callee.as_str()).collect();
        let mut placeholder_at: HashMap<u32, Placeholder> = HashMap::new();
        let mut hidden_lines: HashSet<u32> = HashSet::new();
        let mut last_run: Option<(Option<usize>, u32, u32)> = None; // (parent, start line, end line)
        for u in &doc.units {
            let visible_parent = u.parent.is_none_or(|p| !self.hidden(p));
            if !self.hidden(u.id) || !visible_parent {
                continue;
            }
            for l in u.span.start_line..=u.span.end_line {
                hidden_lines.insert(l);
            }
            // Own-line comments directly above a hidden unit belong to it.
            for &c in &u.comments {
                let c = &doc.comments[c];
                if c.placement == Placement::OwnLine && c.level.is_some_and(|l| l > self.level) {
                    for l in c.span.start_line..=c.span.end_line {
                        hidden_lines.insert(l);
                    }
                }
            }
            // Helpers are shown at their call sites instead.
            let is_called_helper = u.parent.is_none()
                && doc
                    .functions
                    .iter()
                    .any(|f| f.unit == u.id && called.contains(f.name.as_str()));
            if is_called_helper {
                continue;
            }
            let adjacent = last_run.is_some_and(|(p, _, end)| {
                p == u.parent
                    && (end + 1..u.span.start_line)
                        .all(|l| self.blank(l) || hidden_lines.contains(&l))
            });
            match (adjacent, last_run) {
                (true, Some((p, start, _))) => {
                    placeholder_at.get_mut(&start).unwrap().units.push(u.id);
                    last_run = Some((p, start, u.span.end_line));
                }
                _ => {
                    placeholder_at.insert(u.span.start_line, Placeholder { units: vec![u.id] });
                    last_run = Some((u.parent, u.span.start_line, u.span.end_line));
                }
            }
        }

        for line in 1..=n {
            if line <= skip_until {
                continue;
            }
            if let Some(p) = placeholder_at.get(&line) {
                self.emit_placeholder(p, line, &mut out);
                continue;
            }
            if hidden_lines.contains(&line) {
                continue;
            }
            let text = self.idx.line_text(self.source, line);
            if text.trim().is_empty() {
                if out.last().is_some_and(|l| !l.is_empty()) {
                    out.push(String::new());
                }
                continue;
            }
            // Own-line comments: version, card, sections, notes.
            if let Some(c) = self.own_line_comment(line) {
                let c = &doc.comments[c];
                match c.role {
                    CommentRole::Version | CommentRole::Card => {}
                    CommentRole::Section => {
                        if c.level.is_some_and(|l| l <= self.level) {
                            out.push(text.trim_end().to_string());
                        }
                    }
                    CommentRole::Note => {
                        if c.level.is_some_and(|l| l <= self.level) && c.span.start_line == line {
                            let rendered = self.comment_text(c);
                            if !rendered.is_empty() {
                                out.push(format!("{}// {rendered}", indent(text)));
                            }
                        }
                    }
                }
                continue;
            }
            // Code line.
            if let Some(&u) = collapsed.get(&line) {
                let unit = &doc.units[u];
                let first = self.code_part(line);
                let last = self.code_part(unit.span.end_line);
                let inner: Vec<usize> = self.children[u].clone();
                let summary = self.count_label(u, &inner);
                let mut l = format!("{} … {summary} {}", first.trim_end(), last.trim());
                let all: Vec<usize> = inner.iter().flat_map(|&c| self.subtree(c)).collect();
                let badges = self.badges(&all);
                if !badges.is_empty() {
                    let _ = write!(l, "  ▸ {badges}");
                }
                out.push(l);
                self.push_risks(&all, &format!("{}  ", indent(text)), &mut out);
                skip_until = unit.span.end_line;
                continue;
            }
            let mut l = self.code_part(line).trim_end().to_string();
            let eol: Vec<String> = self
                .comments_at
                .get(&line)
                .into_iter()
                .flatten()
                .map(|&c| &doc.comments[c])
                .filter(|c| {
                    c.placement == Placement::EndOfLine
                        && c.level.is_some_and(|lv| lv <= self.level)
                })
                .map(|c| self.comment_text(c))
                .filter(|t| !t.is_empty())
                .collect();
            if !eol.is_empty() {
                let _ = write!(l, "  // {}", eol.join(" "));
            }
            // Calls to hidden helpers: one badge group for all of them.
            let mut helper_risks: Vec<usize> = Vec::new();
            for call in doc.calls.iter().filter(|c| c.line == line) {
                if let Some(&f) = self.functions.get(call.callee.as_str())
                    && self.hidden(f)
                    && !helper_risks.contains(&f)
                {
                    helper_risks.extend(self.subtree(f));
                }
            }
            let badges = self.badges(&helper_risks);
            if !badges.is_empty() {
                let _ = write!(l, "  ▸ {badges}");
            }
            out.push(l);
            if !helper_risks.is_empty() {
                self.push_risks(&helper_risks, &format!("{}  ", indent(text)), &mut out);
            }
        }
        while out.last().is_some_and(String::is_empty) {
            out.pop();
        }
        while out.first().is_some_and(String::is_empty) {
            out.remove(0);
        }
        let mut s = out.join("\n");
        s.push('\n');
        s
    }

    fn emit_placeholder(&self, p: &Placeholder, line: u32, out: &mut Vec<String>) {
        let doc = self.doc;
        let all: Vec<usize> = p.units.iter().flat_map(|&u| self.subtree(u)).collect();
        let lines: usize = p
            .units
            .iter()
            .map(|&u| {
                let s = doc.units[u].span;
                (s.start_line..=s.end_line)
                    .filter(|&l| !self.blank(l))
                    .count()
            })
            .sum();
        let plural = if lines == 1 { "line" } else { "lines" };
        let what = match p.units.as_slice() {
            [u] if doc.units[*u].parent.is_none() => {
                format!("{} ({lines} {plural})", doc.units[*u].label)
            }
            _ => format!("{lines} {plural}"),
        };
        let ind = indent(self.idx.line_text(self.source, line));
        let mut l = format!("{ind}⋯ {what}");
        let badges = self.badges(&all);
        if !badges.is_empty() {
            let _ = write!(l, "  ▸ {badges}");
        }
        out.push(l);
        self.push_risks(&all, &format!("{ind}  "), out);
    }

    /// `n fields` / `n events` / `n lines` for a collapsed unit.
    fn count_label(&self, u: usize, inner: &[usize]) -> String {
        let unit = &self.doc.units[u];
        let fields = inner
            .iter()
            .filter(|&&c| self.doc.units[c].kind == UnitKind::Field)
            .count();
        let (n, word) = if fields == inner.len() && fields > 0 {
            let word = match unit.label.as_str() {
                "interface Events" => "event",
                "interface Jobs" => "job",
                _ => "field",
            };
            (fields, word)
        } else {
            (inner.len(), "statement")
        };
        format!("{n} {word}{}", if n == 1 { "" } else { "s" })
    }

    /// All lines strictly inside `u` are blank or hidden, and at least one child is hidden.
    fn interior_hidden(&self, u: usize) -> bool {
        let s = self.doc.units[u].span;
        let any_hidden_child = self.children[u].iter().any(|&c| self.hidden(c));
        any_hidden_child
            && (s.start_line + 1..s.end_line).all(|l| {
                self.blank(l) || self.doc.lines[(l - 1) as usize].is_some_and(|lv| lv > self.level)
            })
    }

    fn blank(&self, line: u32) -> bool {
        self.idx.line_text(self.source, line).trim().is_empty()
    }

    fn own_line_comment(&self, line: u32) -> Option<usize> {
        self.doc
            .comments
            .iter()
            .find(|c| {
                c.placement == Placement::OwnLine
                    && c.span.start_line <= line
                    && line <= c.span.end_line
            })
            .map(|c| c.id)
    }

    /// The line without its end-of-line comments.
    fn code_part(&self, line: u32) -> String {
        let text = self.idx.line_text(self.source, line);
        let line_start = self.idx.line_start(line);
        let cut = self
            .comments_at
            .get(&line)
            .into_iter()
            .flatten()
            .map(|&c| &self.doc.comments[c])
            .filter(|c| c.placement == Placement::EndOfLine)
            .map(|c| (c.span.start - line_start) as usize)
            .min();
        match cut {
            Some(cut) => text[..cut.min(text.len())].trim_end().to_string(),
            None => text.trim_end().to_string(),
        }
    }

    /// What a comment shows in the text view: sigil/tag and prose, no refs or markers.
    fn comment_text(&self, c: &Comment) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(s) = c.sigil {
            parts.push(s.as_str().to_string());
        }
        if let Some(t) = c.tag {
            parts.push(t.as_str().to_string());
        }
        if !c.text.is_empty() {
            parts.push(c.text.clone());
        } else if c.tag.is_some() {
            // `tested: @ path "name"`: show where it points, since there is no prose.
            for &r in &c.refs {
                if let Some(t) = &self.doc.refs[r].target {
                    parts.push(t.raw.clone());
                }
            }
        }
        if parts.len() == 1 && c.sigil.is_some() && c.text.is_empty() {
            return String::new();
        }
        parts.join(" ")
    }

    fn badges(&self, units: &[usize]) -> String {
        let set: HashSet<usize> = units.iter().copied().collect();
        let mut groups: Vec<(&str, Vec<String>)> = Vec::new();
        let mut add = |verb: &'static str, name: Option<&String>| {
            let i = match groups.iter().position(|(v, _)| *v == verb) {
                Some(i) => i,
                None => {
                    groups.push((verb, Vec::new()));
                    groups.len() - 1
                }
            };
            if let Some(n) = name
                && !groups[i].1.contains(n)
            {
                groups[i].1.push(n.clone());
            }
        };
        for e in self
            .doc
            .effects
            .iter()
            .filter(|e| e.unit.is_some_and(|u| set.contains(&u)))
        {
            let verb = match (e.category.as_str(), e.call.as_str()) {
                ("access", _) => "authorizes",
                ("input", _) => "validates",
                ("read", _) => "reads",
                ("write", _) => "writes",
                ("effect", "emit") => "emits",
                ("effect", "queue") => "queues",
                ("effect", "mail") => "mails",
                ("effect", "notify") => "notifies",
                ("effect", "external") => "calls",
                ("effect", "log") => "logs",
                ("failure", _) => "fails",
                ("consistency", "db.transaction") => "transaction",
                ("consistency", "lock") => "lock",
                _ => continue,
            };
            add(verb, e.name.as_ref());
        }
        let order = [
            "authorizes",
            "validates",
            "reads",
            "writes",
            "transaction",
            "lock",
            "emits",
            "queues",
            "mails",
            "notifies",
            "calls",
            "logs",
            "fails",
        ];
        groups.sort_by_key(|(v, _)| order.iter().position(|o| o == v));
        let mut parts: Vec<String> = groups
            .into_iter()
            .map(|(v, names)| {
                if names.is_empty() {
                    v.to_string()
                } else {
                    format!("{v} {}", names.join(", "))
                }
            })
            .collect();
        let comments: Vec<&Comment> = self
            .doc
            .comments
            .iter()
            .filter(|c| c.unit.is_some_and(|u| set.contains(&u)))
            .collect();
        let risks = comments
            .iter()
            .filter(|c| c.sigil.is_some_and(Sigil::is_risk))
            .count();
        let unsure = comments
            .iter()
            .filter(|c| c.sigil == Some(Sigil::Unsure))
            .count();
        if risks > 0 {
            parts.push(format!("{risks} !"));
        }
        if unsure > 0 {
            parts.push(format!("{unsure} ?"));
        }
        parts.join(" · ")
    }

    /// Risks attached to `units` that are hidden at this level, shown under the placeholder or call.
    fn push_risks(&self, units: &[usize], indent: &str, out: &mut Vec<String>) {
        let set: HashSet<usize> = units.iter().copied().collect();
        for r in &self.doc.risks {
            let Some(u) = r.unit else { continue };
            let comment_level = self.doc.comments[r.comment].level.unwrap_or(1);
            if set.contains(&u) && comment_level > self.level {
                out.push(format!("{indent}{} {}", r.kind.as_str(), r.text));
            }
        }
    }
}

fn indent(line: &str) -> String {
    line.chars().take_while(|c| c.is_whitespace()).collect()
}
