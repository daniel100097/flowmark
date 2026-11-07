//! A real review written by an AI for pingdotgg/t3code PR #25709 (`examples/t3code-pr-25709/`):
//! base and head versions of the same file.

use flowmark_parser::{parse, view, viewmodel};

const BASE: &str = include_str!("../../../examples/t3code-pr-25709/base.review.ts");
const HEAD: &str = include_str!("../../../examples/t3code-pr-25709/head.review.ts");

#[test]
fn both_versions_are_clean() {
    for (name, src) in [("base", BASE), ("head", HEAD)] {
        let doc = parse(src);
        assert!(doc.diagnostics.is_empty(), "{name}: {:#?}", doc.diagnostics);
        assert_eq!(doc.entries.len(), 2, "{name}");
    }
}

#[test]
fn entries() {
    let doc = parse(HEAD);
    let rpc = &doc.entries[0];
    assert_eq!(rpc.name.as_deref(), Some("updateServerSettings"));
    let t = rpc.trigger.as_ref().unwrap();
    assert_eq!(
        (t.kind.as_str(), t.name.as_str()),
        ("rpc", "server.updateSettings")
    );
    assert_eq!(rpc.input.as_deref(), Some("SettingsUpdate"));
    let push = &doc.entries[1];
    assert_eq!(push.trigger.as_ref().unwrap().kind, "event");
}

#[test]
fn diff_finds_what_the_pr_changed() {
    let (b, h) = (parse(BASE), parse(HEAD));
    let m = viewmodel::build_diff(&b, BASE, "base.review.ts", &h, HEAD, "head.review.ts");
    let ch = m.changes.unwrap();
    let card: Vec<_> = ch.card.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(card, ["access", "input", "config", "tests"]);
    assert_eq!(ch.risks_added.len(), 2);
    assert!(ch.risks_removed.is_empty());
    let patch = ch
        .types
        .iter()
        .find(|t| t.name == "ServerSettingsPatch")
        .unwrap();
    assert_eq!(patch.details, ["+ secretProviders"]);
    assert!(
        ch.types
            .iter()
            .any(|t| t.name == "SecretProvider" && t.change == "added")
    );
}

#[test]
fn l1_view_bubbles_both_risks() {
    let doc = parse(HEAD);
    let text = view::render(&doc, HEAD, 1);
    assert!(
        text.contains("! `config` is never decoded on the server"),
        "{text}"
    );
    assert!(text.contains("!? add and edit send the same"), "{text}");
    // One badge group per line, even when the line calls two hidden helpers.
    let line = text
        .lines()
        .find(|l| l.contains("const settings = await saveSettings("))
        .unwrap();
    assert_eq!(line.matches('▸').count(), 1, "{line}");
}
