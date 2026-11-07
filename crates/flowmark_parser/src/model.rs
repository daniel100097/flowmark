//! The parsed document. Everything here is plain data and serializes to JSON
//! (`flowmark parse`), so viewers and other tools never need the TypeScript AST.
//!
//! Lines are 1-based. Offsets are UTF-8 byte offsets into the source text.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Span {
    pub start: u32,
    pub end: u32,
    pub start_line: u32,
    pub end_line: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Document {
    /// `0.1` from the `// flowmark 0.1` line.
    pub version: Option<String>,
    pub card: Option<Card>,
    pub sections: Vec<Section>,
    pub types: Vec<TypeDecl>,
    pub entries: Vec<Entry>,
    pub functions: Vec<FunctionDecl>,
    /// Every statement, object member and type member, in source order. A tree through `parent`.
    pub units: Vec<Unit>,
    pub comments: Vec<Comment>,
    pub refs: Vec<Ref>,
    /// Every `!` and `!?` comment.
    pub risks: Vec<Risk>,
    /// Calls to prelude functions that matter to a reviewer (access, input, reads, writes, effects, ...).
    pub effects: Vec<Effect>,
    /// Calls to functions defined in this file.
    pub calls: Vec<LocalCall>,
    /// Detail level of every line (`lines[0]` is line 1). `None` for blank lines.
    pub lines: Vec<Option<u8>>,
    pub diagnostics: Vec<Diagnostic>,
    /// Template literals and regexes (byte ranges): the scanner cannot tokenize them without the parser.
    #[serde(skip)]
    pub(crate) literal_spans: Vec<(u32, u32)>,
    /// Byte offsets of `entry(` calls.
    #[serde(skip)]
    pub(crate) entry_calls: Vec<u32>,
}

