# TraceQL compatibility

Target: the TraceQL syntax of Grafana Tempo v2.6. The parser accepts the subset
below and rejects everything else with a typed error; it never guesses at an
unrecognised construct.

Rejections carry a category, and the category is part of the contract:

| Category | Meaning |
| --- | --- |
| `ParseError::Unsupported` | Valid TraceQL this parser does not implement |
| `ParseError::Syntax` | Malformed input |
| `ParseError::Validation` | Well formed, but the value type or enum keyword does not match the operator or field |

The split matters. A caller writing a structural query needs "not supported",
not "your query is broken", so a deferred construct must never be reported as
malformed. `tests/acceptance_matrix.rs` is the parity table for this, readable
end to end.

## Supported

| Area | Constructs |
| --- | --- |
| Spanset selector | `{}` and a single `{ <field-expr> }` block |
| Scoped attributes | `resource.<name>`, `span.<name>` |
| Intrinsics | `span:name`, `trace:id`, `span:duration`, `trace:duration`, `span:status`, `span:kind` |
| Bare aliases | `name`, `duration`, `status`, `kind` — the unscoped spellings Grafana's Search builder emits, treated as the span-scoped intrinsics |
| Comparisons | `=`, `!=`, `=~`, `!~`, `<`, `<=`, `>`, `>=` |
| Boolean combinations | `&&`, `||` and parentheses, with `&&` binding tighter |
| Literals | double-quoted and backtick strings, integers, decimals, durations (`100ms`, `2.5s`), booleans, and the `status`/`kind` enum keywords quoted or bare (`status = error`, `status = "ok"`) |

Integer literals are kept exact rather than being routed through a float, so a
64-bit id compared with `=` does not lose precision.

## Regex patterns are not compiled here

`=~` and `!~` store their pattern verbatim. The parser checks only that the
operand is a string; it does not compile the pattern or check it against RE2.
Lookaround, backreferences and other non-RE2 constructs therefore *parse*, and
the caller's regex engine rejects them when it compiles the pattern. This keeps
regex-engine knowledge out of a syntactic parser, at the cost of one extra
failure point for callers that accept untrusted patterns.

## Not supported

| Construct | Result |
| --- | --- |
| Structural operators: `>>`, `<<`, `>`, `<`, `~` and their `!`/`&` forms | `Unsupported` |
| Spanset combining across blocks: `{…} && {…}`, `{…} \|\| {…}` | `Unsupported` |
| Nested spanset selectors: `{ { … } }` | `Unsupported` |
| TraceQL metrics pipes: `{…} \| rate()` | `Unsupported` |
| Query hints: `{…} with (most_recent=true)` | `Unsupported` |
| `event.` and `link.` scopes | `Unsupported` |
| Unscoped attributes: `{ service = "x" }` | `Unsupported` |
| Unknown intrinsics: `{ span:foo = "x" }` | `Unsupported` |

A field expression may nest at most `MAX_NESTING_DEPTH` (128) levels. A
parenthesised group costs one level; each `&&`/`||` operator in a chain also
costs one, because the resulting left-deep tree is as deep as the chain is long
and *dropping* it recurses once per node. Beyond the limit the query is a
`Syntax` error, so hostile input cannot exhaust the stack — an overflow would
abort the process rather than fail the query.

## Test corpus

`tests/corpus/parser_cases.jsonl` holds the compatibility cases, one JSON object
per line: `id`, `query`, `expect` (`accept`, `unsupported`, `syntax` or
`validation`), `source`, `tags` and an optional `error_contains` substring.
Every case must hold; there is no skip or expected-failure list, so a regression
cannot be hidden by reclassifying it. `tests/corpus/MANIFEST.sha256` records the
corpus checksum.

`source` records why a case exists: `tempo_docs` cases exercise syntax described
in public TraceQL documentation, `grafana_search` cases have the shape of
queries the Grafana Search builder produces, and `packdb_regression` cases pin
fixed defects.

## Provenance

Grafana Tempo is licensed AGPL-3.0; this parser is Apache-2.0. It is a
clean-room behavioural implementation: no source file and no test fixture is
copied or mechanically ported from Tempo. The corpus was authored from public
TraceQL documentation and from observed Grafana Search query-builder output.

This was checked, not assumed. Every corpus query longer than 12 characters was
searched for, whitespace-normalised, in Tempo's `pkg/traceql` tree at the
documented commit: none occurs. The handful of queries that do appear in
Tempo's *documentation* are canonical one-line examples of the syntax itself,
such as `{ status = error }`, and are recorded as `tempo_docs`.
