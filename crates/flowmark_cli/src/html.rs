//! A self-contained HTML page: view model + viewer script + styles, no network access needed.

use flowmark_parser::viewmodel::ViewModel;

const VIEWER_JS: &str = include_str!("../../../viewer/viewer.js");
const VIEWER_CSS: &str = include_str!("../../../viewer/viewer.css");

pub fn page(model: &ViewModel, level: u8) -> Result<String, String> {
    // `<` is escaped so the JSON cannot close the <script> element.
    let json = serde_json::to_string(model)
        .map_err(|e| e.to_string())?
        .replace('<', "\\u003c");
    let title = escape(model.title.as_deref().unwrap_or(&model.file));
    Ok(format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="generator" content="flowmark {version}">
<title>{title} · Flowmark</title>
<style>
html, body {{ margin: 0; }}
{VIEWER_CSS}</style>
</head>
<body class="flowmark">
<div id="flowmark"></div>
<script type="application/json" id="flowmark-model">{json}</script>
<script>
{VIEWER_JS}</script>
<script>
Flowmark.mount(document.getElementById("flowmark"), JSON.parse(document.getElementById("flowmark-model").textContent), {{ level: {level} }});
</script>
</body>
</html>
"#,
        version = env!("CARGO_PKG_VERSION"),
    ))
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
