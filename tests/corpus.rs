// Copyright 2026 PackDb Contributors
//
// Licensed under the Apache License, Version 2.0

//! TraceQL compatibility corpus.
//!
//! Each line of `tests/corpus/parser_cases.jsonl` names a query and the
//! parse-outcome category a TraceQL implementation compatible with Grafana
//! Tempo must produce for it. Every case must hold: there is no skip or
//! expected-failure list, so a regression cannot be hidden by reclassifying it.
//! `COMPATIBILITY.md` describes the case format and provenance. Set
//! `TRACEQL_TEST_FILTER` to run the cases whose id or tags contain that
//! substring.
//!
//! The outcome is a category, not accept/reject, because the distinction is
//! load-bearing: valid TraceQL this parser defers (`unsupported`) must never be
//! reported as malformed (`syntax`). A user writing a structural query needs
//! "not supported", not "your query is broken", and collapsing the two would
//! hide exactly the compatibility gaps this corpus exists to find.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashSet;
use std::path::Path;

use packdb_traceql_parser::{ParseError, parse};

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    query: String,
    expect: Expect,
    source: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    error_contains: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum Expect {
    /// In the supported subset: parses to a `Spanset`.
    Accept,
    /// Valid TraceQL outside the subset, rejected with a typed error.
    Unsupported,
    /// Malformed input.
    Syntax,
    /// Well formed, but the value type or enum keyword does not match.
    Validation,
}

impl Expect {
    /// The outcome the parser actually produced, with its message.
    fn observed(query: &str) -> (Self, Option<String>) {
        match parse(query) {
            Ok(_) => (Expect::Accept, None),
            Err(e @ ParseError::Unsupported(_)) => (Expect::Unsupported, Some(e.to_string())),
            Err(e @ ParseError::Syntax(_)) => (Expect::Syntax, Some(e.to_string())),
            Err(e @ ParseError::Validation(_)) => (Expect::Validation, Some(e.to_string())),
        }
    }
}

fn load_cases() -> Vec<Case> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/parser_cases.jsonl");
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read corpus {}: {e}", path.display()));
    content
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(i, line)| {
            serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("corpus line {}: {e}\n{line}", i + 1))
        })
        .collect()
}

fn check(case: &Case) -> Result<(), String> {
    let (observed, message) = Expect::observed(&case.query);
    if observed != case.expect {
        return Err(match message {
            Some(m) => format!("expected {:?}, got {observed:?}: {m}", case.expect),
            None => format!("expected {:?}, got {observed:?}", case.expect),
        });
    }
    match (&case.error_contains, &message) {
        (Some(needle), Some(m)) if !m.contains(needle.as_str()) => {
            Err(format!("error `{m}` does not contain `{needle}`"))
        }
        _ => Ok(()),
    }
}

#[test]
fn corpus_is_well_formed() {
    let cases = load_cases();
    assert!(!cases.is_empty(), "corpus is empty");
    let mut ids = HashSet::new();
    for case in &cases {
        assert!(
            ids.insert(case.id.as_str()),
            "duplicate case id `{}`",
            case.id
        );
        assert!(
            matches!(
                case.source.as_str(),
                "tempo_docs" | "grafana_search" | "packdb_regression"
            ),
            "case `{}` has unknown source `{}`",
            case.id,
            case.source
        );
        if case.error_contains.is_some() {
            assert!(
                case.expect != Expect::Accept,
                "case `{}` sets error_contains on an accept case",
                case.id
            );
        }
    }
}

/// Every outcome category must be represented. A corpus that had drifted to
/// accept-only cases would still pass `corpus_cases_hold` while testing none of
/// the rejection paths this parser exists to keep distinct.
#[test]
fn corpus_covers_every_outcome_category() {
    let cases = load_cases();
    for expected in [
        Expect::Accept,
        Expect::Unsupported,
        Expect::Syntax,
        Expect::Validation,
    ] {
        assert!(
            cases.iter().any(|c| c.expect == expected),
            "no corpus case expects {expected:?}"
        );
    }
}

#[test]
fn corpus_cases_hold() {
    let filter = std::env::var("TRACEQL_TEST_FILTER").ok();
    let cases = load_cases();
    let mut run = 0usize;
    let mut failures = Vec::new();
    for case in &cases {
        if let Some(f) = &filter
            && !case.id.contains(f.as_str())
            && !case.tags.iter().any(|t| t.contains(f.as_str()))
        {
            continue;
        }
        run += 1;
        if let Err(msg) = check(case) {
            failures.push(format!("{}: {msg}\n    query: {}", case.id, case.query));
        }
    }
    println!(
        "corpus: {run} run, {} failed, {} total",
        failures.len(),
        cases.len()
    );
    assert!(run > 0, "filter {filter:?} selected no cases");
    assert!(
        failures.is_empty(),
        "{} case(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