// ─── card ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct Card {
    pub span: Span,
    pub title: String,
    pub summary: Vec<String>,
    pub fields: Vec<CardField>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CardField {
    pub name: String,
    pub line: u32,
    /// The value as written.
    pub raw: String,
    /// The value split on `,` and `→`. Empty for `none`.
    pub items: Vec<CardItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CardItem {
    /// The item without its `( note )` and `!`.
    pub text: String,
    /// `event`, `queue`, `mail`, `notify` or `external` when the item starts with one of them.
    pub kind: Option<String>,
    /// `text` without `kind`.
    pub name: String,
    pub note: Option<String>,
    /// The item ends with `!`.
    pub risk: bool,
}

// ─── sections and declarations ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct Section {
    pub name: String,
    pub line: u32,
    /// From the marker to the line before the next marker (or the end of the file).
    pub end_line: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct TypeDecl {
    pub name: String,
    /// `type` or `interface`
    pub keyword: String,
    pub unit: usize,
    pub fields: Vec<TypeField>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TypeField {
    pub name: String,
    pub unit: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    /// The const the entry is assigned to.
    pub name: Option<String>,
    pub unit: usize,
    pub trigger: Option<Trigger>,
    /// `access` as written, e.g. `"public"` or `{ role: "admin" }`.
    pub access: Option<String>,
    /// The type in `input: shape<T>()`: what the input is validated against before `handle` runs.
    pub input: Option<String>,
    /// Every option except `trigger`, `access` and `handle`, in source order.
    pub options: Vec<EntryOption>,
    pub handle: Option<Handle>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Trigger {
    /// The string as written, without quotes.
    pub raw: String,
    /// `http`, `event`, `job`, `message`, `schedule`, `cli`, `graphql`, `rpc` or `ws`.
    pub kind: String,
    /// What follows the kind: `/api/users` for HTTP, `UserRegistered` for an event, ...
    pub name: String,
    /// HTTP method, or `query`/`mutation`/`subscription` for GraphQL.
    pub method: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EntryOption {
    pub name: String,
    pub value: String,
    pub unit: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Handle {
    pub unit: usize,
    pub params: String,
    pub return_type: Option<String>,
    pub is_async: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct FunctionDecl {
    pub name: String,
    pub unit: usize,
    pub params: String,
    pub return_type: Option<String>,
    pub is_async: bool,
}

// ─── units ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitKind {
    /// A statement in a file, block or function body.
    Statement,
    /// A property or method of an object literal (entry options, `handle`).
    Member,
    /// A member of a type literal or interface.
    Field,
}

/// Something a comment can attach to and a detail level applies to.
#[derive(Debug, Clone, Serialize)]
pub struct Unit {
    pub id: usize,
    pub parent: Option<usize>,
    pub kind: UnitKind,
    /// The TypeScript syntax kind, e.g. `VariableStatement`.
    pub syntax: String,
    /// Short label for messages and placeholders: a declared name or the first line.
    pub label: String,
    pub span: Span,
    pub depth: u32,
    pub section: Option<String>,
    /// Level from the construct (§9.2), before markers and parents.
    pub default_level: u8,
    /// Level from an `// L1`–`// L3` marker.
    pub marker: Option<u8>,
    /// Effective level: max(marker or default, parent's level).
    pub level: u8,
    /// Comments attached to this unit.
    pub comments: Vec<usize>,
    /// The ref that makes this unit a ref scope (§8.3).
    pub scope: Option<usize>,
}

// ─── comments and refs ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Placement {
    /// After code on the same line.
    EndOfLine,
    /// Alone on its line(s).
    OwnLine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommentRole {
    Version,
    Card,
    Section,
    Note,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Sigil {
    #[serde(rename = "!")]
    Risk,
    #[serde(rename = "!?")]
    PossibleRisk,
    #[serde(rename = "?")]
    Unsure,
}

impl Sigil {
    pub fn as_str(self) -> &'static str {
        match self {
            Sigil::Risk => "!",
            Sigil::PossibleRisk => "!?",
            Sigil::Unsure => "?",
        }
    }
    pub fn is_risk(self) -> bool {
        matches!(self, Sigil::Risk | Sigil::PossibleRisk)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Tag {
    #[serde(rename = "tested:")]
    Tested,
    #[serde(rename = "used by:")]
    UsedBy,
}

impl Tag {
    pub fn as_str(self) -> &'static str {
        match self {
            Tag::Tested => "tested:",
            Tag::UsedBy => "used by:",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Comment {
    pub id: usize,
    pub span: Span,
    pub placement: Placement,
    pub role: CommentRole,
    pub raw: String,
    /// `1`–`3` from an `L1`–`L3` marker.
    pub level_marker: Option<u8>,
    pub sigil: Option<Sigil>,
    pub tag: Option<Tag>,
    /// The comment's prose: without `//`, marker, sigil, tag and refs.
    pub text: String,
    pub refs: Vec<usize>,
    /// The unit this comment is attached to (§7.2).
    pub unit: Option<usize>,
    /// The level at which this comment is shown. Risks are also bubbled up by viewers (§9.4).
    pub level: Option<u8>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Target {
    /// As written.
    pub raw: String,
    pub path: Option<String>,
    /// Canonical form: separators `::` and `->` replaced by `.`.
    pub symbol: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Ref {
    pub id: usize,
    pub comment: usize,
    pub line: u32,
    pub raw: String,
    pub target: Option<Target>,
    pub quote: Option<String>,
    /// This ref is on the first line of a function, `handle` or type and makes it a scope.
    pub defines_scope: bool,
    /// For a quote-only ref: the ref whose target it searches in.
    pub scope: Option<usize>,
    /// Where the checker searches: `target`, or the scope's target.
    pub resolved: Option<Target>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Risk {
    pub comment: usize,
    pub kind: Sigil,
    pub text: String,
    pub line: u32,
    pub unit: Option<usize>,
    /// Name of the top-level declaration the risk belongs to (`registerUser`, `type User`, ...).
    pub owner: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Effect {
    pub unit: Option<usize>,
    pub line: u32,
    /// Prelude category: `access`, `input`, `read`, `write`, `effect`, `output`, `failure`, `consistency`, `config`.
    pub category: String,
    /// The prelude call, e.g. `db.insert`, `emit`, `external`.
    pub call: String,
    /// What it acts on: the type argument, event/job/template name, `Service.op`, ...
    pub name: Option<String>,
    /// Byte range of the callee (`db.insert`, `external("Stripe").createCustomer`).
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalCall {
    pub unit: Option<usize>,
    pub line: u32,
    pub callee: String,
    /// Byte range of the callee.
    pub start: u32,
    pub end: u32,
}

// ─── diagnostics ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
    pub line: u32,
}

impl Document {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
}
