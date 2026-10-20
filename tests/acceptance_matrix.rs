// Copyright 2026 PackDB Contributors
//
// Licensed under the Apache License, Version 2.0

//! TraceQL parser acceptance/rejection matrix.
//!
//! This is the committed parity artifact for the supported search subset: a
//! data-driven table pairing each fixture query with the parse outcome category
//! the parser must produce. It exercises only the public [`parse`] API and the
//! public [`ParseError`] categories, so it doubles as the compatibility
//! contract reviewers can read end to end.
//!
//! Categories:
//! - [`Outcome::Accept`]: in the supported subset; parses to a `Spanset`. This
//!   includes any `=~`/`!~` regex pattern, which the parser stores verbatim
//!   without compiling — RE2 validation is the caller's concern.
//! - [`Outcome::Unsupported`]: valid TraceQL outside the subset, rejected with
//!   a typed error so callers never silently mis-parse it.
//! - [`Outcome::Syntax`]: malformed input.
//! - [`Outcome::Validation`]: well-formed but the value type / enum keyword does
//!   not match the operator or field.
//!
//! Fixtures are authored from public TraceQL documentation and Grafana Search
//! query-builder output; none are copied from upstream Tempo source.

use packdb_traceql_parser::{ParseError, parse};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Accept,
    Unsupported,
    Syntax,
    Validation,
}

impl Outcome {
    /// Classify the parser's result for a fixture.
    fn of(query: &str) -> Self {
        match parse(query) {
            Ok(_) => Outcome::Accept,
            Err(ParseError::Unsupported(_)) => Outcome::Unsupported,
            Err(ParseError::Syntax(_)) => Outcome::Syntax,
            Err(ParseError::Validation(_)) => Outcome::Validation,
        }
    }
}

