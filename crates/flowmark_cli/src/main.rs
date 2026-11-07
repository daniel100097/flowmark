//! `flowmark` command line.
//!
//! ```text
//! flowmark parse <file>              document as JSON
//! flowmark check <file>...           diagnostics; exit code 1 on errors
//! flowmark view  <file> [--level N]  text view at detail level 0–3 (default 1)
//! flowmark html  <file> [<head-file>] [-o out.html] [--level N]
//!                                    interactive viewer as one HTML file; with two files, a diff
//! flowmark model <file> [<head-file>]  view model JSON (what the viewer renders)
//! ```

use std::process::ExitCode;

mod html;

use flowmark_parser::{Severity, parse, view, viewmodel};

const USAGE: &str = "usage:
  flowmark parse <file>              document as JSON
  flowmark check <file>...           diagnostics; exit code 1 on errors
  flowmark view  <file> [--level N]  text view at detail level 0-3 (default 1)
  flowmark html  <file> [<head-file>] [-o out.html] [--level N]
                                     interactive viewer as one HTML file;
                                     with two files: base and head, shown as a diff
  flowmark model <file> [<head-file>]  view model JSON (what the viewer renders)";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

fn run(args: &[String]) -> Result<ExitCode, String> {
    let (command, rest) = args.split_first().ok_or(USAGE)?;
    match command.as_str() {
        "parse" => {
            let [file] = rest else {
                return Err(USAGE.into());
            };
            let doc = parse(&read(file)?);
            let json = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
            println!("{json}");
            Ok(ExitCode::SUCCESS)
        }
        "check" => {
            if rest.is_empty() {
                return Err(USAGE.into());
            }
            let mut errors = 0;
            for file in rest {
                let doc = parse(&read(file)?);
                for d in &doc.diagnostics {
                    let severity = match d.severity {
                        Severity::Error => {
                            errors += 1;
                            "error"
                        }
                        Severity::Warning => "warning",
                    };
                    println!("{file}:{}: {severity} {}: {}", d.line, d.code, d.message);
                }
            }
            Ok(if errors > 0 {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            })
        }
        "view" => {
            let mut file = None;
            let mut level = 1u8;
            let mut it = rest.iter();
            while let Some(a) = it.next() {
                match a.as_str() {
                    "--level" | "-l" => {
                        let v = it.next().ok_or("--level needs a value")?;
                        level = v
                            .parse()
                            .ok()
                            .filter(|l| *l <= 3)
                            .ok_or("--level must be 0, 1, 2 or 3")?;
                    }
                    f if file.is_none() => file = Some(f.to_string()),
                    _ => return Err(USAGE.into()),
                }
            }
            let file = file.ok_or(USAGE)?;
            let source = read(&file)?;
            let doc = parse(&source);
            print!("{}", view::render(&doc, &source, level));
            Ok(ExitCode::SUCCESS)
        }
        "model" => {
            let model = match rest {
                [file] => {
                    let source = read(file)?;
                    viewmodel::build(&parse(&source), &source, file)
                }
                [base, head] => {
                    let (bs, hs) = (read(base)?, read(head)?);
                    viewmodel::build_diff(&parse(&bs), &bs, base, &parse(&hs), &hs, head)
                }
                _ => return Err(USAGE.into()),
            };
            println!(
                "{}",
                serde_json::to_string(&model).map_err(|e| e.to_string())?
            );
            Ok(ExitCode::SUCCESS)
        }
        "html" => {
            let mut files = Vec::new();
            let mut out = None;
            let mut level = 1u8;
            let mut it = rest.iter();
            while let Some(a) = it.next() {
                match a.as_str() {
                    "-o" | "--out" => out = Some(it.next().ok_or("-o needs a path")?.clone()),
                    "--level" | "-l" => {
                        let v = it.next().ok_or("--level needs a value")?;
                        level = v
                            .parse()
                            .ok()
                            .filter(|l| *l <= 3)
                            .ok_or("--level must be 0, 1, 2 or 3")?;
                    }
                    f => files.push(f.to_string()),
                }
            }
            let model = match files.as_slice() {
                [file] => {
                    let source = read(file)?;
                    viewmodel::build(&parse(&source), &source, file)
                }
                [base, head] => {
                    let (bs, hs) = (read(base)?, read(head)?);
                    viewmodel::build_diff(&parse(&bs), &bs, base, &parse(&hs), &hs, head)
                }
                _ => return Err(USAGE.into()),
            };
            let page = html::page(&model, level)?;
            match out {
                Some(path) => std::fs::write(&path, page).map_err(|e| format!("{path}: {e}"))?,
                None => print!("{page}"),
            }
            Ok(ExitCode::SUCCESS)
        }
        _ => Err(USAGE.into()),
    }
}

fn read(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))
}
