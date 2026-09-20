// Copyright 2026 PackDb Contributors
//
// Licensed under the Apache License, Version 2.0

//! Malformed and hostile inputs must produce `Ok` or `Err`, never a panic,
//! hang or stack overflow. The inputs are deterministic so a failure
//! reproduces exactly; longer randomized fuzzing runs separately.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use packdb_traceql_parser::parse;

fn corpus_queries() -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/parser_cases.jsonl");
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: serde_json::Value = serde_json::from_str(l).unwrap();
            v["query"].as_str().unwrap().to_owned()
        })
        .collect()
}

/// Every prefix of every corpus query, cut at each character boundary.
/// Truncated input exercises the end-of-input path in every production — the
/// one a quote-aware scanner is most likely to run off the end of.
#[test]
fn truncated_queries_do_not_panic() {
    for q in corpus_queries() {
        for (i, _) in q.char_indices() {
            let _ = parse(&q[..i]);
        }
    }
}

/// Byte-level mutations of corpus queries: replace, delete or insert a byte
/// drawn from TraceQL's structural characters. Splitting a multi-byte
/// character is skipped because `parse` takes `&str`.
#[test]
fn mutated_queries_do_not_panic() {
    const ALPHABET: &[u8] = b"{}()[]|=!~\"`\\,<>&:.+-*/% \t\n0123456789abyzAZ_#";
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    // Longer hardening runs raise this through the environment.
    let rounds: usize = std::env::var("TRACEQL_ADVERSARIAL_ROUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(64);
    for q in corpus_queries() {
        for _ in 0..rounds {
            let mut bytes = q.clone().into_bytes();
            let pos = (next() as usize) % (bytes.len() + 1);
            let byte = ALPHABET[(next() as usize) % ALPHABET.len()];
            match next() % 3 {
                0 if pos < bytes.len() => bytes[pos] = byte,
                1 if pos < bytes.len() => {
                    bytes.remove(pos);
                }
                _ => bytes.insert(pos, byte),
            }
            if let Ok(s) = String::from_utf8(bytes) {
                let _ = parse(&s);
            }
        }
    }
}

#[test]
fn non_ascii_and_control_input_does_not_panic() {
    for q in [
        "",
        " ",
        "\0",
        "{",
        "}",
        "{}",
        "{ span.name = \"\u{1F600}\" }",
        "{ resource.service.name = \"é\" }",
        "{ span.\u{00e9} = 1 }",
        "{ span:duration > 5\u{00b5}s }",
        "{ span.a = \"\\",
        "{ span.a = `x` }",
        "{ span.a = \"x\" \u{2028}&& span.b = 1 }",
        "{ span.a =~ \"(\" }",
        "{ trace:id = \"\u{0}\" }",
    ] {
        let _ = parse(q);
    }
}

/// Deeply nested input must be rejected or parsed without exhausting the
/// stack. Run on a thread with a deliberately small stack so a recursion bound
/// that only holds on a large server thread still fails here. A stack overflow
/// aborts the process, so this is a denial-of-service bound, not a nicety.
#[test]
fn deep_nesting_does_not_overflow_the_stack() {
    let handle = std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(|| {
            for depth in [64usize, 1_000, 10_000, 100_000] {
                let leaf = "span.a = 1";
                let parens = format!("{{ {}{leaf}{} }}", "(".repeat(depth), ")".repeat(depth));
                let _ = parse(&parens);
                let ands = format!("{{ {leaf}{} }}", format!(" && {leaf}").repeat(depth));
                let _ = parse(&ands);
                let ors = format!("{{ {leaf}{} }}", format!(" || {leaf}").repeat(depth));
                let _ = parse(&ors);
                let mixed = format!(
                    "{{ {leaf}{} }}",
                    format!(" && {leaf} || {leaf}").repeat(depth)
                );
                let _ = parse(&mixed);
                // Unterminated nesting: the scanner must not run away either.
                let open = format!("{{ {}{leaf}", "(".repeat(depth));
                let _ = parse(&open);
            }
        })
        .unwrap();
    handle
        .join()
        .expect("parser overflowed or panicked on nested input");
}
