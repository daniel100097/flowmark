//! One test per diagnostic, plus scanner edge cases.

use flowmark_parser::{Placement, Severity, parse};

const HEADER: &str = "// flowmark 0.1
/**
 * Test
 * A test file.
 *
 * access:   public
 * input:    none
 * writes:   none
 * effects:  none
 * output:   200 string
 */
";

fn with_header(body: &str) -> String {
    format!("{HEADER}\n{body}")
}

fn codes(source: &str) -> Vec<&'static str> {
    parse(source).diagnostics.iter().map(|d| d.code).collect()
}

const ENTRY: &str = r#"const ping = entry({
  trigger: "GET /ping",
  access: "public",
  handle(req): string {
    return "pong"
  },
})
"#;

#[test]
fn clean_file() {
    let doc = parse(&with_header(ENTRY));
    assert!(doc.diagnostics.is_empty(), "{:#?}", doc.diagnostics);
}

#[test]
fn e000_syntax_error() {
    assert!(codes(&with_header("const x = (\n")).contains(&"E000"));
}

#[test]
fn e001_missing_version() {
    assert!(codes(&HEADER.replace("// flowmark 0.1\n", "")).contains(&"E001"));
}

#[test]
fn e002_missing_card() {
    assert!(codes("// flowmark 0.1\nconst a = 1\n").contains(&"E002"));
}

#[test]
fn e003_card_problems() {
    let src = with_header("").replace(" * input:    none\n", "");
    let doc = parse(&src);
    let d = doc.diagnostics.iter().find(|d| d.code == "E003").unwrap();
    assert!(d.message.contains("`input`"), "{}", d.message);
}

#[test]
fn e010_e011_sections() {
    let c = codes(&with_header(
        "// --- flow ---\n\n// --- data ---\n\n// --- extras ---\n",
    ));
    assert!(c.contains(&"E011"), "{c:?}");
    assert!(c.contains(&"E010"), "{c:?}");
}

#[test]
fn e020_entry_not_in_const() {
    let src = with_header(r#"entry({ trigger: "GET /x", access: "public", handle() {} })"#);
    assert!(codes(&src).contains(&"E020"));
}

#[test]
fn e021_missing_access() {
    let src = with_header(&ENTRY.replace("  access: \"public\",\n", ""));
    let doc = parse(&src);
    assert!(
        doc.diagnostics
            .iter()
            .any(|d| d.code == "E021" && d.message.contains("`access`"))
    );
}

#[test]
fn e022_unknown_trigger() {
    assert!(codes(&with_header(&ENTRY.replace("GET /ping", "FETCH /ping"))).contains(&"E022"));
    assert!(codes(&with_header(&ENTRY.replace("GET /ping", "event"))).contains(&"E022"));
}

#[test]
fn e023_options_share_a_line() {
    let src = with_header(&ENTRY.replace("  access: \"public\",\n", "").replace(
        "trigger: \"GET /ping\",",
        "trigger: \"GET /ping\", access: \"public\",",
    ));
    assert!(codes(&src).contains(&"E023"));
}

#[test]
fn e024_access_cannot_be_lowered() {
    let src = with_header(&ENTRY.replace("access: \"public\",", "access: \"public\", // L2"));
    let doc = parse(&src);
    assert!(doc.diagnostics.iter().any(|d| d.code == "E024"));
    let access = doc.units.iter().find(|u| u.label == "access").unwrap();
    assert_eq!(access.level, 1);
}

#[test]
fn e030_bad_comment() {
    assert!(
        codes(&with_header(
            &ENTRY.replace("return \"pong\"", "return \"pong\" // @ \"open")
        ))
        .contains(&"E030")
    );
}

#[test]
fn e031_quote_without_scope() {
    assert!(codes(&with_header("const a = 1 // @ \"x\"\n")).contains(&"E031"));
}

#[test]
fn e080_imports() {
    assert!(codes(&with_header("import x from \"y\"\n")).contains(&"E080"));
    assert!(codes(&with_header("export const a = 1\n")).contains(&"E080"));
}

#[test]
fn w024_option_order() {
    let src = with_header(&ENTRY.replace(
        "  trigger: \"GET /ping\",\n  access: \"public\",\n",
        "  access: \"public\",\n  trigger: \"GET /ping\",\n",
    ));
    assert!(codes(&src).contains(&"W024"));
}

#[test]
fn w040_floating_comment() {
    assert!(codes(&with_header("// floating\n\nconst a = 1\n")).contains(&"W040"));
}

#[test]
fn w050_ineffective_marker() {
    let src = with_header("function helper() {\n  const a = 1 // L1\n}\n");
    let doc = parse(&src);
    assert!(
        doc.diagnostics
            .iter()
            .any(|d| d.code == "W050" && d.severity == Severity::Warning)
    );
}

#[test]
fn error_path_defaults_to_l2_and_can_be_promoted() {
    let body = r#"const refund = entry({
  trigger: "POST /refund",
  access: "user",
  handle(req): string {
    if (!req.raw) fail(404)
    // L1
    if (req.raw.status !== "paid") fail(409)
    return "ok"
  },
})
"#;
    let doc = parse(&with_header(body));
    assert!(doc.diagnostics.is_empty(), "{:#?}", doc.diagnostics);
    let ifs: Vec<_> = doc
        .units
        .iter()
        .filter(|u| u.syntax == "IfStatement")
        .map(|u| u.level)
        .collect();
    assert_eq!(ifs, [2, 1]);
}

