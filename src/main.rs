// Copyright (c) 2026 elci-group
// SPDX-License-Identifier: MIT
//! verity checks whether a project's README backs its claims with evidence
//! in the code, and flags claims that a plain-text/keyword pass alone would
//! call implemented but that no test ever exercises.
//!
//! None of uni's default suite (amber, bart, chakra, ferret, fract, isopod,
//! lwoodz, tempcheq, traci, vamos) checks this. Each of them scores the code
//! against itself — dependency health, entropy, license text, panic paths,
//! observability. None of them cross-reference what the README *claims*
//! against what the source actually *does*. That gap let oip ship a
//! `--revert` flag whose path-suffix math silently restored to the wrong
//! filename (never touching the original), and a README that advertised a
//! "diff preview before overwriting" the code never rendered — both invisible
//! to a codebase-only scan, and both exactly the shape of bug verity is built
//! to surface: a documented promise with no corresponding evidence, or
//! evidence that exists but nothing exercises.

use clap::Parser;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

mod rules;
use rules::{ClaimRule, Severity, CLAIM_RULES};

#[derive(Parser)]
#[command(
    name = "verity",
    version,
    about = "Checks whether a README's claims are backed by evidence in the code"
)]
struct Cli {
    /// Project directory to check (defaults to the current directory)
    path: Option<PathBuf>,

    /// Emit machine-readable JSON instead of a human report
    #[arg(long)]
    json: bool,
}

#[derive(Serialize)]
struct Finding {
    id: String,
    severity: String,
    message: String,
}

#[derive(Serialize)]
struct Report {
    project: String,
    readme_found: bool,
    findings: Vec<Finding>,
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let root = cli.path.unwrap_or_else(|| PathBuf::from("."));

    let readme = find_readme(&root);
    let Some(readme_path) = readme else {
        let report = Report {
            project: root.display().to_string(),
            readme_found: false,
            findings: Vec::new(),
        };
        emit(
            &report,
            cli.json,
            "no README found — nothing to check claims against",
        );
        return std::process::ExitCode::SUCCESS;
    };

    let readme_text = fs::read_to_string(&readme_path).unwrap_or_default();
    let corpus = build_corpus(&root);
    let test_corpus = build_test_corpus(&root);

    let mut findings = Vec::new();
    for rule in CLAIM_RULES {
        if let Some(finding) = check_rule(rule, &readme_text, &corpus, &test_corpus) {
            findings.push(finding);
        }
    }

    let report = Report {
        project: root.display().to_string(),
        readme_found: true,
        findings,
    };
    let has_findings = !report.findings.is_empty();
    emit(&report, cli.json, "");

    if has_findings {
        std::process::ExitCode::FAILURE
    } else {
        std::process::ExitCode::SUCCESS
    }
}

fn check_rule(rule: &ClaimRule, readme: &str, corpus: &str, test_corpus: &str) -> Option<Finding> {
    let readme_lower = readme.to_lowercase();
    let claims = rule
        .claim_phrases
        .iter()
        .any(|p| readme_lower.contains(&p.to_lowercase()));
    if !claims {
        return None;
    }

    let corpus_lower = corpus.to_lowercase();
    let has_evidence = rule
        .evidence_phrases
        .iter()
        .any(|p| corpus_lower.contains(&p.to_lowercase()));

    if !has_evidence {
        return Some(Finding {
            id: rule.id.to_string(),
            severity: rule.severity.as_str().to_string(),
            message: format!(
                "README claims \"{}\" but no evidence of it was found in the code ({})",
                rule.claim_phrases[0], rule.message
            ),
        });
    }

    if rule.requires_test_coverage {
        let test_lower = test_corpus.to_lowercase();
        let tested = rule
            .evidence_phrases
            .iter()
            .any(|p| test_lower.contains(&p.to_lowercase()))
            || rule
                .test_keywords
                .iter()
                .any(|k| test_lower.contains(&k.to_lowercase()));
        if !tested {
            return Some(Finding {
                id: format!("{}-untested", rule.id),
                severity: Severity::Major.as_str().to_string(),
                message: format!(
                    "README claims \"{}\" and the code implements it, but no test exercises it ({})",
                    rule.claim_phrases[0], rule.message
                ),
            });
        }
    }

    None
}