/// The acceptance/rejection matrix. Grouped by construct for readability;
/// the grouping is documentation only — every row is asserted identically.
const MATRIX: &[(&str, Outcome)] = &[
    // --- Empty spanset selector --------------------------------------------
    ("{}", Outcome::Accept),
    ("{   }", Outcome::Accept),
    // --- Service / resource attribute filters ------------------------------
    (r#"{ resource.service.name = "frontend" }"#, Outcome::Accept),
    (r#"{ resource.service.name != "legacy" }"#, Outcome::Accept),
    (r#"{ resource.region = "us-east-1" }"#, Outcome::Accept),
    // --- Span attribute filters --------------------------------------------
    ("{ span.http.status_code = 500 }", Outcome::Accept),
    ("{ span.http.status_code >= 400 }", Outcome::Accept),
    (r#"{ span.http.method =~ "GET|POST" }"#, Outcome::Accept),
    (r#"{ span.http.target !~ "/health.*" }"#, Outcome::Accept),
    ("{ span.ok = true }", Outcome::Accept),
    ("{ span.retries = 3 }", Outcome::Accept),
    ("{ span.ratio = 0.5 }", Outcome::Accept),
    // --- Span name (intrinsic + Grafana bare alias) ------------------------
    (r#"{ span:name = "HTTP GET" }"#, Outcome::Accept),
    (r#"{ name = "POST /api/orders" }"#, Outcome::Accept),
    (r#"{ span:name =~ "GET /.*" }"#, Outcome::Accept),
    // --- Trace id ----------------------------------------------------------
    (
        r#"{ trace:id = "0af7651916cd43dd8448eb211c80319c" }"#,
        Outcome::Accept,
    ),
    // --- Duration (span + trace + bare alias) ------------------------------
    ("{ span:duration > 100ms }", Outcome::Accept),
    ("{ trace:duration >= 1s }", Outcome::Accept),
    ("{ duration > 500ms }", Outcome::Accept),
    ("{ span:duration <= 2.5s }", Outcome::Accept),
    // --- Status enum (intrinsic + alias, quoted + unquoted) ----------------
    ("{ span:status = error }", Outcome::Accept),
    ("{ status = error }", Outcome::Accept),
    (r#"{ status = "ok" }"#, Outcome::Accept),
    ("{ status != unset }", Outcome::Accept),
    // --- Kind enum (intrinsic + alias) -------------------------------------
    ("{ span:kind = server }", Outcome::Accept),
    ("{ kind = client }", Outcome::Accept),
    ("{ kind != internal }", Outcome::Accept),
    // --- Boolean combinations and precedence -------------------------------
    (
        "{ span.http.status_code >= 400 && span.http.status_code < 600 }",
        Outcome::Accept,
    ),
    (
        r#"{ resource.service.name = "api" || resource.service.name = "web" }"#,
        Outcome::Accept,
    ),
    (
        r#"{ (span.a = "1" || span.b = "2") && span.c = "3" }"#,
        Outcome::Accept,
    ),
    // --- Representative Grafana Search builder output (compact, no spaces) --
    (
        r#"{ resource.service.name="frontend" && name="GET /api" && status=error }"#,
        Outcome::Accept,
    ),
    (
        r#"{ resource.service.name = "checkout" && duration > 250ms }"#,
        Outcome::Accept,
    ),
    // --- Regex patterns parse verbatim; RE2 validation is the caller's job -
    // The syntactic parser stores the pattern as a string and does not compile
    // it, so both RE2-valid groups and non-RE2 constructs (lookaround,
    // backreferences) parse here. The caller's regex engine rejects the latter
    // when it compiles the pattern — never as a parser error.
    (r#"{ span.name =~ "(?:GET|POST) /.*" }"#, Outcome::Accept),
    (r#"{ span.name =~ "(?i)get" }"#, Outcome::Accept),
    (r#"{ span.name =~ "(?P<verb>GET|POST)" }"#, Outcome::Accept),
    (r#"{ span.name =~ "[(?=]" }"#, Outcome::Accept), // char class, not lookahead
    (r#"{ span.name =~ "foo(?=bar)" }"#, Outcome::Accept), // lookahead, deferred
    (r#"{ span.name !~ "(?<=foo)bar" }"#, Outcome::Accept), // lookbehind, deferred
    (r"{ span.name =~ `(foo)\1` }", Outcome::Accept), // backreference, deferred
    // --- Unsupported: deferred TraceQL constructs -------------------------
    (
        r#"{ span.a = "1" } >> { span.b = "2" }"#,
        Outcome::Unsupported,
    ),
    (r#"{ span.a = "1" } | rate()"#, Outcome::Unsupported),
    (
        r#"{ span.a = "1" } with (most_recent=true)"#,
        Outcome::Unsupported,
    ),
    (r#"{ event.name = "exception" }"#, Outcome::Unsupported),
    (r#"{ link.trace_id = "abc" }"#, Outcome::Unsupported),
    (
        r#"{ span.a = "1" } && { span.b = "2" }"#,
        Outcome::Unsupported,
    ),
    ("{ { span.a = 1 } }", Outcome::Unsupported),
    (r#"{ span:foo = "x" }"#, Outcome::Unsupported),
    (r#"{ service = "frontend" }"#, Outcome::Unsupported),
    // --- Syntax: malformed input -------------------------------------------
    ("", Outcome::Syntax),
    ("{", Outcome::Syntax),
    (r#"resource.service.name = "api""#, Outcome::Syntax),
    (r#"{ span.name == "x" }"#, Outcome::Syntax),
    ("{ span:duration > }", Outcome::Syntax),
    (r#"{ = "x" }"#, Outcome::Syntax),
    (r#"{ span. = "x" }"#, Outcome::Syntax),
    ("{ status = }", Outcome::Syntax),
    ("{ kind = }", Outcome::Syntax),
    // --- Validation: operator / value-type / enum mismatch -----------------
    ("{ span.code =~ 500 }", Outcome::Validation),
    (r#"{ span.name > "x" }"#, Outcome::Validation),
    ("{ status = bogus }", Outcome::Validation),
    ("{ kind = wormhole }", Outcome::Validation),
    ("{ status > error }", Outcome::Validation),
];

#[test]
fn acceptance_rejection_matrix() {
    let mut failures = Vec::new();
    for &(query, expected) in MATRIX {
        let actual = Outcome::of(query);
        if actual != expected {
            failures.push(format!(
                "  {query:?}: expected {expected:?}, got {actual:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "acceptance/rejection matrix mismatches:\n{}",
        failures.join("\n")
    );
}

#[test]
fn matrix_covers_every_outcome_category() {
    for category in [
        Outcome::Accept,
        Outcome::Unsupported,
        Outcome::Syntax,
        Outcome::Validation,
    ] {
        assert!(
            MATRIX.iter().any(|&(_, o)| o == category),
            "matrix has no fixture for {category:?}"
        );
    }
}
