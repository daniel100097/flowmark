//! The spec example (`examples/register-user.review.ts`) end to end.
//!
//! View snapshots live in `tests/snapshots/`. Regenerate them with `UPDATE_SNAPSHOTS=1 cargo test`.

use flowmark_parser::{Sigil, UnitKind, parse, view};

const EXAMPLE: &str = include_str!("../../../examples/register-user.review.ts");

#[test]
fn example_has_exactly_its_known_finding() {
    let doc = parse(EXAMPLE);
    // The example's real bug: the entry declares `input` but the handler passes `req.raw` on.
    let found: Vec<_> = doc.diagnostics.iter().map(|d| (d.code, d.line)).collect();
    assert_eq!(found, [("W060", 53)], "{:#?}", doc.diagnostics);
    assert_eq!(doc.version.as_deref(), Some("0.1"));
}

#[test]
fn card() {
    let doc = parse(EXAMPLE);
    let card = doc.card.unwrap();
    assert_eq!(card.title, "Register user");
    let access = &card.fields[0];
    assert_eq!(access.name, "access");
    assert_eq!(
        access
            .items
            .iter()
            .map(|i| i.name.as_str())
            .collect::<Vec<_>>(),
        ["public", "system"]
    );
    assert_eq!(access.items[0].note.as_deref(), Some("registerUser"));
}

#[test]
fn sections() {
    let doc = parse(EXAMPLE);
    let names: Vec<_> = doc.sections.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["data", "flow", "details", "async"]);
}

#[test]
fn entries() {
    let doc = parse(EXAMPLE);
    let [register, welcome] = doc.entries.as_slice() else {
        panic!("{:#?}", doc.entries)
    };

    assert_eq!(register.name.as_deref(), Some("registerUser"));
    let t = register.trigger.as_ref().unwrap();
    assert_eq!(
        (t.kind.as_str(), t.method.as_deref(), t.name.as_str()),
        ("http", Some("POST"), "/api/users")
    );
    assert_eq!(register.access.as_deref(), Some("\"public\""));
    assert_eq!(register.input.as_deref(), Some("SignupInput"));
    assert_eq!(
        register
            .options
            .iter()
            .map(|o| o.name.as_str())
            .collect::<Vec<_>>(),
        ["rateLimit"]
    );
    let h = register.handle.as_ref().unwrap();
    assert_eq!(h.return_type.as_deref(), Some("Promise<UserResponse>"));
    assert!(h.is_async);

    assert_eq!(welcome.name.as_deref(), Some("sendWelcomeMail"));
    assert_eq!(welcome.trigger.as_ref().unwrap().kind, "event");
    assert_eq!(
        welcome
            .options
            .iter()
            .map(|o| o.name.as_str())
            .collect::<Vec<_>>(),
        ["queued", "retries"]
    );
}

#[test]
fn types_and_functions() {
    let doc = parse(EXAMPLE);
    let names: Vec<_> = doc.types.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(
        names,
        ["SignupInput", "User", "Address", "UserResponse", "Events"]
    );
    let user = &doc.types[1];
    assert_eq!(
        user.fields
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>(),
        ["id", "name", "email", "password", "role"]
    );
    assert_eq!(doc.functions.len(), 1);
    assert_eq!(doc.functions[0].name, "createUser");
    assert_eq!(doc.functions[0].params, "data: Raw");
}

#[test]
fn refs_and_scopes() {
    let doc = parse(EXAMPLE);
    // `const user = await createUser(req.raw) // @ "$request->all()"` searches in the handle's scope.
    let r = doc
        .refs
        .iter()
        .find(|r| r.quote.as_deref() == Some("$request->all()"))
        .unwrap();
    assert_eq!(
        r.resolved.as_ref().unwrap().symbol.as_deref(),
        Some("UserController.store")
    );
    // `@ "protected $fillable"` on a field searches in the type's scope (a path).
    let r = doc
        .refs
        .iter()
        .find(|r| r.quote.as_deref() == Some("protected $fillable"))
        .unwrap();
    assert_eq!(
        r.resolved.as_ref().unwrap().path.as_deref(),
        Some("app/Models/User.php")
    );
    // `used by:` above a function does not make a scope; the end-of-line ref on its first line does.
    let f = &doc.units[doc.functions[0].unit];
    let scope = &doc.refs[f.scope.unwrap()];
    assert_eq!(
        scope.target.as_ref().unwrap().symbol.as_deref(),
        Some("UserService.create")
    );
}

#[test]
fn risks() {
    let doc = parse(EXAMPLE);
    let owners: Vec<_> = doc
        .risks
        .iter()
        .map(|r| r.owner.as_deref().unwrap())
        .collect();
    assert_eq!(
        owners,
        [
            "type User",
            "registerUser",
            "registerUser",
            "createUser",
            "sendWelcomeMail"
        ]
    );
    assert!(doc.risks.iter().all(|r| r.kind == Sigil::Risk));
}

#[test]
fn effects_and_calls() {
    let doc = parse(EXAMPLE);
    let effects: Vec<_> = doc
        .effects
        .iter()
        .map(|e| {
            format!(
                "{}:{}:{}",
                e.category,
                e.call,
                e.name.clone().unwrap_or_default()
            )
        })
        .collect();
    assert_eq!(
        effects,
        [
            "input:shape:SignupInput",
            "output:respond:201",
            "consistency:db.transaction:",
            "write:db.insert:User",
            "write:db.insert:Address",
            "effect:emit:UserRegistered",
            "effect:mail:WelcomeMail",
        ]
    );
    assert_eq!(
        doc.calls
            .iter()
            .map(|c| c.callee.as_str())
            .collect::<Vec<_>>(),
        ["createUser"]
    );
}

#[test]
fn levels() {
    let doc = parse(EXAMPLE);
    let unit = |label: &str| doc.units.iter().find(|u| u.label == label).unwrap();
    assert_eq!(unit("type User").level, 1);
    assert_eq!(unit("role").level, 2, "fields of multi-line types are L2");
    assert_eq!(unit("userId").level, 1, "single-line types are shown whole");
    assert_eq!(unit("registerUser").level, 1);
    assert_eq!(unit("createUser").level, 2, "helpers are L2");
    let email = doc
        .units
        .iter()
        .find(|u| u.label == "email" && u.kind == UnitKind::Statement)
        .unwrap();
    assert_eq!((email.marker, email.level), (Some(3), 3));
    assert_eq!(doc.lines[0], Some(0), "version line");
    assert_eq!(doc.lines.len(), EXAMPLE.lines().count());
}

#[test]
fn view_snapshots() {
    let doc = parse(EXAMPLE);
    for level in 0..=3 {
        let actual = view::render(&doc, EXAMPLE, level);
        let path = format!(
            "{}/tests/snapshots/register-user.L{level}.txt",
            env!("CARGO_MANIFEST_DIR")
        );
        if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
            std::fs::write(&path, &actual).unwrap();
            continue;
        }
        let expected =
            std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("missing snapshot {path}"));
        assert_eq!(
            actual, expected,
            "L{level} view changed; rerun with UPDATE_SNAPSHOTS=1 if intended"
        );
    }
}