fn find_readme(root: &Path) -> Option<PathBuf> {
    for name in [
        "README.md",
        "readme.md",
        "Readme.md",
        "README.markdown",
        "README",
    ] {
        let candidate = root.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

const IGNORED_DIRS: &[&str] = &[
    "target",
    "node_modules",
    ".git",
    "dist",
    "build",
    ".uni",
    ".kaptaind",
];
const SOURCE_EXTENSIONS: &[&str] = &[
    "rs", "py", "js", "ts", "jsx", "tsx", "go", "java", "kt", "swift", "rb", "toml", "json",
];

/// Concatenates every source/manifest file under `root` (skipping build and
/// VCS directories) into one lowercase-searchable corpus. Bounded to keep
/// memory and runtime predictable on large repos.
fn build_corpus(root: &Path) -> String {
    let mut corpus = String::new();
    walk(root, &mut |path| {
        if is_source_file(path) {
            if let Ok(contents) = fs::read_to_string(path) {
                corpus.push_str(&contents);
                corpus.push('\n');
            }
        }
        corpus.len() < 8_000_000
    });
    corpus
}

/// Same as `build_corpus` but limited to files under a `tests/` directory or
/// `#[cfg(test)]` modules within source files, so claims can be checked for
/// test coverage specifically rather than just any mention in the codebase.
fn build_test_corpus(root: &Path) -> String {
    let mut corpus = String::new();
    walk(root, &mut |path| {
        if is_source_file(path) {
            let under_tests_dir = path
                .components()
                .any(|c| c.as_os_str() == "tests" || c.as_os_str() == "test");
            if under_tests_dir {
                if let Ok(contents) = fs::read_to_string(path) {
                    corpus.push_str(&contents);
                    corpus.push('\n');
                }
            } else if let Ok(contents) = fs::read_to_string(path) {
                corpus.push_str(&extract_test_regions(&contents));
                corpus.push('\n');
            }
        }
        corpus.len() < 8_000_000
    });
    corpus
}

/// Extracts the body of every `#[cfg(test)]` / `#[test]` region from Rust
/// source by brace-depth tracking from each such attribute to its enclosing
/// item's closing brace. Non-Rust files are returned unchanged, since most of
/// the other supported languages don't fence tests behind a source-level
/// attribute the same way (their tests already live under a `tests/`-style
/// path, which `build_test_corpus` handles separately).
fn extract_test_regions(content: &str) -> String {
    if !content.contains("#[cfg(test)]") && !content.contains("#[test]") {
        return String::new();
    }
    let mut out = String::new();
    let mut search_from = 0usize;
    while let Some(start) = next_test_marker(&content[search_from..]).map(|rel| search_from + rel) {
        let mut depth = 0i32;
        let mut seen_brace = false;
        let mut end = content.len();
        for (offset, ch) in content[start..].char_indices() {
            match ch {
                '{' => {
                    depth += 1;
                    seen_brace = true;
                }
                '}' => {
                    depth -= 1;
                    if seen_brace && depth <= 0 {
                        end = start + offset + ch.len_utf8();
                        break;
                    }
                }
                ';' if !seen_brace => {
                    end = start + offset + ch.len_utf8();
                    break;
                }
                _ => {}
            }
        }
        out.push_str(&content[start..end]);
        out.push('\n');
        search_from = end;
    }
    out
}

/// Byte offset (relative to `s`) of whichever comes first: `#[cfg(test)]` or
/// `#[test]`. `str::find` only ever returns char-boundary offsets, so this is
/// safe to use for subsequent slicing — unlike scanning byte-by-byte, which
/// panics the moment it lands inside a multi-byte character (e.g. an em dash
/// in a doc comment).
fn next_test_marker(s: &str) -> Option<usize> {
    let cfg_test = s.find("#[cfg(test)]");
    let test = s.find("#[test]");
    match (cfg_test, test) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

fn is_source_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| SOURCE_EXTENSIONS.contains(&e))
}

fn walk(dir: &Path, visit: &mut impl FnMut(&Path) -> bool) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name();
            if IGNORED_DIRS.contains(&name.to_string_lossy().as_ref()) {
                continue;
            }
            walk(&path, visit);
        } else if !visit(&path) {
            return;
        }
    }
}

