# verity

verity checks whether a project's README backs its own claims. It scans
`README.md` for phrases that promise a specific capability, then checks the
source tree for evidence that the capability actually exists — and, for
safety-critical claims like revert/restore/undo, whether a test actually
exercises it.

## Why

Every tool in the [uni](https://github.com/elci-group/uni) analysis suite
(amber, bart, chakra, ferret, fract, isopod, lwoodz, tempcheq, traci, vamos)
scores a codebase against itself: dependency health, entropy, license text,
panic paths, observability coverage. None of them cross-reference what the
README *claims* against what the code *does*.

That gap let [oip](https://github.com/elci-group/oip) ship with a `--revert`
flag whose backup/restore path math didn't invert (it derived a backup name
with `path.with_extension("oip.backup")` and undid it with
`path.with_extension("")`, which strips only the last extension segment —
`main.rs` -> `main.oip.backup` -> reverts to `main.oip`, never touching
`main.rs`) and a README that advertised a "diff preview before overwriting"
the code never rendered. Neither defect is visible to a codebase-only scan:
the code was internally consistent, just silently different from what it
claimed to do. verity is built to catch exactly that shape of bug.

## Usage

```bash
verity /path/to/project
verity --json /path/to/project
```

Exits `0` when every claim it checks is backed by evidence (or by a test, for
claims that require test coverage), `1` otherwise.

## How it works

1. Find `README.md` (or a close variant) at the project root.
2. Build a corpus of the project's source and manifest files, plus a
   separate test-only corpus (files under `tests/`, and `#[cfg(test)]` /
   `#[test]` regions extracted from source files).
3. For each rule in the claim table (see `src/rules.rs`): if the README
   contains one of the rule's claim phrases, check the corpus for one of its
   evidence phrases.
   - No evidence anywhere → **unbacked claim**.
   - Evidence exists, but the rule requires test coverage and none of its
     evidence/test keywords appear in the test-only corpus → **implemented
     but untested**.

The rule table starts with eight checks: diff preview, confirmation prompts,
revert/restore/undo (test-coverage required), encryption, atomic writes,
smart project-file search, backup safety, and bare "we have a test suite"
claims. It's deliberately coarse substring matching — in the same spirit as
ferret's hunk-level heuristic rules — trading precision for zero-setup
detection. Add rules to `CLAIM_RULES` in `src/rules.rs` as new claim/evidence
pairs come up.

## Development

```bash
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```
