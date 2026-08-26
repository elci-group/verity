// Copyright (c) 2026 elci-group
// SPDX-License-Identifier: MIT
//! The claim-vs-evidence rule table. Each rule pairs README phrases that
//! claim a capability with source-level phrases that would substantiate it.
//! Deliberately coarse substring matching, in the same spirit as ferret's
//! hunk-level heuristics: cheap, zero-setup, and tuned to catch the gap
//! between what a README promises and what the code actually does, not to
//! prove correctness of what's there.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Minor,
    Major,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Minor => "minor",
            Severity::Major => "major",
        }
    }
}

pub struct ClaimRule {
    /// Stable identifier for this rule, used as the finding id.
    pub id: &'static str,
    /// README phrases (checked case-insensitively) that claim the capability.
    /// Any one match is enough to consider the claim made.
    pub claim_phrases: &'static [&'static str],
    /// Source-corpus phrases that would substantiate the claim. Any one match
    /// counts as evidence.
    pub evidence_phrases: &'static [&'static str],
    /// When true, evidence must appear in test code (a `tests/` file or a
    /// `#[cfg(test)]`/`#[test]` region), not just anywhere in the source, or
    /// the claim is flagged as implemented-but-untested rather than
    /// unimplemented.
    pub requires_test_coverage: bool,
    /// Extra keywords (beyond `evidence_phrases`) that count as test coverage
    /// when `requires_test_coverage` is set — for cases where the test would
    /// exercise the behavior under a different name than the implementation.
    pub test_keywords: &'static [&'static str],
    /// Human-readable context appended to the finding message.
    pub message: &'static str,
    pub severity: Severity,
}

pub const CLAIM_RULES: &[ClaimRule] = &[
    ClaimRule {
        id: "diff-preview",
        claim_phrases: &[
            "diff preview",
            "shows a diff",
            "preview the diff",
            "preview before overwrit",
        ],
        evidence_phrases: &[
            "similar::",
            "textdiff",
            "unified_diff",
            "difflib",
            "diffy::",
            "print_diff",
            "fn diff",
        ],
        requires_test_coverage: false,
        test_keywords: &[],
        message: "no diff-rendering call or diff crate found",
        severity: Severity::Major,
    },
    ClaimRule {
        id: "confirmation-prompt",
        claim_phrases: &[
            "confirm before",
            "confirmation prompt",
            "are you sure",
            "prompts you before",
            "y/n prompt",
        ],
        evidence_phrases: &["read_line", "dialoguer", "confirm::", "stdin()."],
        requires_test_coverage: false,
        test_keywords: &[],
        message: "no stdin read or confirmation-prompt call found",
        severity: Severity::Major,
    },
    ClaimRule {
        id: "revert-restore",
        claim_phrases: &[
            "revert",
            "restore the previous",
            "undo the last",
            "rollback the",
        ],
        evidence_phrases: &["fn revert", "fn restore", "fn undo", "fn rollback"],
        requires_test_coverage: true,
        test_keywords: &["revert", "restore", "undo", "rollback"],
        message: "a claimed revert/restore/undo path should have a test proving it round-trips",
        severity: Severity::Major,
    },
    ClaimRule {
        id: "encryption",
        claim_phrases: &["encrypted", "encryption at rest", "end-to-end encrypt"],
        evidence_phrases: &[
            "aes",
            "chacha20",
            "ring::",
            "rustls",
            "sodiumoxide",
            "age::",
            "crypto",
        ],
        requires_test_coverage: false,
        test_keywords: &[],
        message: "no cryptography dependency or call found",
        severity: Severity::Major,
    },
    ClaimRule {
        id: "atomic-write",
        claim_phrases: &["atomic write", "atomically", "crash-safe write"],
        evidence_phrases: &["tempfile", "namedtempfile", "persist(", "rename("],
        requires_test_coverage: false,
        test_keywords: &[],
        message: "no temp-file-then-rename pattern found; a plain write() is not atomic",
        severity: Severity::Minor,
    },
    ClaimRule {
        id: "smart-project-search",
        claim_phrases: &[
            "searches your project",
            "analyzes candidate",
            "smart script matching",
            "project root detection",
        ],
        evidence_phrases: &[
            "read_dir",
            "walkdir",
            "fn find_project_root",
            "fn find_candidate",
        ],
        requires_test_coverage: false,
        test_keywords: &[],
        message: "no directory-walking / project-root-detection code found",
        severity: Severity::Minor,
    },
    ClaimRule {
        id: "backup-safety",
        claim_phrases: &["keeps backups", "backup before", "never lose", "reversible"],
        evidence_phrases: &["backup", ".bak", "fn backup"],
        requires_test_coverage: false,
        test_keywords: &[],
        message: "no backup-file logic found",
        severity: Severity::Minor,
    },
    ClaimRule {
        id: "test-suite-claim",
        claim_phrases: &[
            "test suite",
            "fully tested",
            "extensively tested",
            "covered by tests",
        ],
        evidence_phrases: &["#[test]", "def test_", "it(\"", "describe(\"", "@test"],
        requires_test_coverage: false,
        test_keywords: &[],
        message: "no test function was found anywhere in the source tree",
        severity: Severity::Major,
    },
];