fn emit(report: &Report, json: bool, empty_note: &str) {
    if json {
        println!("{}", serde_json::to_string_pretty(report).unwrap());
        return;
    }

    println!(
        "verity — README claim vs. code evidence for {}",
        report.project
    );
    if !report.readme_found {
        println!("  {empty_note}");
        return;
    }
    if report.findings.is_empty() {
        println!(
            "  no unbacked or untested claims found ({} rule(s) checked)",
            CLAIM_RULES.len()
        );
        return;
    }
    for f in &report.findings {
        println!("  [{}] {} — {}", f.severity, f.id, f.message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_test_regions_isolates_cfg_test_block() {
        let content = r#"
fn real_fn() {}

#[cfg(test)]
mod tests {
    #[test]
    fn revert_round_trips() {
        assert!(true);
    }
}
"#;
        let extracted = extract_test_regions(content);
        assert!(extracted.contains("revert_round_trips"));
        assert!(!extracted.contains("fn real_fn"));
    }

    #[test]
    fn extract_test_regions_empty_when_no_tests() {
        assert_eq!(extract_test_regions("fn f() {}"), "");
    }

    // Regression test: dogfooding verity against its own source panicked
    // here ("start byte index N is not a char boundary") because the old
    // byte-by-byte scan advanced one byte at a time through multi-byte UTF-8
    // (e.g. an em dash in a doc comment) and then sliced at that offset.
    #[test]
    fn extract_test_regions_handles_multibyte_utf8_before_test_marker() {
        let content =
            "// note — an em dash before the marker\n#[test]\nfn t() { assert!(true); }\n";
        let extracted = extract_test_regions(content);
        assert!(extracted.contains("fn t()"));
    }

    #[test]
    fn check_rule_flags_claim_with_no_evidence() {
        let rule = &CLAIM_RULES[0];
        let readme = format!("This tool has {}.", rule.claim_phrases[0]);
        let finding = check_rule(rule, &readme, "", "");
        assert!(finding.is_some());
    }

    #[test]
    fn check_rule_silent_when_claim_absent() {
        let rule = &CLAIM_RULES[0];
        let finding = check_rule(rule, "totally unrelated readme text", "", "");
        assert!(finding.is_none());
    }

    #[test]
    fn check_rule_silent_when_evidence_present() {
        let rule = &CLAIM_RULES[0];
        let readme = format!("This tool has {}.", rule.claim_phrases[0]);
        let corpus = rule.evidence_phrases[0].to_string();
        let finding = check_rule(rule, &readme, &corpus, &corpus);
        assert!(finding.is_none());
    }

    // Regression case for the bug that motivated verity: a claimed,
    // implemented revert/restore feature with zero test coverage.
    #[test]
    fn revert_rule_flags_implemented_but_untested_claim() {
        let rule = rules::CLAIM_RULES
            .iter()
            .find(|r| r.id == "revert-restore")
            .expect("revert-restore rule present");
        let readme = format!("Supports {} of the last change.", rule.claim_phrases[0]);
        let corpus = rule.evidence_phrases[0].to_string();
        let finding =
            check_rule(rule, &readme, &corpus, "").expect("should flag missing test coverage");
        assert!(finding.id.ends_with("-untested"));
    }
}
