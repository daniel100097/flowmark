//! The data the interactive viewer renders (`viewer/viewer.js`).
//!
//! The viewer never parses anything: every row is pre-tokenized, every block knows its level, summary,
//! badges and risks, and in diff mode rows carry `added` / `removed` and the model has a summary of what
//! changed (SPEC §11.5). The same model is produced natively (`flowmark html`) and, later, in WebAssembly.

use std::collections::{HashMap, HashSet};

use serde::Serialize;
use similar::{DiffOp, TextDiff};
use tsrs_ast::Kind;
use tsrs_scanner::Scanner;

use crate::model::*;
use crate::text::LineIndex;

/// Bumped when the JSON shape changes.
pub const MODEL_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize)]
pub struct ViewModel {
    pub model_version: u32,
    /// Display name of the (head) file.
    pub file: String,
    /// Diff mode: display name of the base file.
    pub base_file: Option<String>,
    pub title: Option<String>,
    pub card: Option<Card>,
    pub rows: Vec<Row>,
    /// One per unit of the (head) document, same ids as `Document::units`.
    pub blocks: Vec<Block>,
    pub risks: Vec<RiskItem>,
    pub diagnostics: Vec<DiagItem>,
    pub changes: Option<Changes>,
    /// Diff mode only: the base and head files on their own, so a viewer can switch between
    /// "Changes", "Before" and "After".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sides: Option<Box<Sides>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Sides {
    pub base: ViewModel,
    pub head: ViewModel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RowKind {
    Code,
    Comment,
    Section,
    Blank,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Change {
    Added,
    Removed,
}

#[derive(Debug, Clone, Serialize)]
pub struct Row {
    /// Line in the (head) file; `None` for removed rows.
    pub line: Option<u32>,
    /// Diff mode: line in the base file; `None` for added rows.
    pub base_line: Option<u32>,
    pub kind: RowKind,
    pub level: Option<u8>,
    /// Innermost block containing the row.
    pub block: Option<usize>,
    /// Leading whitespace, in columns.
    pub indent: u32,
    /// The code part of the line, highlighted. For comment rows: the leading whitespace.
    pub code: Vec<Segment>,
    /// End-of-line comments (code rows) or the comment of a comment row.
    pub comments: Vec<RowComment>,
    /// Blocks of helper functions called on this row.
    pub calls: Vec<usize>,
    pub change: Option<Change>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Segment {
    pub text: String,
    /// `kw`, `str`, `num`, `type`, `entry`, `helper`, or a prelude category: `c-write`, `c-effect`, ...
    pub class: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RowComment {
    pub level: Option<u8>,
    pub sigil: Option<Sigil>,
    pub tag: Option<Tag>,
    pub marker: Option<u8>,
    pub text: String,
    pub refs: Vec<RefChip>,
    /// Section name for section rows.
    pub section: Option<String>,
    /// Continuation line of a block comment, or a comment without Flowmark structure.
    pub plain: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RefChip {
    pub label: String,
    pub target: Option<String>,
    pub quote: Option<String>,
    /// Where a quote-only ref searches (its scope's target).
    pub resolved: Option<String>,
    pub defines_scope: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Block {
    pub id: usize,
    pub parent: Option<usize>,
    pub level: u8,
    /// First row, including own-line comments attached directly above the unit.
    pub first_row: usize,
    /// Row where the unit itself starts.
    pub head_row: usize,
    pub last_row: usize,
    pub label: String,
    pub kind: UnitKind,
    /// A top-level function that is called from somewhere: shown at its call sites when hidden.
    pub helper: bool,
    /// What the children are: `4 fields`, `1 event`, `3 statements`.
    pub summary: String,
    /// Non-blank lines in the block.
    pub lines: u32,
    /// Effects in the subtree: `writes User, Address`, `emits UserRegistered`, ...
    pub badges: Vec<String>,
    /// Risks (`ViewModel::risks` ids) in the subtree.
    pub risks: Vec<usize>,
    /// `?` comments in the subtree.
    pub unsure: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct RiskItem {
    pub id: usize,
    pub kind: Sigil,
    pub text: String,
    pub owner: Option<String>,
    pub row: usize,
    pub block: Option<usize>,
    pub level: Option<u8>,
    pub change: Option<Change>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiagItem {
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
    pub line: u32,
    pub row: Option<usize>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Changes {
    pub card: Vec<FieldChange>,
    pub risks_added: Vec<String>,
    pub risks_removed: Vec<String>,
    pub entries: Vec<NamedChange>,
    pub types: Vec<NamedChange>,
    pub lines_added: u32,
    pub lines_removed: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldChange {
    pub name: String,
    pub base: Option<String>,
    pub head: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NamedChange {
    pub name: String,
    /// `added`, `removed` or `changed`
    pub change: String,
    pub details: Vec<String>,
}

// ─── single file ────────────────────────────────────────────────────────────

/// The model for one file.
pub fn build(doc: &Document, source: &str, file: &str) -> ViewModel {
    let rows = file_rows(doc, source);
    let line_to_row: Vec<usize> = (0..rows.len()).collect();
    assemble(doc, source, file, rows, &line_to_row, None)
}

/// The model for a change from `base` to `head` (SPEC §11.5).
pub fn build_diff(
    base: &Document,
    base_source: &str,
    base_file: &str,
    head: &Document,
    head_source: &str,
    head_file: &str,
) -> ViewModel {
    let base_rows = file_rows(base, base_source);
    let head_rows = file_rows(head, head_source);
    let mut rows = Vec::new();
    let mut head_line_to_row = vec![0; head_rows.len()];
    let mut base_line_to_row = vec![0; base_rows.len()];

    let diff = TextDiff::from_lines(base_source, head_source);
    for op in diff.ops() {
        match *op {
            DiffOp::Equal {
                old_index,
                new_index,
                len,
            } => {
                for i in 0..len {
                    let mut r = head_rows[new_index + i].clone();
                    r.base_line = Some((old_index + i) as u32 + 1);
                    base_line_to_row[old_index + i] = rows.len();
                    head_line_to_row[new_index + i] = rows.len();
                    rows.push(r);
                }
            }
            DiffOp::Delete {
                old_index, old_len, ..
            } => {
                push_removed(
                    &base_rows,
                    old_index,
                    old_len,
                    &mut rows,
                    &mut base_line_to_row,
                );
            }
            DiffOp::Insert {
                new_index, new_len, ..
            } => {
                push_added(
                    &head_rows,
                    new_index,
                    new_len,
                    &mut rows,
                    &mut head_line_to_row,
                );
            }
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                push_removed(
                    &base_rows,
                    old_index,
                    old_len,
                    &mut rows,
                    &mut base_line_to_row,
                );
                push_added(
                    &head_rows,
                    new_index,
                    new_len,
                    &mut rows,
                    &mut head_line_to_row,
                );
            }
        }
    }

    let changes = changes(base, head, &rows);
    let mut model = assemble(
        head,
        head_source,
        head_file,
        rows,
        &head_line_to_row,
        Some(changes),
    );
    model.base_file = Some(base_file.to_string());
    model.sides = Some(Box::new(Sides {
        base: build(base, base_source, base_file),
        head: build(head, head_source, head_file),
    }));

    // Risks that only exist in the base file.
    let head_risks: HashSet<String> = head.risks.iter().map(risk_key).collect();
    for r in &base.risks {
        if head_risks.contains(&risk_key(r)) {
            continue;
        }
        let row = base_line_to_row[(r.line - 1) as usize];
        let id = model.risks.len();
        model.risks.push(RiskItem {
            id,
            kind: r.kind,
            text: r.text.clone(),
            owner: r.owner.clone(),
            row,
            block: model.rows[row].block,
            level: model.rows[row].level,
            change: Some(Change::Removed),
        });
    }
    let base_risks: HashSet<String> = base.risks.iter().map(risk_key).collect();
    for r in &mut model.risks {
        if r.change.is_none() && !base_risks.contains(&format!("{} {}", r.kind.as_str(), r.text)) {
            r.change = Some(Change::Added);
        }
    }
    model
}

fn push_removed(
    base_rows: &[Row],
    from: usize,
    len: usize,
    rows: &mut Vec<Row>,
    map: &mut [usize],
) {
    for i in from..from + len {
        let mut r = base_rows[i].clone();
        r.base_line = r.line;
        r.line = None;
        r.change = Some(Change::Removed);
        r.calls.clear();
        map[i] = rows.len();
        rows.push(r);
    }
}

fn push_added(head_rows: &[Row], from: usize, len: usize, rows: &mut Vec<Row>, map: &mut [usize]) {
    for i in from..from + len {
        let mut r = head_rows[i].clone();
        r.change = Some(Change::Added);
        map[i] = rows.len();
        rows.push(r);
    }
}

fn risk_key(r: &Risk) -> String {
    format!("{} {}", r.kind.as_str(), r.text)
}

/// Blocks, risks and diagnostics for `doc`, whose line `l` is row `line_to_row[l - 1]`.
fn assemble(
    doc: &Document,
    source: &str,
    file: &str,
    mut rows: Vec<Row>,
    line_to_row: &[usize],
    changes: Option<Changes>,
) -> ViewModel {
    let row_of = |line: u32| line_to_row[(line - 1) as usize];
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); doc.units.len()];
    for u in &doc.units {
        if let Some(p) = u.parent {
            children[p].push(u.id);
        }
    }
    let subtree = |u: usize| {
        let mut out = vec![u];
        let mut i = 0;
        while i < out.len() {
            out.extend(children[out[i]].iter().copied());
            i += 1;
        }
        out
    };
    let called: HashSet<&str> = doc.calls.iter().map(|c| c.callee.as_str()).collect();
    let idx = LineIndex::new(source);

    // Risks first: blocks refer to them.
    let mut risks: Vec<RiskItem> = doc
        .risks
        .iter()
        .enumerate()
        .map(|(id, r)| RiskItem {
            id,
            kind: r.kind,
            text: r.text.clone(),
            owner: r.owner.clone(),
            row: row_of(r.line),
            block: None,
            level: doc.comments[r.comment].level,
            change: None,
        })
        .collect();

    let mut blocks = Vec::with_capacity(doc.units.len());
    for u in &doc.units {
        let mut first_line = u.span.start_line;
        for &c in &u.comments {
            let c = &doc.comments[c];
            if c.placement == Placement::OwnLine && c.span.start_line < first_line {
                first_line = c.span.start_line;
            }
        }
        let all = subtree(u.id);
        let set: HashSet<usize> = all.iter().copied().collect();
        let kids = &children[u.id];
        let fields = kids
            .iter()
            .filter(|&&c| doc.units[c].kind == UnitKind::Field)
            .count();
        let summary = if !kids.is_empty() && fields == kids.len() {
            let word = match u.label.as_str() {
                "interface Events" => "event",
                "interface Jobs" => "job",
                _ => "field",
            };
            plural(fields, word)
        } else {
            plural(kids.len(), "statement")
        };
        let lines = (u.span.start_line..=u.span.end_line)
            .filter(|&l| !idx.line_text(source, l).trim().is_empty())
            .count() as u32;
        let block_risks: Vec<usize> = doc
            .risks
            .iter()
            .enumerate()
            .filter(|(_, r)| r.unit.is_some_and(|x| set.contains(&x)))
            .map(|(i, _)| i)
            .collect();
        let unsure = doc
            .comments
            .iter()
            .filter(|c| c.sigil == Some(Sigil::Unsure) && c.unit.is_some_and(|x| set.contains(&x)))
            .count() as u32;
        blocks.push(Block {
            id: u.id,
            parent: u.parent,
            level: u.level,
            first_row: row_of(first_line),
            head_row: row_of(u.span.start_line),
            last_row: row_of(u.span.end_line),
            label: u.label.clone(),
            kind: u.kind,
            helper: u.parent.is_none()
                && doc
                    .functions
                    .iter()
                    .any(|f| f.unit == u.id && called.contains(f.name.as_str())),
            summary,
            lines,
            badges: badges(doc, &set),
            risks: block_risks,
            unsure,
        });
    }

    // Innermost block of every row (parents first, so children override). Removed rows inside a
    // block's range belong to it too.
    for b in &blocks {
        for row in rows.iter_mut().take(b.last_row + 1).skip(b.first_row) {
            row.block = Some(b.id);
        }
    }
    for r in &mut risks {
        r.block = rows[r.row].block;
    }
    // Helper calls.
    let helper_block: HashMap<&str, usize> = doc
        .functions
        .iter()
        .map(|f| (f.name.as_str(), f.unit))
        .collect();
    for row in &mut rows {
        if row.change == Some(Change::Removed) {
            continue;
        }
        if let Some(line) = row.line {
            row.calls = doc
                .calls
                .iter()
                .filter(|c| c.line == line)
                .filter_map(|c| helper_block.get(c.callee.as_str()).copied())
                .collect();
        }
    }

    let diagnostics = doc
        .diagnostics
        .iter()
        .map(|d| DiagItem {
            severity: d.severity,
            code: d.code,
            message: d.message.clone(),
            line: d.line,
            row: line_to_row.get((d.line.max(1) - 1) as usize).copied(),
        })
        .collect();

    ViewModel {
        model_version: MODEL_VERSION,
        file: file.to_string(),
        base_file: None,
        title: doc.card.as_ref().map(|c| c.title.clone()),
        card: doc.card.clone(),
        rows,
        blocks,
        risks,
        diagnostics,
        changes,
        sides: None,
    }
}

fn plural(n: usize, word: &str) -> String {
    format!("{n} {word}{}", if n == 1 { "" } else { "s" })
}

/// `writes User, Address`, `emits UserRegistered`, ... for the effects of `units`.
fn badges(doc: &Document, units: &HashSet<usize>) -> Vec<String> {
    let mut groups: Vec<(&str, Vec<String>)> = Vec::new();
    for e in doc
        .effects
        .iter()
        .filter(|e| e.unit.is_some_and(|u| units.contains(&u)))
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
        let i = match groups.iter().position(|(v, _)| *v == verb) {
            Some(i) => i,
            None => {
                groups.push((verb, Vec::new()));
                groups.len() - 1
            }
        };
        if let Some(n) = &e.name
            && !groups[i].1.contains(n)
        {
            groups[i].1.push(n.clone());
        }
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
    groups
        .into_iter()
        .map(|(v, names)| {
            if names.is_empty() {
                v.to_string()
            } else {
                format!("{v} {}", names.join(", "))
            }
        })
        .collect()
}

// ─── rows ───────────────────────────────────────────────────────────────────

/// One row per line, except the version line and the card (shown separately) which become blank rows
/// with no content, so that row `i` is always line `i + 1`.
fn file_rows(doc: &Document, source: &str) -> Vec<Row> {
    let idx = LineIndex::new(source);
    let classes = highlight(doc, source);
    let n = idx.line_count();
    let mut rows = Vec::with_capacity(n as usize);

    let mut eol: HashMap<u32, Vec<&Comment>> = HashMap::new();
    let mut own: HashMap<u32, &Comment> = HashMap::new();
    for c in &doc.comments {
        if c.placement == Placement::EndOfLine {
            eol.entry(c.span.start_line).or_default().push(c);
        } else {
            for l in c.span.start_line..=c.span.end_line {
                own.insert(l, c);
            }
        }
    }

    for line in 1..=n {
        let text = idx.line_text(source, line);
        let indent = text.chars().take_while(|c| c.is_whitespace()).count() as u32;
        let level = doc.lines.get((line - 1) as usize).copied().flatten();
        let mut row = Row {
            line: Some(line),
            base_line: None,
            kind: RowKind::Code,
            level,
            block: None,
            indent,
            code: Vec::new(),
            comments: Vec::new(),
            calls: Vec::new(),
            change: None,
        };
        if text.trim().is_empty() {
            row.kind = RowKind::Blank;
            row.level = None;
            rows.push(row);
            continue;
        }
        if let Some(c) = own.get(&line) {
            let lead: String = text.chars().take_while(|c| c.is_whitespace()).collect();
            row.code = vec![Segment {
                text: lead,
                class: None,
            }];
            match c.role {
                CommentRole::Version | CommentRole::Card => {
                    row.kind = RowKind::Blank;
                    row.level = Some(0);
                    row.code.clear();
                }
                CommentRole::Section => {
                    row.kind = RowKind::Section;
                    row.comments.push(RowComment {
                        level: c.level,
                        sigil: None,
                        tag: None,
                        marker: None,
                        text: c.text.clone(),
                        refs: Vec::new(),
                        section: Some(c.text.clone()),
                        plain: false,
                    });
                }
                CommentRole::Note => {
                    row.kind = RowKind::Comment;
                    if line == c.span.start_line {
                        row.comments.push(row_comment(doc, c));
                    } else {
                        // Continuation of a block comment.
                        row.comments.push(RowComment {
                            level: c.level,
                            sigil: None,
                            tag: None,
                            marker: None,
                            text: text.trim().to_string(),
                            refs: Vec::new(),
                            section: None,
                            plain: true,
                        });
                    }
                }
            }
            rows.push(row);
            continue;
        }
        // Code line: highlighted code part, then its end-of-line comments.
        let line_start = idx.line_start(line) as usize;
        let comments = eol.get(&line).cloned().unwrap_or_default();
        let code_end = comments
            .iter()
            .map(|c| c.span.start as usize)
            .min()
            .unwrap_or(line_start + text.len());
        row.code = segments(source, &classes, line_start, code_end.max(line_start));
        trim_trailing_space(&mut row.code);
        row.comments = comments.iter().map(|c| row_comment(doc, c)).collect();
        rows.push(row);
    }
    rows
}

fn row_comment(doc: &Document, c: &Comment) -> RowComment {
    RowComment {
        level: c.level,
        sigil: c.sigil,
        tag: c.tag,
        marker: c.level_marker,
        text: c.text.clone(),
        refs: c
            .refs
            .iter()
            .map(|&r| {
                let r = &doc.refs[r];
                RefChip {
                    label: r
                        .target
                        .as_ref()
                        .map(|t| t.raw.clone())
                        .or_else(|| r.quote.as_ref().map(|q| format!("\"{q}\"")))
                        .unwrap_or_default(),
                    target: r.target.as_ref().map(|t| t.raw.clone()),
                    quote: r.quote.clone(),
                    resolved: r.resolved.as_ref().map(|t| t.raw.clone()),
                    defines_scope: r.defines_scope,
                }
            })
            .collect(),
        section: None,
        plain: c.raw.starts_with("/*"),
    }
}

fn trim_trailing_space(segs: &mut Vec<Segment>) {
    while let Some(last) = segs.last_mut() {
        let trimmed = last.text.trim_end().len();
        if trimmed == 0 {
            segs.pop();
        } else {
            last.text.truncate(trimmed);
            break;
        }
    }
}

const CLASSES: &[&str] = &[
    "",
    "kw",
    "str",
    "num",
    "type",
    "entry",
    "helper",
    "c-access",
    "c-input",
    "c-read",
    "c-write",
    "c-effect",
    "c-output",
    "c-failure",
    "c-consistency",
    "c-config",
];

fn class_id(name: &str) -> u8 {
    CLASSES.iter().position(|c| *c == name).unwrap_or(0) as u8
}

/// A highlight class per byte of `source`.
fn highlight(doc: &Document, source: &str) -> Vec<u8> {
    let mut classes = vec![0u8; source.len()];
    let mut set = |start: u32, end: u32, class: u8| {
        let (s, e) = (start as usize, (end as usize).min(source.len()));
        if s < e {
            classes[s..e].fill(class);
        }
    };
    // tsrs's scanner needs a 'static text. The view model is built once per file, so leaking is fine
    // (the parser keeps its own copy for the same reason).
    let text: &'static str = Box::leak(source.to_string().into_boxed_str());
    let mut spans = doc.literal_spans.clone();
    spans.sort_unstable();
    let mut sc = Scanner::new();
    sc.set_text(text);
    sc.set_skip_trivia(false);
    loop {
        let kind = sc.scan();
        if kind == Kind::EndOfFile {
            break;
        }
        let (start, end) = (sc.token_start(), sc.token_end());
        if let Some(&(a, b)) = spans.iter().find(|(a, b)| start >= *a && start < *b) {
            set(a, b, class_id("str"));
            sc.reset_token_state(b);
            continue;
        }
        let class = if Kind::FIRST_KEYWORD <= kind && kind <= Kind::LAST_KEYWORD {
            "kw"
        } else {
            match kind {
                Kind::StringLiteral | Kind::NoSubstitutionTemplateLiteral => "str",
                Kind::NumericLiteral | Kind::BigIntLiteral => "num",
                Kind::Identifier if text[start as usize..].starts_with(char::is_uppercase) => {
                    "type"
                }
                _ => "",
            }
        };
        if !class.is_empty() {
            set(start, end, class_id(class));
        }
    }
    for e in &doc.effects {
        set(e.start, e.end, class_id(&format!("c-{}", e.category)));
    }
    for c in &doc.calls {
        set(c.start, c.end, class_id("helper"));
    }
    for &pos in &doc.entry_calls {
        set(pos, pos + "entry".len() as u32, class_id("entry"));
    }
    classes
}

/// Runs of equal class in `source[start..end]`.
fn segments(source: &str, classes: &[u8], start: usize, end: usize) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::new();
    let mut i = start;
    while i < end {
        let class = classes[i];
        let mut j = i + 1;
        while j < end && (classes[j] == class || !source.is_char_boundary(j)) {
            j += 1;
        }
        let text = source[i..j].trim_end_matches(['\r', '\n']).to_string();
        if !text.is_empty() {
            let class = (class != 0).then(|| CLASSES[class as usize].to_string());
            out.push(Segment { text, class });
        }
        i = j;
    }
    out
}

// ─── changes (§11.5) ────────────────────────────────────────────────────────

fn changes(base: &Document, head: &Document, rows: &[Row]) -> Changes {
    let mut ch = Changes {
        lines_added: rows
            .iter()
            .filter(|r| r.change == Some(Change::Added) && r.kind != RowKind::Blank)
            .count() as u32,
        lines_removed: rows
            .iter()
            .filter(|r| r.change == Some(Change::Removed) && r.kind != RowKind::Blank)
            .count() as u32,
        ..Changes::default()
    };

    // 1. Card fields.
    let fields = |d: &Document| -> Vec<(String, String)> {
        d.card
            .as_ref()
            .map(|c| {
                c.fields
                    .iter()
                    .map(|f| (f.name.clone(), f.raw.clone()))
                    .collect()
            })
            .unwrap_or_default()
    };
    let (bf, hf) = (fields(base), fields(head));
    for (name, _) in crate::card::FIELDS {
        let b = bf.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone());
        let h = hf.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone());
        if b != h {
            ch.card.push(FieldChange {
                name: name.to_string(),
                base: b,
                head: h,
            });
        }
    }

    // 2. Risks.
    let bk: Vec<String> = base.risks.iter().map(risk_key).collect();
    let hk: Vec<String> = head.risks.iter().map(risk_key).collect();
    ch.risks_added = hk.iter().filter(|k| !bk.contains(k)).cloned().collect();
    ch.risks_removed = bk.iter().filter(|k| !hk.contains(k)).cloned().collect();

    // 3. Entries.
    let describe = |e: &Entry| -> Vec<(String, String)> {
        let mut v = vec![
            (
                "trigger".to_string(),
                e.trigger
                    .as_ref()
                    .map(|t| t.raw.clone())
                    .unwrap_or_default(),
            ),
            ("access".to_string(), e.access.clone().unwrap_or_default()),
            ("input".to_string(), e.input.clone().unwrap_or_default()),
        ];
        v.extend(e.options.iter().map(|o| (o.name.clone(), o.value.clone())));
        if let Some(h) = &e.handle {
            v.push((
                "returns".to_string(),
                h.return_type.clone().unwrap_or_default(),
            ));
        }
        v
    };
    let name_of = |e: &Entry| e.name.clone().unwrap_or_default();
    for h in &head.entries {
        match base.entries.iter().find(|b| name_of(b) == name_of(h)) {
            None => ch.entries.push(NamedChange {
                name: name_of(h),
                change: "added".into(),
                details: Vec::new(),
            }),
            Some(b) => {
                let (bd, hd) = (describe(b), describe(h));
                let mut details = Vec::new();
                for (k, hv) in &hd {
                    match bd.iter().find(|(bk, _)| bk == k) {
                        Some((_, bv)) if bv != hv => details.push(format!("{k}: {bv} → {hv}")),
                        None => details.push(format!("{k}: {hv} (new)")),
                        _ => {}
                    }
                }
                for (k, bv) in &bd {
                    if !hd.iter().any(|(hk, _)| hk == k) {
                        details.push(format!("{k}: {bv} (removed)"));
                    }
                }
                if !details.is_empty() {
                    ch.entries.push(NamedChange {
                        name: name_of(h),
                        change: "changed".into(),
                        details,
                    });
                }
            }
        }
    }
    for b in &base.entries {
        if !head.entries.iter().any(|h| name_of(h) == name_of(b)) {
            ch.entries.push(NamedChange {
                name: name_of(b),
                change: "removed".into(),
                details: Vec::new(),
            });
        }
    }

    // 4. Types.
    let field_names = |d: &Document, t: &TypeDecl| -> Vec<String> {
        t.fields
            .iter()
            .map(|f| d.units[f.unit].label.clone())
            .collect()
    };
    for h in &head.types {
        match base.types.iter().find(|b| b.name == h.name) {
            None => ch.types.push(NamedChange {
                name: h.name.clone(),
                change: "added".into(),
                details: Vec::new(),
            }),
            Some(b) => {
                let (bf, hf) = (field_names(base, b), field_names(head, h));
                let mut details: Vec<String> = hf
                    .iter()
                    .filter(|f| !bf.contains(f))
                    .map(|f| format!("+ {f}"))
                    .collect();
                details.extend(
                    bf.iter()
                        .filter(|f| !hf.contains(f))
                        .map(|f| format!("- {f}")),
                );
                if !details.is_empty() {
                    ch.types.push(NamedChange {
                        name: h.name.clone(),
                        change: "changed".into(),
                        details,
                    });
                }
            }
        }
    }
    for b in &base.types {
        if !head.types.iter().any(|h| h.name == b.name) {
            ch.types.push(NamedChange {
                name: b.name.clone(),
                change: "removed".into(),
                details: Vec::new(),
            });
        }
    }
    ch
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    const EXAMPLE: &str = include_str!("../../../examples/register-user.review.ts");

    #[test]
    fn rows_match_lines() {
        let doc = parse(EXAMPLE);
        let m = build(&doc, EXAMPLE, "register-user.review.ts");
        assert_eq!(m.rows.len(), EXAMPLE.lines().count());
        assert_eq!(m.blocks.len(), doc.units.len());
        // The card and version line render as empty rows; the viewer shows the card separately.
        assert!(m.rows[..14].iter().all(|r| r.kind == RowKind::Blank));
    }

    #[test]
    fn highlighting_and_categories() {
        let doc = parse(EXAMPLE);
        let m = build(&doc, EXAMPLE, "x");
        let classes: HashSet<String> = m
            .rows
            .iter()
            .flat_map(|r| r.code.iter().filter_map(|s| s.class.clone()))
            .collect();
        for c in [
            "kw", "str", "type", "entry", "helper", "c-write", "c-effect", "c-input", "c-output",
        ] {
            assert!(classes.contains(c), "missing class {c}: {classes:?}");
        }
        // The code part is the line without its end-of-line comment.
        let row = m
            .rows
            .iter()
            .find(|r| r.code.iter().any(|s| s.text.contains("createUser")))
            .unwrap();
        let code: String = row.code.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(code, "    const user = await createUser(req.raw)");
        assert_eq!(
            row.comments[0].refs[0].resolved.as_deref(),
            Some("UserController.store")
        );
    }

    #[test]
    fn blocks() {
        let doc = parse(EXAMPLE);
        let m = build(&doc, EXAMPLE, "x");
        let b = |label: &str| m.blocks.iter().find(|b| b.label == label).unwrap();
        assert_eq!(b("type User").summary, "5 fields");
        assert_eq!(b("interface Events").summary, "1 event");
        let helper = b("createUser");
        assert!(helper.helper);
        assert_eq!(
            helper.badges,
            [
                "writes User, Address",
                "transaction",
                "emits UserRegistered"
            ]
        );
        assert_eq!(helper.risks.len(), 1);
        // `// used by:` above the function belongs to its block.
        assert_eq!(helper.first_row + 1, helper.head_row);
    }

    #[test]
    fn diff() {
        let head = EXAMPLE
            .replace(" * access:   public (registerUser)", " * access:   user (registerUser)")
            .replace("  access: \"public\",", "  access: \"user\",")
            .replace(
                "    await db.insert<Address>({ ...data.address, userId: user.id }) // @ \"->address()->create(\"\n",
                "    await db.insert<Address>({ ...data.address, userId: user.id }) // @ \"->address()->create(\"\n    // ! external call inside the transaction\n    await external(\"Stripe\").createCustomer(email) // @ \"Customer::create(\"\n",
            );
        let (b, h) = (parse(EXAMPLE), parse(&head));
        assert!(!h.has_errors(), "{:#?}", h.diagnostics);
        let m = build_diff(&b, EXAMPLE, "base", &h, &head, "head");
        let ch = m.changes.as_ref().unwrap();
        assert_eq!(ch.card.len(), 1);
        assert_eq!(ch.card[0].name, "access");
        assert_eq!(ch.risks_added, ["! external call inside the transaction"]);
        assert_eq!(ch.entries.len(), 1);
        assert_eq!(ch.entries[0].details, ["access: \"public\" → \"user\""]);
        // Card lines count as card changes, not as code lines.
        assert_eq!((ch.lines_added, ch.lines_removed), (3, 1));
        let removed: Vec<_> = m
            .rows
            .iter()
            .filter(|r| r.change == Some(Change::Removed))
            .collect();
        assert_eq!(removed.len(), 2, "card line + access line");
        assert!(
            removed
                .iter()
                .all(|r| r.line.is_none() && r.base_line.is_some())
        );
        let helper = m.blocks.iter().find(|b| b.label == "createUser").unwrap();
        assert!(
            helper
                .badges
                .contains(&"calls Stripe.createCustomer".to_string())
        );
        assert!(m.risks.iter().any(|r| r.change == Some(Change::Added)));
        let sides = m.sides.as_ref().unwrap();
        assert!(sides.base.changes.is_none() && sides.head.changes.is_none());
        assert_eq!(sides.base.rows.len(), EXAMPLE.lines().count());
        assert_eq!(sides.head.rows.len(), head.lines().count());
    }
}
