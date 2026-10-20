# packdb-traceql-parser

A parser for [TraceQL](https://grafana.com/docs/tempo/latest/traceql/), the
trace query language used by Grafana Tempo. It turns a query string into a typed
syntax tree and returns structured errors for malformed or unsupported input. It
does not evaluate queries, and its only runtime dependency is `thiserror`.

The parser was written for PackDB's Tempo-compatible search API and is
maintained as a standalone library.

## Usage

Use a tagged source release of this repository. The crate is not published to
crates.io:

```toml
[dependencies]
packdb-traceql-parser = { git = "https://github.com/packdb-oss/traceql-parser", tag = "v0.1.1" }
```

```rust
use packdb_traceql_parser::{ParseError, parse};

match parse(r#"{ resource.service.name = "api" && span:duration > 100ms }"#) {
    Ok(spanset) => println!("{:?}", spanset.filter),
    Err(ParseError::Unsupported(what)) => eprintln!("not supported: {what}"),
    Err(e) => eprintln!("{e}"),
}
```

`parse` returns:

- `ParseError::Unsupported` for valid TraceQL outside the supported subset —
  structural operators, metrics pipes, hints, `event.`/`link.` scopes — so a
  caller can tell "not implemented" from "malformed";
- `ParseError::Syntax` for malformed input, including queries nested deeper than
  `MAX_NESTING_DEPTH`;
- `ParseError::Validation` for input that is well formed but whose value type or
  enum keyword does not match the operator or field.

## Supported TraceQL

[COMPATIBILITY.md](COMPATIBILITY.md) lists supported and unsupported constructs
and describes the test corpus. In summary: the `{ … }` spanset selector;
`resource.`/`span.` scoped attributes; the `span:name`, `trace:id`,
`span:duration`, `trace:duration`, `span:status` and `span:kind` intrinsics plus
the bare aliases Grafana's Search builder emits; the eight comparison operators;
`&&`, `||` and parentheses; and string, number, duration, boolean and
`status`/`kind` enum literals.

Regex patterns for `=~`/`!~` are stored verbatim and not compiled, so RE2
validation is the caller's responsibility.

## Requirements

Rust 1.94.1 or later (edition 2024). The crate uses no `unsafe` code and has no
platform-specific code.

## Development

```sh
scripts/oss-ci.sh checks         # format, lint, tests, docs, policy and package checks
scripts/oss-ci.sh compatibility  # corpus, acceptance matrix and adversarial inputs
scripts/oss-ci.sh coverage       # per-file line coverage (needs cargo-llvm-cov)
```

Set `OSS_CHECK_SCOPE=full` when running locally. See
[CONTRIBUTING.md](CONTRIBUTING.md).

## License

Apache License 2.0; see [LICENSE](LICENSE). Contributions are accepted under the
same license.
