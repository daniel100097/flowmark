//! Turns Flowmark source into a [`Document`], using the tsrs TypeScript parser and scanner.

use std::collections::HashSet;

use tsrs_ast::{Kind, Node, SourceFileParseOptions, parseoptions::ExternalModuleIndicatorOptions};
use tsrs_core::{P, get_script_kind_from_file_name, tspath::Path};
use tsrs_scanner::{Scanner, skip_trivia};

use crate::card::parse_card;
use crate::grammar::{parse_comment, parse_target};
use crate::model::*;
use crate::text::LineIndex;

/// Sections in their required order (§4).
pub const SECTIONS: &[&str] = &["data", "flow", "details", "async"];

/// Entry options in their required order (§6.1).
const ENTRY_OPTION_ORDER: &[&str] = &[
    "trigger",
    "access",
    "input",
    "queued",
    "rateLimit",
    "retries",
    "idempotent",
    "unique",
    "overlap",
    "single",
    "guards",
    "timeout",
    "backoff",
    "handle",
];
/// Entry options that default to L2 (§6.1).
const ENTRY_OPTIONS_L2: &[&str] = &["guards", "timeout", "backoff"];

const L1_BUDGET_PER_ENTRY: usize = 15;
const L2_BUDGET_PER_HELPER: usize = 25;

/// Parses a Flowmark file. Never fails: problems are reported in [`Document::diagnostics`].
pub fn parse(source: &str) -> Document {
    let name = "/flowmark.review.ts";
    let file = tsrs_parser::parse_source_file(
        SourceFileParseOptions {
            file_name: name.to_string(),
            path: Path::new(name.to_string()),
            external_module_indicator_options: ExternalModuleIndicatorOptions::default(),
        },
        source,
        get_script_kind_from_file_name(name),
    );
    let mut b = Builder::new(file.text());
    for d in file.diagnostics() {
        let line = d
            .loc()
            .and_then(|l| l.pos().source_offset())
            .map_or(1, |p| b.idx.line(p));
        b.error("E000", line, format!("syntax: {}", d.localize()));
    }
    b.collect_function_names(file.as_node());
    b.walk(file.as_node(), None, 0);
    b.build_declarations(file.as_node());
    b.scan_comments();
    b.sections_and_roles();
    b.attach_comments();
    b.resolve_refs();
    b.compute_levels();
    b.collect_risks();
    b.check_structure();
    b.finish()
}

struct Builder {
    text: &'static str,
    idx: LineIndex,
    units: Vec<Unit>,
    unit_nodes: Vec<P<Node>>,
    /// Units whose `if` ends in `fail(...)` (error paths, §9.2).
    error_paths: HashSet<usize>,
    function_names: HashSet<String>,
    entry_calls: Vec<u32>,
    excluded: Vec<(u32, u32)>,
    doc: Document,
}

impl Builder {
    fn new(text: &'static str) -> Self {
        Builder {
            text,
            idx: LineIndex::new(text),
            units: Vec::new(),
            unit_nodes: Vec::new(),
            error_paths: HashSet::new(),
            function_names: HashSet::new(),
            entry_calls: Vec::new(),
            excluded: Vec::new(),
            doc: Document {
                version: None,
                card: None,
                sections: Vec::new(),
                types: Vec::new(),
                entries: Vec::new(),
                functions: Vec::new(),
                units: Vec::new(),
                comments: Vec::new(),
                refs: Vec::new(),
                risks: Vec::new(),
                effects: Vec::new(),
                calls: Vec::new(),
                lines: Vec::new(),
                diagnostics: Vec::new(),
                literal_spans: Vec::new(),
                entry_calls: Vec::new(),
            },
        }
    }

    fn finish(mut self) -> Document {
        self.doc.units = self.units;
        self.doc.diagnostics.sort_by_key(|d| d.line);
        self.doc.literal_spans = self.excluded;
        self.doc.entry_calls = self.entry_calls;
        self.doc
    }

    fn diag(&mut self, severity: Severity, code: &'static str, line: u32, message: String) {
        self.doc.diagnostics.push(Diagnostic {
            severity,
            code,
            message,
            line,
        });
    }
    fn error(&mut self, code: &'static str, line: u32, message: String) {
        self.diag(Severity::Error, code, line, message);
    }
    fn warn(&mut self, code: &'static str, line: u32, message: String) {
        self.diag(Severity::Warning, code, line, message);
    }

    // ─── node helpers ───────────────────────────────────────────────────────