#[test]
fn comments_inside_strings_templates_and_regexes_are_not_comments() {
    let body = r#"const a = "http://example.com" // @ A.b
const b = `x ${"//"} // y` // real
const c = /a\/\/b/ // also real
"#;
    let doc = parse(&with_header(body));
    let notes: Vec<_> = doc
        .comments
        .iter()
        .filter(|c| c.placement == Placement::EndOfLine)
        .map(|c| c.raw.as_str())
        .collect();
    assert_eq!(notes, ["// @ A.b", "// real", "// also real"]);
}

#[test]
fn all_trigger_kinds() {
    for (trigger, kind) in [
        ("GET /x", "http"),
        ("event UserRegistered", "event"),
        ("job SendInvoice", "job"),
        ("message orders.created", "message"),
        ("schedule daily 03:00", "schedule"),
        ("cli users:import", "cli"),
        ("mutation createUser", "graphql"),
        ("rpc UserService.Create", "rpc"),
        ("ws chat.message", "ws"),
    ] {
        let t = flowmark_parser::parse_trigger(trigger).unwrap();
        assert_eq!(t.kind, kind, "{trigger}");
    }
}

const WITH_INPUT: &str = r#"const signup = entry({
  trigger: "POST /signup",
  access: "public",
  input: shape<SignupInput>(),
  handle(req): string {
    return save(req.input)
  },
})
"#;

#[test]
fn input_is_recorded() {
    let doc = parse(&with_header(WITH_INPUT));
    assert!(doc.diagnostics.is_empty(), "{:#?}", doc.diagnostics);
    assert_eq!(doc.entries[0].input.as_deref(), Some("SignupInput"));
    assert!(
        doc.entries[0].options.is_empty(),
        "input is not a plain option"
    );
}

#[test]
fn e025_input_must_be_a_shape() {
    let src = with_header(&WITH_INPUT.replace("shape<SignupInput>()", "SignupInput"));
    assert!(codes(&src).contains(&"E025"));
}

#[test]
fn e026_no_input_on_events() {
    let src = with_header(
        r#"const onSignup = entry({
  trigger: "event UserRegistered",
  access: "system",
  input: shape<SignupInput>(),
  queued: true,
  handle(e) {},
})
"#,
    );
    assert!(codes(&src).contains(&"E026"));
}

#[test]
fn w060_declared_input_but_reads_raw() {
    let src = with_header(&WITH_INPUT.replace("save(req.input)", "save(req.raw)"));
    let doc = parse(&src);
    let d = doc
        .diagnostics
        .iter()
        .find(|d| d.code == "W060")
        .expect("W060");
    assert!(d.message.contains("req.raw"), "{}", d.message);
    // Without a declared input, reading raw is not flagged (there is nothing better to read).
    let src = with_header(
        &WITH_INPUT
            .replace("  input: shape<SignupInput>(),\n", "")
            .replace("save(req.input)", "save(req.raw)"),
    );
    assert!(!codes(&src).contains(&"W060"));
}

#[test]
fn input_cannot_be_lowered() {
    let src = with_header(&WITH_INPUT.replace(
        "input: shape<SignupInput>(),",
        "input: shape<SignupInput>(), // L2",
    ));
    assert!(codes(&src).contains(&"E024"));
}