    fn start(&self, n: P<Node>) -> u32 {
        skip_trivia(self.text, n.pos().source_offset().unwrap_or(0))
    }
    fn end(&self, n: P<Node>) -> u32 {
        n.end().source_offset().unwrap_or(0)
    }
    fn src(&self, n: P<Node>) -> &'static str {
        let (s, e) = (self.start(n) as usize, self.end(n) as usize);
        &self.text[s.min(e)..e]
    }
    fn line_of(&self, n: P<Node>) -> u32 {
        self.idx.line(self.start(n))
    }
    fn name_text(&self, n: Option<P<Node>>) -> Option<String> {
        let n = n?;
        Some(match n.kind() {
            Kind::Identifier
            | Kind::PrivateIdentifier
            | Kind::StringLiteral
            | Kind::NumericLiteral
            | Kind::NoSubstitutionTemplateLiteral => n.text().to_string(),
            _ => self.src(n).to_string(),
        })
    }
    fn string_literal(&self, n: P<Node>) -> Option<String> {
        matches!(
            n.kind(),
            Kind::StringLiteral | Kind::NoSubstitutionTemplateLiteral
        )
        .then(|| n.text().to_string())
    }
    /// `initializer()` panics on kinds without one.
    fn init(n: P<Node>) -> Option<P<Node>> {
        match n.kind() {
            Kind::PropertyAssignment
            | Kind::VariableDeclaration
            | Kind::Parameter
            | Kind::PropertyDeclaration
            | Kind::PropertySignature
            | Kind::BindingElement
            | Kind::EnumMember => n.initializer(),
            _ => None,
        }
    }
    /// `name()` panics on kinds without one.
    fn name_of(n: P<Node>) -> Option<P<Node>> {
        match n.kind() {
            Kind::PropertyAssignment
            | Kind::ShorthandPropertyAssignment
            | Kind::MethodDeclaration
            | Kind::PropertySignature
            | Kind::MethodSignature
            | Kind::FunctionDeclaration
            | Kind::TypeAliasDeclaration
            | Kind::InterfaceDeclaration
            | Kind::VariableDeclaration
            | Kind::Parameter
            | Kind::GetAccessor
            | Kind::SetAccessor
            | Kind::ClassDeclaration
            | Kind::EnumDeclaration => n.name(),
            _ => None,
        }
    }
    fn unit_of(&self, n: P<Node>) -> Option<usize> {
        self.unit_nodes.iter().position(|u| *u == n)
    }
    /// `a.b.c` for identifiers and property accesses.
    fn callee_path(&self, n: P<Node>) -> Option<String> {
        match n.kind() {
            Kind::Identifier => Some(n.text().to_string()),
            Kind::PropertyAccessExpression => {
                let pa = n.as_property_access_expression();
                Some(format!(
                    "{}.{}",
                    self.callee_path(pa.expression)?,
                    pa.name.text()
                ))
            }
            _ => None,
        }
    }
    fn params_text(&self, n: P<Node>) -> String {
        match (n.parameters().first(), n.parameters().last()) {
            (Some(&f), Some(&l)) => {
                self.text[self.start(f) as usize..self.end(l) as usize].to_string()
            }
            _ => String::new(),
        }
    }
    fn is_async(&self, n: P<Node>) -> bool {
        self.src(n).starts_with("async")
            || self.src(n).starts_with("export async")
            || Self::init(n).is_some_and(|i| self.src(i).starts_with("async"))
    }

    // ─── walk ───────────────────────────────────────────────────────────────

    fn collect_function_names(&mut self, file: P<Node>) {
        for &s in file.statements() {
            if s.kind() == Kind::FunctionDeclaration
                && let Some(name) = self.name_text(Self::name_of(s))
            {
                self.function_names.insert(name);
            }
        }
    }

    fn walk(&mut self, node: P<Node>, cur: Option<usize>, depth: u32) {
        self.visit_node(node, cur);
        for child in node.iter_children() {
            match self.child_unit_kind(node, child) {
                Some(kind) => {
                    let id = self.add_unit(child, kind, cur, depth);
                    self.walk(child, Some(id), depth + 1);
                }
                None => self.walk(child, cur, depth),
            }
        }
    }

    fn child_unit_kind(&self, parent: P<Node>, child: P<Node>) -> Option<UnitKind> {
        match parent.kind() {
            Kind::SourceFile
            | Kind::Block
            | Kind::ModuleBlock
            | Kind::CaseClause
            | Kind::DefaultClause => parent
                .statements()
                .contains(&child)
                .then_some(UnitKind::Statement),
            Kind::IfStatement => {
                let s = parent.as_if_statement();
                let branch = child == s.then_statement || Some(child) == s.else_statement;
                (branch && child.kind() != Kind::Block).then_some(UnitKind::Statement)
            }
            Kind::ForStatement
            | Kind::ForInStatement
            | Kind::ForOfStatement
            | Kind::WhileStatement
            | Kind::DoStatement => (child == parent.statement() && child.kind() != Kind::Block)
                .then_some(UnitKind::Statement),
            Kind::ObjectLiteralExpression => Some(UnitKind::Member),
            Kind::TypeLiteral | Kind::InterfaceDeclaration => matches!(
                child.kind(),
                Kind::PropertySignature
                    | Kind::MethodSignature
                    | Kind::IndexSignature
                    | Kind::CallSignature
                    | Kind::ConstructSignature
            )
            .then_some(UnitKind::Field),
            _ => None,
        }
    }

    fn add_unit(&mut self, n: P<Node>, kind: UnitKind, parent: Option<usize>, depth: u32) -> usize {
        let id = self.units.len();
        let span = self.idx.span(self.start(n), self.end(n));
        let label = self.label(n);
        if n.kind() == Kind::IfStatement && self.ends_in_fail(n.as_if_statement().then_statement) {
            self.error_paths.insert(id);
        }
        self.units.push(Unit {
            id,
            parent,
            kind,
            syntax: format!("{:?}", n.kind()),
            label,
            span,
            depth,
            section: None,
            default_level: 0,
            marker: None,
            level: 0,
            comments: Vec::new(),
            scope: None,
        });
        self.unit_nodes.push(n);
        id
    }

    fn label(&self, n: P<Node>) -> String {
        let named = match n.kind() {
            Kind::FunctionDeclaration => self.name_text(Self::name_of(n)),
            Kind::TypeAliasDeclaration => self
                .name_text(Self::name_of(n))
                .map(|s| format!("type {s}")),
            Kind::InterfaceDeclaration => self
                .name_text(Self::name_of(n))
                .map(|s| format!("interface {s}")),
            Kind::PropertyAssignment
            | Kind::ShorthandPropertyAssignment
            | Kind::MethodDeclaration
            | Kind::PropertySignature
            | Kind::MethodSignature => self.name_text(Self::name_of(n)),
            Kind::VariableStatement => {
                let list = n.as_variable_statement().declaration_list;
                let names: Vec<String> = list
                    .as_variable_declaration_list()
                    .declarations
                    .nodes()
                    .iter()
                    .filter_map(|d| self.name_text(Self::name_of(*d)))
                    .collect();
                (!names.is_empty()).then(|| names.join(", "))
            }
            _ => None,
        };
        named.unwrap_or_else(|| {
            let first = self.src(n).lines().next().unwrap_or("");
            let first = first.split(" //").next().unwrap_or(first).trim();
            if first.chars().count() > 60 {
                format!("{}…", first.chars().take(59).collect::<String>())
            } else {
                first.to_string()
            }
        })
    }

    fn ends_in_fail(&self, then: P<Node>) -> bool {
        let last = if then.kind() == Kind::Block {
            then.statements().last().copied()
        } else {
            Some(then)
        };
        let Some(last) = last else { return false };
        let expr = match last.kind() {
            Kind::ExpressionStatement | Kind::ReturnStatement => last.expression(),
            _ => None,
        };
        expr.is_some_and(|e| {
            e.kind() == Kind::CallExpression
                && self
                    .callee_path(e.as_call_expression().expression)
                    .as_deref()
                    == Some("fail")
        })
    }

    /// Per-node bookkeeping during the walk: prelude calls, local calls, spans to skip when scanning comments.
    fn visit_node(&mut self, n: P<Node>, cur: Option<usize>) {
        match n.kind() {
            Kind::RegularExpressionLiteral
            | Kind::NoSubstitutionTemplateLiteral
            | Kind::TemplateExpression => {
                self.excluded.push((self.start(n), self.end(n)));
            }
            Kind::CallExpression => self.visit_call(n, cur),
            _ => {}
        }
    }

    fn visit_call(&mut self, n: P<Node>, cur: Option<usize>) {
        let call = n.as_call_expression();
        let line = self.line_of(n);
        let type_arg = n.type_arguments().first().map(|&t| self.src(t).to_string());
        let str_arg = |i: usize| n.arguments().get(i).and_then(|&a| self.string_literal(a));
        let src_arg = |i: usize| n.arguments().get(i).map(|&a| self.src(a).to_string());

        // external("Stripe").createCustomer(...)
        if call.expression.kind() == Kind::PropertyAccessExpression {
            let pa = call.expression.as_property_access_expression();
            if pa.expression.kind() == Kind::CallExpression {
                let inner = pa.expression;
                if self
                    .callee_path(inner.as_call_expression().expression)
                    .as_deref()
                    == Some("external")
                {
                    let service = inner
                        .arguments()
                        .first()
                        .and_then(|&a| self.string_literal(a));
                    let name = service.map(|s| format!("{s}.{}", pa.name.text()));
                    self.effect(call.expression, cur, "effect", "external", name);
                    return;
                }
            }
        }

        let Some(path) = self.callee_path(call.expression) else {
            return;
        };
        let (category, name) = match path.as_str() {
            "entry" => {
                self.entry_calls.push(self.start(n));
                return;
            }
            "validate" | "shape" => ("input", type_arg),
            "authorize" => ("access", str_arg(0)),
            "fail" | "assert" => ("failure", src_arg(if path == "fail" { 0 } else { 1 })),
            "db.find" | "db.findOrFail" | "db.list" | "db.count" => ("read", type_arg),
            "db.query" => ("read", None),
            "db.insert" | "db.update" | "db.upsert" | "db.delete" => ("write", type_arg),
            "db.transaction" | "db.afterCommit" | "lock" => ("consistency", None),
            "cache.get" | "cache.remember" => ("read", Some("cache".into())),
            "cache.set" | "cache.forget" => ("write", Some("cache".into())),
            "files.read" => ("read", Some("files".into())),
            "files.write" | "files.delete" => ("write", Some("files".into())),
            "files.url" => ("output", Some("files.url".into())),
            "emit" | "queue" => ("effect", str_arg(0)),
            "mail" => ("effect", str_arg(1)),
            "notify" => ("effect", str_arg(1)),
            "log" => ("effect", None),
            "respond" => ("output", src_arg(0)),
            "redirect" => ("output", None),
            "config" | "flag" => ("config", str_arg(0)),
            p if self.function_names.contains(p) => {
                let (start, end) = (self.start(call.expression), self.end(call.expression));
                self.doc.calls.push(LocalCall {
                    unit: cur,
                    line,
                    callee: p.to_string(),
                    start,
                    end,
                });
                return;
            }
            _ => return,
        };
        self.effect(call.expression, cur, category, &path, name);
    }

    fn effect(
        &mut self,
        callee: P<Node>,
        unit: Option<usize>,
        category: &str,
        call: &str,
        name: Option<String>,
    ) {
        let (start, end) = (self.start(callee), self.end(callee));
        let line = self.idx.line(start);
        self.doc.effects.push(Effect {
            unit,
            line,
            category: category.into(),
            call: call.into(),
            name,
            start,
            end,
        });
    }

    // ─── declarations ───────────────────────────────────────────────────────

    fn build_declarations(&mut self, file: P<Node>) {
        let mut entry_calls_in_consts = HashSet::new();
        for &s in file.statements() {
            let Some(unit) = self.unit_of(s) else {
                continue;
            };
            let line = self.line_of(s);
            if self.src(s).starts_with("export ") {
                self.error("E080", line, "review files have no exports".into());
            }
            match s.kind() {
                Kind::ImportDeclaration | Kind::ImportEqualsDeclaration => {
                    self.error("E080", line, "review files have no imports".into());
                }
                Kind::ExportDeclaration | Kind::ExportAssignment => {
                    self.error("E080", line, "review files have no exports".into());
                }
                Kind::FunctionDeclaration => {
                    let name = self.name_text(Self::name_of(s)).unwrap_or_default();
                    let return_type = s.type_node().map(|t| self.src(t).to_string());
                    let decl = FunctionDecl {
                        name,
                        unit,
                        params: self.params_text(s),
                        return_type,
                        is_async: self.is_async(s),
                    };
                    self.doc.functions.push(decl);
                }
                Kind::TypeAliasDeclaration | Kind::InterfaceDeclaration => {
                    let keyword = if s.kind() == Kind::TypeAliasDeclaration {
                        "type"
                    } else {
                        "interface"
                    };
                    let fields = self
                        .units
                        .iter()
                        .filter(|u| u.parent == Some(unit) && u.kind == UnitKind::Field)
                        .map(|u| TypeField {
                            name: u.label.clone(),
                            unit: u.id,
                        })
                        .collect();
                    let name = self.name_text(Self::name_of(s)).unwrap_or_default();
                    self.doc.types.push(TypeDecl {
                        name,
                        keyword: keyword.into(),
                        unit,
                        fields,
                    });
                }
                Kind::VariableStatement => {
                    let list = s.as_variable_statement().declaration_list;
                    for &decl in list.as_variable_declaration_list().declarations.nodes() {
                        let Some(init) = Self::init(decl) else {
                            continue;
                        };
                        if init.kind() == Kind::CallExpression
                            && self
                                .callee_path(init.as_call_expression().expression)
                                .as_deref()
                                == Some("entry")
                        {
                            entry_calls_in_consts.insert(self.start(init));
                            let name = self.name_text(Self::name_of(decl));
                            self.build_entry(name, unit, init);
                        }
                    }
                }
                _ => {}
            }
        }
        for pos in self.entry_calls.clone() {
            if !entry_calls_in_consts.contains(&pos) {
                let line = self.idx.line(pos);
                self.error(
                    "E020",
                    line,
                    "an entry must be a top-level `const <name> = entry({ ... })`".into(),
                );
            }
        }
    }

    fn build_entry(&mut self, name: Option<String>, unit: usize, call: P<Node>) {
        let line = self.line_of(call);
        let label = name.clone().unwrap_or_else(|| "entry".into());
        let mut entry = Entry {
            name,
            unit,
            trigger: None,
            access: None,
            input: None,
            options: Vec::new(),
            handle: None,
        };
        let mut handle_fn: Option<P<Node>> = None;
        let mut input_line = line;
        let Some(&obj) = call
            .arguments()
            .first()
            .filter(|a| a.kind() == Kind::ObjectLiteralExpression)
        else {
            self.error(
                "E021",
                line,
                format!("`{label}`: entry() takes one object literal"),
            );
            self.doc.entries.push(entry);
            return;
        };

        let mut lines_seen: Vec<(u32, String)> = Vec::new();
        let mut order: Vec<(String, u32)> = Vec::new();
        for &prop in obj.properties() {
            let Some(prop_unit) = self.unit_of(prop) else {
                continue;
            };
            let pline = self.line_of(prop);
            let pname = self.name_text(Self::name_of(prop)).unwrap_or_default();
            if let Some((_, other)) = lines_seen.iter().find(|(l, _)| *l == pline) {
                self.error(
                    "E023",
                    pline,
                    format!("`{label}`: options `{other}` and `{pname}` share a line"),
                );
            }
            lines_seen.push((pline, pname.clone()));
            order.push((pname.clone(), pline));
            match pname.as_str() {
                "trigger" => {
                    let raw = Self::init(prop).and_then(|i| self.string_literal(i));
                    match raw {
                        Some(raw) => match parse_trigger(&raw) {
                            Ok(t) => entry.trigger = Some(t),
                            Err(m) => self.error("E022", pline, format!("`{label}`: {m}")),
                        },
                        None => self.error(
                            "E022",
                            pline,
                            format!("`{label}`: trigger must be a string"),
                        ),
                    }
                }
                "access" => entry.access = Self::init(prop).map(|i| self.src(i).to_string()),
                "input" => {
                    input_line = pline;
                    // input: shape<T>()
                    let shape = Self::init(prop).filter(|i| {
                        i.kind() == Kind::CallExpression
                            && self
                                .callee_path(i.as_call_expression().expression)
                                .as_deref()
                                == Some("shape")
                    });
                    match shape.and_then(|c| c.type_arguments().first().copied()) {
                        Some(t) => entry.input = Some(self.src(t).to_string()),
                        None => self.error(
                            "E025",
                            pline,
                            format!("`{label}`: input must be `shape<T>()`"),
                        ),
                    }
                }
                "handle" => {
                    let func = match prop.kind() {
                        Kind::MethodDeclaration => Some(prop),
                        _ => Self::init(prop).filter(|i| {
                            matches!(i.kind(), Kind::ArrowFunction | Kind::FunctionExpression)
                        }),
                    };
                    handle_fn = func;
                    match func {
                        Some(f) => {
                            entry.handle = Some(Handle {
                                unit: prop_unit,
                                params: self.params_text(f),
                                return_type: f.type_node().map(|t| self.src(t).to_string()),
                                is_async: self.is_async(prop),
                            })
                        }
                        None => self.error(
                            "E021",
                            pline,
                            format!("`{label}`: handle must be a function"),
                        ),
                    }
                }
                _ => {
                    let value = Self::init(prop)
                        .map(|i| self.src(i).to_string())
                        .unwrap_or_default();
                    entry.options.push(EntryOption {
                        name: pname,
                        value,
                        unit: prop_unit,
                    });
                }
            }
        }
        for required in ["trigger", "access", "handle"] {
            if !order.iter().any(|(n, _)| n == required) {
                self.error(
                    "E021",
                    line,
                    format!("`{label}`: entry is missing `{required}`"),
                );
            }
        }
        // `input` only where something outside sends data (§6.1).
        if entry.input.is_some()
            && let Some(t) = &entry.trigger
            && matches!(t.kind.as_str(), "event" | "job" | "schedule")
        {
            self.error(
                "E026",
                input_line,
                format!(
                    "`{label}`: {} entries take no `input`; their payload is already typed",
                    t.kind
                ),
            );
        }
        // Declared input, but the handler reads the unvalidated request (§12.3).
        if entry.input.is_some()
            && let Some(f) = handle_fn
            && let Some(param) = f
                .parameters()
                .first()
                .and_then(|p| Self::name_of(*p))
                .filter(|n| n.kind() == Kind::Identifier)
                .map(|n| n.text().to_string())
        {
            let mut uses = Vec::new();
            self.find_raw_uses(f, &param, &mut uses);
            for l in uses {
                self.warn(
                    "W060",
                    l,
                    format!(
                        "`{label}` declares `input`, but `handle` reads `{param}.raw`, which is not validated"
                    ),
                );
            }
        }
        // Option order (§6.1).
        let rank = |n: &str| ENTRY_OPTION_ORDER.iter().position(|o| *o == n);
        let mut last = 0;
        for (n, l) in &order {
            match rank(n) {
                Some(r) if r < last => self.warn(
                    "W024",
                    *l,
                    format!("`{label}`: option `{n}` is out of order (§6.1)"),
                ),
                Some(r) => last = r,
                None => self.warn("W025", *l, format!("`{label}`: unknown option `{n}`")),
            }
        }
        self.doc.entries.push(entry);
    }

    /// Lines where `<param>.raw` is read inside `node`.
    fn find_raw_uses(&self, node: P<Node>, param: &str, out: &mut Vec<u32>) {
        if node.kind() == Kind::PropertyAccessExpression {
            let pa = node.as_property_access_expression();
            if pa.expression.kind() == Kind::Identifier
                && pa.expression.text() == param
                && pa.name.text() == "raw"
            {
                let line = self.line_of(node);
                if !out.contains(&line) {
                    out.push(line);
                }
            }
        }
        for child in node.iter_children() {
            self.find_raw_uses(child, param, out);
        }
    }

    // ─── comments ───────────────────────────────────────────────────────────

    fn scan_comments(&mut self) {
        self.excluded.sort_unstable();
        let mut sc = Scanner::new();
        sc.set_text(self.text);
        sc.set_skip_trivia(false);
        let mut found = Vec::new();
        loop {
            let kind = sc.scan();
            if kind == Kind::EndOfFile {
                break;
            }
            let start = sc.token_start();
            // Template literals and regexes need the parser's context to scan; skip over them.
            if let Some(&(_, end)) = self
                .excluded
                .iter()
                .find(|(a, b)| start >= *a && start < *b)
            {
                sc.reset_token_state(end);
                continue;
            }
            if matches!(
                kind,
                Kind::SingleLineCommentTrivia | Kind::MultiLineCommentTrivia
            ) {
                found.push((start, sc.token_end()));
            }
        }
        for (start, end) in found {
            let span = self.idx.span(start, end);
            let line_start = self.idx.line_start(span.start_line) as usize;
            let own_line = self.text[line_start..start as usize].trim().is_empty();
            let raw = self.text[start as usize..end as usize].to_string();
            let id = self.doc.comments.len();
            self.doc.comments.push(Comment {
                id,
                span,
                placement: if own_line {
                    Placement::OwnLine
                } else {
                    Placement::EndOfLine
                },
                role: CommentRole::Note,
                raw,
                level_marker: None,
                sigil: None,
                tag: None,
                text: String::new(),
                refs: Vec::new(),
                unit: None,
                level: None,
            });
        }
    }

    fn sections_and_roles(&mut self) {
        let first_unit_start = self
            .units
            .iter()
            .find(|u| u.parent.is_none())
            .map_or(u32::MAX, |u| u.span.start);
        let mut card_seen = false;
        for i in 0..self.doc.comments.len() {
            let c = &self.doc.comments[i];
            let (raw, span, placement) = (c.raw.clone(), c.span, c.placement);
            if i == 0
                && span.start == 0
                && let Some(v) = raw.strip_prefix("// flowmark ")
            {
                self.doc.version = Some(v.trim().to_string());
                self.doc.comments[i].role = CommentRole::Version;
                self.doc.comments[i].text = raw.clone();
                continue;
            }
            if !card_seen && raw.starts_with("/**") && span.start < first_unit_start {
                card_seen = true;
                let (card, errors) = parse_card(&raw, span);
                for (line, m) in errors {
                    self.error("E003", line, m);
                }
                self.doc.card = Some(card);
                self.doc.comments[i].role = CommentRole::Card;
                continue;
            }
            if placement == Placement::OwnLine
                && let Some(body) = raw.strip_prefix("//")
                && let Some(name) = section_name(body)
            {
                self.doc.comments[i].role = CommentRole::Section;
                self.doc.comments[i].text = name.clone();
                self.doc.sections.push(Section {
                    name,
                    line: span.start_line,
                    end_line: 0,
                });
                continue;
            }
            self.parse_note(i);
        }

        // Section ranges, names and order.
        let last_line = self.idx.line_count();
        let n = self.doc.sections.len();
        for s in 0..n {
            let end = if s + 1 < n {
                self.doc.sections[s + 1].line - 1
            } else {
                last_line
            };
            self.doc.sections[s].end_line = end;
        }
        let mut last_rank = None;
        let mut seen = HashSet::new();
        for s in self.doc.sections.clone() {
            match SECTIONS.iter().position(|x| *x == s.name) {
                None => self.error(
                    "E010",
                    s.line,
                    format!(
                        "unknown section `{}` (sections: {})",
                        s.name,
                        SECTIONS.join(", ")
                    ),
                ),
                Some(r) => {
                    if !seen.insert(s.name.clone()) {
                        self.error(
                            "E012",
                            s.line,
                            format!("section `{}` appears twice", s.name),
                        );
                    } else if last_rank.is_some_and(|l| r < l) {
                        self.error(
                            "E011",
                            s.line,
                            format!("section `{}` is out of order (§4)", s.name),
                        );
                    }
                    last_rank = Some(r);
                }
            }
        }
        for u in &mut self.units {
            u.section = self
                .doc
                .sections
                .iter()
                .find(|s| u.span.start_line >= s.line && u.span.start_line <= s.end_line)
                .map(|s| s.name.clone());
        }
    }

    fn parse_note(&mut self, i: usize) {
        let raw = self.doc.comments[i].raw.clone();
        let line = self.doc.comments[i].span.start_line;
        let parsed = if let Some(body) = raw.strip_prefix("//") {
            parse_comment(body)
        } else {
            // Block comments carry prose (and refs), but no marker, sigil or tag.
            let body = raw
                .trim_start_matches("/*")
                .trim_start_matches('*')
                .trim_end_matches("*/");
            let body: Vec<&str> = body
                .lines()
                .map(|l| l.trim().trim_start_matches('*').trim())
                .collect();
            let mut p = parse_comment(&body.join(" "));
            p.level = None;
            p.sigil = None;
            p.tag = None;
            p
        };
        for e in &parsed.errors {
            self.error("E030", line, e.clone());
        }
        let mut ref_ids = Vec::new();
        for r in parsed.refs {
            let id = self.doc.refs.len();
            self.doc.refs.push(Ref {
                id,
                comment: i,
                line,
                raw: r.raw,
                target: r.target.as_deref().map(parse_target),
                quote: r.quote,
                defines_scope: false,
                scope: None,
                resolved: None,
            });
            ref_ids.push(id);
        }
        let c = &mut self.doc.comments[i];
        c.level_marker = parsed.level;
        c.sigil = parsed.sigil;
        c.tag = parsed.tag;
        c.text = parsed.text;
        c.refs = ref_ids;
    }

    // ─── attachment (§7.2) ──────────────────────────────────────────────────

    fn first_unit_starting(&self, line: u32) -> Option<usize> {
        self.units
            .iter()
            .find(|u| u.span.start_line == line)
            .map(|u| u.id)
    }
    fn innermost_ending(&self, line: u32) -> Option<usize> {
        self.units
            .iter()
            .filter(|u| u.span.end_line == line)
            .max_by_key(|u| u.depth)
            .map(|u| u.id)
    }
    fn innermost_covering(&self, line: u32) -> Option<usize> {
        self.units
            .iter()
            .filter(|u| u.span.start_line <= line && line <= u.span.end_line)
            .max_by_key(|u| u.depth)
            .map(|u| u.id)
    }

    fn attach_comments(&mut self) {
        let notes: Vec<usize> = self
            .doc
            .comments
            .iter()
            .filter(|c| c.role == CommentRole::Note)
            .map(|c| c.id)
            .collect();
        let mut i = 0;
        while i < notes.len() {
            let c = &self.doc.comments[notes[i]];
            if c.placement == Placement::EndOfLine {
                let line = c.span.start_line;
                let unit = self
                    .first_unit_starting(line)
                    .or_else(|| self.innermost_ending(line))
                    .or_else(|| self.innermost_covering(line));
                self.set_unit(notes[i], unit);
                i += 1;
                continue;
            }
            // A group of own-line comments on consecutive lines.
            let mut j = i;
            while j + 1 < notes.len() {
                let (a, b) = (
                    &self.doc.comments[notes[j]],
                    &self.doc.comments[notes[j + 1]],
                );
                if b.placement == Placement::OwnLine && b.span.start_line == a.span.end_line + 1 {
                    j += 1;
                } else {
                    break;
                }
            }
            let first_line = self.doc.comments[notes[i]].span.start_line;
            let next_line = self.doc.comments[notes[j]].span.end_line + 1;
            let next_is_blank = next_line > self.idx.line_count()
                || self.idx.line_text(self.text, next_line).trim().is_empty();
            let next_has_comment = self
                .doc
                .comments
                .iter()
                .any(|c| c.span.start_line == next_line && c.placement == Placement::OwnLine);
            let unit = if next_is_blank || next_has_comment {
                None
            } else {
                self.first_unit_starting(next_line)
            };
            let unit = match unit {
                Some(u) => Some(u),
                None => {
                    let enclosing = self.innermost_covering(first_line);
                    if next_is_blank || enclosing.is_none() {
                        self.warn(
                            "W040",
                            first_line,
                            "comment is not directly above a statement (§7.2); it attaches to the enclosing block"
                                .into(),
                        );
                    }
                    enclosing
                }
            };
            for &n in &notes[i..=j] {
                self.set_unit(n, unit);
            }
            i = j + 1;
        }
    }

    fn set_unit(&mut self, comment: usize, unit: Option<usize>) {
        self.doc.comments[comment].unit = unit;
        if let Some(u) = unit {
            self.units[u].comments.push(comment);
        }
    }

    // ─── refs (§8.3) ────────────────────────────────────────────────────────

    fn resolve_refs(&mut self) {
        // Scopes: a ref with a target, in an end-of-line comment on the first line of a function,
        // a method / function-valued property, or a type.
        for u in 0..self.units.len() {
            let node = self.unit_nodes[u];
            let scope_capable = match node.kind() {
                Kind::FunctionDeclaration
                | Kind::MethodDeclaration
                | Kind::TypeAliasDeclaration
                | Kind::InterfaceDeclaration => true,
                Kind::PropertyAssignment => Self::init(node).is_some_and(|i| {
                    matches!(i.kind(), Kind::ArrowFunction | Kind::FunctionExpression)
                }),
                _ => false,
            };
            if !scope_capable {
                continue;
            }
            let start_line = self.units[u].span.start_line;
            let scope_ref = self.units[u].comments.iter().find_map(|&c| {
                let c = &self.doc.comments[c];
                if c.placement != Placement::EndOfLine
                    || c.span.start_line != start_line
                    || c.tag.is_some()
                {
                    return None;
                }
                c.refs
                    .iter()
                    .copied()
                    .find(|&r| self.doc.refs[r].target.is_some())
            });
            if let Some(r) = scope_ref {
                self.units[u].scope = Some(r);
                self.doc.refs[r].defines_scope = true;
            }
        }
        // Quote-only refs search in the nearest scope.
        for r in 0..self.doc.refs.len() {
            if self.doc.refs[r].target.is_some() {
                self.doc.refs[r].resolved = self.doc.refs[r].target.clone();
                continue;
            }
            if self.doc.refs[r].quote.is_none() {
                continue;
            }
            let comment = self.doc.refs[r].comment;
            let mut unit = self.doc.comments[comment].unit;
            let mut scope = None;
            while let Some(u) = unit {
                if let Some(s) = self.units[u].scope.filter(|&s| s != r) {
                    scope = Some(s);
                    break;
                }
                unit = self.units[u].parent;
            }
            match scope {
                Some(s) => {
                    self.doc.refs[r].scope = Some(s);
                    self.doc.refs[r].resolved = self.doc.refs[s].target.clone();
                }
                None => {
                    let line = self.doc.refs[r].line;
                    let raw = self.doc.refs[r].raw.clone();
                    self.error(
                        "E031",
                        line,
                        format!("quote-only ref `{raw}` has no enclosing ref scope (§8.3)"),
                    );
                }
            }
        }
    }

    // ─── levels (§9) ────────────────────────────────────────────────────────

    fn compute_levels(&mut self) {
        let entry_units: HashSet<usize> = self.doc.entries.iter().map(|e| e.unit).collect();
        let event_like_types: HashSet<usize> = self
            .doc
            .types
            .iter()
            .filter(|t| t.keyword == "interface" && (t.name == "Events" || t.name == "Jobs"))
            .map(|t| t.unit)
            .collect();
        let top_types: HashSet<usize> = self.doc.types.iter().map(|t| t.unit).collect();

        for u in 0..self.units.len() {
            // Marker from attached comments.
            let markers: Vec<(u8, u32)> = self.units[u]
                .comments
                .iter()
                .filter_map(|&c| {
                    let c = &self.doc.comments[c];
                    c.level_marker.map(|m| (m, c.span.start_line))
                })
                .collect();
            if markers.len() > 1 {
                self.warn(
                    "W052",
                    markers[1].1,
                    "more than one level marker on the same statement".into(),
                );
            }
            let marker = markers.first().map(|m| m.0);
            let marker_line = markers.first().map_or(0, |m| m.1);

            let unit = &self.units[u];
            let parent = unit.parent;
            let parent_level = parent.map_or(0, |p| self.units[p].level);
            let node = self.unit_nodes[u];
            let default = match parent {
                None => {
                    if node.kind() == Kind::FunctionDeclaration {
                        2
                    } else {
                        1
                    }
                }
                Some(p) => match unit.kind {
                    UnitKind::Field if event_like_types.contains(&p) => 2,
                    UnitKind::Field if top_types.contains(&p) => {
                        let ps = self.units[p].span;
                        if ps.start_line != ps.end_line { 2 } else { 0 }
                    }
                    UnitKind::Member
                        if entry_units.contains(&self.entry_ancestor(u).unwrap_or(usize::MAX)) =>
                    {
                        let is_entry_option = self.is_entry_option(u);
                        if is_entry_option && ENTRY_OPTIONS_L2.contains(&unit.label.as_str()) {
                            2
                        } else {
                            0
                        }
                    }
                    UnitKind::Statement if self.error_paths.contains(&u) => 2,
                    _ => 0,
                },
            };

            let mut marker = marker;
            if let Some(m) = marker {
                let label = self.units[u].label.clone();
                if self.is_entry_option(u)
                    && matches!(label.as_str(), "trigger" | "access" | "input")
                    && m > 1
                {
                    self.error(
                        "E024",
                        marker_line,
                        format!("`{label}` is always L1 and cannot be lowered"),
                    );
                    marker = None;
                } else if m < parent_level {
                    self.warn(
                        "W050",
                        marker_line,
                        format!(
                            "level marker L{m} has no effect inside an L{parent_level} block (§9.3)"
                        ),
                    );
                }
            }
            let base = marker.unwrap_or(default);
            let unit = &mut self.units[u];
            unit.default_level = default;
            unit.marker = marker;
            unit.level = base.max(parent_level);
        }

        // Comment levels.
        let section_levels: Vec<Option<u8>> = self
            .doc
            .sections
            .iter()
            .map(|s| {
                self.units
                    .iter()
                    .filter(|u| u.parent.is_none() && u.section.as_deref() == Some(s.name.as_str()))
                    .map(|u| u.level)
                    .min()
            })
            .collect();
        for c in 0..self.doc.comments.len() {
            let comment = &self.doc.comments[c];
            let level = match comment.role {
                CommentRole::Version | CommentRole::Card => Some(0),
                CommentRole::Section => self
                    .doc
                    .sections
                    .iter()
                    .position(|s| s.line == comment.span.start_line)
                    .and_then(|i| section_levels[i]),
                CommentRole::Note => {
                    let host = comment.unit.map_or(1, |u| self.units[u].level);
                    if comment.sigil == Some(Sigil::Unsure) || comment.tag.is_some() {
                        Some(host.max(2))
                    } else {
                        Some(host)
                    }
                }
            };
            if comment.level_marker.is_some() && comment.unit.is_none() {
                let line = comment.span.start_line;
                self.warn(
                    "W051",
                    line,
                    "level marker is not attached to a statement".into(),
                );
            }
            self.doc.comments[c].level = level;
        }

        // Line levels: units (parents first, children override), then own-line comments.
        let n = self.idx.line_count() as usize;
        let mut lines: Vec<Option<u8>> = vec![None; n];
        for u in &self.units {
            for l in u.span.start_line..=u.span.end_line {
                lines[(l - 1) as usize] = Some(u.level);
            }
        }
        for c in &self.doc.comments {
            if c.placement == Placement::OwnLine {
                for l in c.span.start_line..=c.span.end_line {
                    lines[(l - 1) as usize] = c.level;
                }
            }
        }
        for (i, level) in lines.iter_mut().enumerate() {
            if self
                .idx
                .line_text(self.text, i as u32 + 1)
                .trim()
                .is_empty()
            {
                *level = None;
            }
        }
        self.doc.lines = lines;
    }

    /// The entry unit that `u` belongs to, if any.
    fn entry_ancestor(&self, u: usize) -> Option<usize> {
        let mut cur = Some(u);
        while let Some(c) = cur {
            if self.units[c].parent.is_none() {
                return self.doc.entries.iter().any(|e| e.unit == c).then_some(c);
            }
            cur = self.units[c].parent;
        }
        None
    }

    /// Is `u` a direct option of an entry (`trigger`, `access`, `handle`, ...)?
    fn is_entry_option(&self, u: usize) -> bool {
        self.doc.entries.iter().any(|e| {
            e.options.iter().any(|o| o.unit == u)
                || e.handle.as_ref().is_some_and(|h| h.unit == u)
                || (self.units[u].parent == Some(e.unit)
                    && matches!(self.units[u].label.as_str(), "trigger" | "access" | "input"))
        })
    }

    // ─── risks ──────────────────────────────────────────────────────────────

    fn collect_risks(&mut self) {
        for c in &self.doc.comments {
            let Some(sigil) = c.sigil.filter(|s| s.is_risk()) else {
                continue;
            };
            let owner = c.unit.map(|u| {
                let mut top = u;
                while let Some(p) = self.units[top].parent {
                    top = p;
                }
                self.units[top].label.clone()
            });
            self.doc.risks.push(Risk {
                comment: c.id,
                kind: sigil,
                text: c.text.clone(),
                line: c.span.start_line,
                unit: c.unit,
                owner,
            });
        }
    }

    // ─── structure checks ───────────────────────────────────────────────────

    fn check_structure(&mut self) {
        if self.doc.version.is_none() {
            self.error(
                "E001",
                1,
                "the first line must be `// flowmark <version>`".into(),
            );
        }
        if self.doc.card.is_none() {
            self.error(
                "E002",
                1,
                "missing review card (`/** ... */` before the first declaration)".into(),
            );
        }
        // Budgets (§10.3).
        for e in self.doc.entries.clone() {
            let count = self.count_statements(e.unit, |l| l == 1);
            if count > L1_BUDGET_PER_ENTRY {
                let line = self.units[e.unit].span.start_line;
                let name = e.name.unwrap_or_default();
                self.warn(
                    "W070",
                    line,
                    format!(
                        "`{name}` has {count} L1 statements (budget {L1_BUDGET_PER_ENTRY}); extract a helper (§10.3)"
                    ),
                );
            }
        }
        for f in self.doc.functions.clone() {
            let count = self.count_statements(f.unit, |l| l == 2);
            if count > L2_BUDGET_PER_HELPER {
                let line = self.units[f.unit].span.start_line;
                self.warn(
                    "W071",
                    line,
                    format!("`{}` has {count} L2 statements (budget {L2_BUDGET_PER_HELPER}); split it (§10.3)", f.name),
                );
            }
        }
    }

    fn count_statements(&self, root: usize, level: impl Fn(u8) -> bool) -> usize {
        self.units
            .iter()
            .filter(|u| {
                u.kind == UnitKind::Statement && level(u.level) && self.is_descendant(u.id, root)
            })
            .count()
    }

    fn is_descendant(&self, mut u: usize, root: usize) -> bool {
        while let Some(p) = self.units[u].parent {
            if p == root {
                return true;
            }
            u = p;
        }
        false
    }
}

/// `// --- data ---` → `data`.
fn section_name(body: &str) -> Option<String> {
    let t = body.trim();
    let inner = t.strip_prefix("---")?.strip_suffix("---")?;
    let name = inner.trim().trim_matches('-').trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// Parses a trigger string (§6.1).
pub fn parse_trigger(raw: &str) -> Result<Trigger, String> {
    let (first, rest) = raw
        .trim()
        .split_once(char::is_whitespace)
        .unwrap_or((raw.trim(), ""));
    let rest = rest.trim().to_string();
    let t = |kind: &str, method: Option<&str>| Trigger {
        raw: raw.to_string(),
        kind: kind.to_string(),
        name: rest.clone(),
        method: method.map(str::to_string),
    };
    let trigger = match first {
        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" => {
            if !rest.starts_with('/') {
                return Err(format!(
                    "HTTP trigger `{raw}` needs a path starting with `/`"
                ));
            }
            t("http", Some(first))
        }
        "event" | "job" | "message" | "schedule" | "cli" | "rpc" | "ws" => t(first, None),
        "query" | "mutation" | "subscription" => t("graphql", Some(first)),
        _ => {
            return Err(format!(
                "unknown trigger kind `{first}` (HTTP method, event, job, message, schedule, cli, query, mutation, subscription, rpc, ws)"
            ));
        }
    };
    if trigger.name.is_empty() {
        return Err(format!("trigger `{raw}` needs a name after `{first}`"));
    }
    Ok(trigger)
}
