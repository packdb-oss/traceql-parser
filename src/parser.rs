// Copyright 2026 PackDb Contributors
//
// Licensed under the Apache License, Version 2.0

//! TraceQL parser implementation.
//!
//! Recursive-descent / precedence-climbing parser for the Tempo-compatible
//! search subset (see COMPATIBILITY.md):
//!
//! - Empty spanset selector `{}` and single `{ <field-expr> }` blocks
//! - Scoped attributes (`resource.<name>`, `span.<name>`) and intrinsics
//!   (`span:name`, `trace:id`, `span:duration`, `trace:duration`, `span:status`,
//!   `span:kind`), plus the bare aliases Grafana's Search builder emits
//!   (`name`, `duration`, `status`, `kind`)
//! - Comparisons `= != =~ !~ > >= < <=`
//! - Boolean combinations `&&`, `||`, parentheses
//! - String, number, duration, boolean, and status/kind enum literals
//!
//! Out-of-subset TraceQL (structural operators, metrics pipes, hints,
//! event/link scope, spanset-combining across `{...}` blocks) is rejected with
//! [`ParseError::Unsupported`] so callers never silently mis-parse it.

use std::cell::Cell;
use std::time::Duration;

use crate::ast::syntax::*;
use crate::error::ParseError;

/// Maximum nesting depth accepted in a single field expression.
///
/// Parsing, dropping and every consumer that walks the tree recurse once per
/// level. Without a bound, a short query such as 20,000 nested parentheses
/// exhausts the thread stack, and a stack overflow aborts the whole process
/// rather than failing the query. Handwritten and Grafana-generated queries
/// stay far below this limit.
///
/// Kept equal to `logql-parser`'s limit so both query surfaces refuse the same
/// shapes.
pub const MAX_NESTING_DEPTH: usize = 128;

thread_local! {
    static NESTING_DEPTH: Cell<usize> = const { Cell::new(0) };
}

/// Holds `levels` units of the per-thread nesting budget until dropped, so
/// every early return releases what its frame reserved.
struct NestingGuard {
    levels: usize,
}

impl NestingGuard {
    fn enter() -> Result<Self, ParseError> {
        let mut guard = Self { levels: 0 };
        guard.descend()?;
        Ok(guard)
    }

    /// Reserve one more level for this frame: one recursive step, or one more
    /// operator folded into a left-deep chain held by this frame.
    fn descend(&mut self) -> Result<(), ParseError> {
        let depth = NESTING_DEPTH.with(Cell::get);
        if depth >= MAX_NESTING_DEPTH {
            return Err(ParseError::Syntax(format!(
                "query nesting exceeds the maximum depth of {MAX_NESTING_DEPTH}"
            )));
        }
        NESTING_DEPTH.with(|d| d.set(depth + 1));
        self.levels += 1;
        Ok(())
    }
}

impl Drop for NestingGuard {
    fn drop(&mut self) {
        let levels = self.levels;
        NESTING_DEPTH.with(|d| d.set(d.get().saturating_sub(levels)));
    }
}

/// Parse a TraceQL query string into a syntax-level [`Spanset`].
///
/// # Errors
///
/// Returns [`ParseError::Syntax`] for malformed input,
/// [`ParseError::Unsupported`] for valid TraceQL outside the supported subset,
/// and [`ParseError::Validation`] for a comparison whose value type does not
/// match its operator.
pub fn parse(input: &str) -> Result<Spanset, ParseError> {
    let trimmed = input.trim();

    if trimmed.is_empty() {
        return Err(ParseError::Syntax("empty query".to_string()));
    }

    if !trimmed.starts_with('{') {
        return Err(ParseError::Syntax(format!(
            "expected '{{' to start a spanset selector, got: '{}'",
            preview(trimmed)
        )));
    }

    let (body, after) = extract_spanset(trimmed)?;

    let after = after.trim();
    if !after.is_empty() {
        return Err(classify_trailing(after));
    }

    let inner = body.trim();
    if inner.is_empty() {
        return Ok(Spanset { filter: None });
    }

    let (expr, rest) = parse_field_expr(inner, 0)?;
    let rest = rest.trim();
    if !rest.is_empty() {
        return Err(classify_trailing(rest));
    }

    Ok(Spanset { filter: Some(expr) })
}

/// Extract the body of the leading `{ ... }` block.
///
/// `input` must start with `{`. Returns `(body, remaining)` where `body` is the
/// text between the braces (exclusive) and `remaining` is everything after the
/// closing `}`. The scan is quote-aware so `}` inside a string is ignored.
fn extract_spanset(input: &str) -> Result<(&str, &str), ParseError> {
    let after_open = input.get(1..).unwrap_or("");
    let mut scanner = QuoteScanner::new();
    for (i, ch) in after_open.char_indices() {
        if scanner.advance(ch) {
            continue;
        }
        match ch {
            '}' => {
                let body = after_open.get(..i).unwrap_or("");
                let rest = after_open.get(i + 1..).unwrap_or("");
                return Ok((body, rest));
            }
            // A nested `{` would be a spanset-combining construct, which the
            // subset does not support. Surface it explicitly.
            '{' => {
                return Err(ParseError::Unsupported(
                    "nested spanset selectors are not supported".to_string(),
                ));
            }
            _ => {}
        }
    }
    Err(ParseError::Syntax(
        "unterminated spanset selector: missing '}'".to_string(),
    ))
}

/// Classify text that appears where the parser expected the end of input (or a
/// `&&`/`||` operand). Known TraceQL-but-unsupported constructs map to
/// [`ParseError::Unsupported`]; anything else is a [`ParseError::Syntax`] error.
fn classify_trailing(rest: &str) -> ParseError {
    if rest.starts_with("&&") || rest.starts_with("||") {
        return ParseError::Unsupported(
            "spanset-combining across '{...}' blocks is not supported".to_string(),
        );
    }
    // Structural operators: descendant/ancestor (`>>`/`<<`), child/parent
    // (`>`/`<`), sibling (`~`), and their negated (`!`) and union (`&`) forms.
    // All are valid TraceQL outside our subset, so they must classify as
    // Unsupported rather than falling through to "unexpected trailing input" —
    // a caller writing a structural query needs "not supported", not
    // "malformed".
    //
    // Ordering is significant: each operator must be tested before any operator
    // that is a prefix of it, so `>>` is not reported as `>`.
    for op in [
        "!>>", "!<<", "&>>", "&<<", // 3-char: negated/union descendant/ancestor
        ">>", "<<", // 2-char: descendant/ancestor
        "!>", "!<", "!~", "&>", "&<", "&~", // 2-char: negated/union single forms
        ">", "<", "~", // 1-char: child/parent/sibling
    ] {
        if rest.starts_with(op) {
            return ParseError::Unsupported(format!("structural operator '{op}' is not supported"));
        }
    }
    if rest.starts_with('|') {
        return ParseError::Unsupported("TraceQL metrics ('| ...') are not supported".to_string());
    }
    if let Some(after) = rest.strip_prefix("with")
        && !after.starts_with(|c: char| c.is_alphanumeric() || c == '_')
    {
        return ParseError::Unsupported(
            "TraceQL hints ('with (...)') are not supported".to_string(),
        );
    }
    ParseError::Syntax(format!("unexpected trailing input: '{}'", preview(rest)))
}

// ===========================================================================
// Field expression: precedence climbing over && and ||
// ===========================================================================

/// Boolean operator precedence (higher binds tighter): `||`(1) < `&&`(2).
fn boolop_precedence(op: BoolOp) -> u8 {
    match op {
        BoolOp::Or => 1,
        BoolOp::And => 2,
    }
}

/// Try to read a `&&` or `||` operator at the start of `input`.
fn try_parse_boolop(input: &str) -> Option<(BoolOp, &str)> {
    let input = input.trim();
    if let Some(r) = input.strip_prefix("&&") {
        return Some((BoolOp::And, r.trim()));
    }
    if let Some(r) = input.strip_prefix("||") {
        return Some((BoolOp::Or, r.trim()));
    }
    None
}

/// Parse a field expression with precedence climbing. Returns `(expr, rest)`.
fn parse_field_expr(input: &str, min_prec: u8) -> Result<(FieldExpr, &str), ParseError> {
    let mut nesting = NestingGuard::enter()?;
    let (mut lhs, mut rest) = parse_primary(input)?;

    loop {
        let trimmed = rest.trim();
        let Some((op, after_op)) = try_parse_boolop(trimmed) else {
            break;
        };
        let prec = boolop_precedence(op);
        if prec < min_prec {
            break;
        }
        // Each fold deepens the left-deep tree this frame owns by one level,
        // so it costs one unit of budget even though it does not add a frame.
        // Dropping that tree recurses once per level.
        nesting.descend()?;
        // Left-associative: parse the RHS at one precedence level higher.
        let (rhs, after_rhs) = parse_field_expr(after_op, prec + 1)?;
        lhs = FieldExpr::Binary {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        };
        rest = after_rhs;
    }

    Ok((lhs, rest))
}

/// Parse a primary field expression: a parenthesized expression or a leaf
/// comparison.
fn parse_primary(input: &str) -> Result<(FieldExpr, &str), ParseError> {
    let _nesting = NestingGuard::enter()?;
    let input = input.trim();

    if input.starts_with('(') {
        let after_open = input.get(1..).unwrap_or("").trim();
        let (expr, rest) = parse_field_expr(after_open, 0)?;
        let rest = rest.trim();
        let after_close = rest.strip_prefix(')').ok_or_else(|| {
            ParseError::Syntax("missing closing ')' for parenthesized expression".to_string())
        })?;
        return Ok((expr, after_close));
    }

    let (comparison, rest) = parse_comparison(input)?;
    Ok((FieldExpr::Comparison(comparison), rest))
}

// ===========================================================================
// Comparison: <field> <op> <value>
// ===========================================================================

fn parse_comparison(input: &str) -> Result<(Comparison, &str), ParseError> {
    let (field, rest) = parse_field(input)?;
    let rest = rest.trim();
    let (op, after_op) = parse_comparison_op(rest)?;
    let (value, after_value) = parse_value(after_op.trim(), &field)?;

    validate_comparison(op, &value)?;

    Ok((Comparison { field, op, value }, after_value))
}

/// Parse the left-hand field: a scoped attribute or an intrinsic.
fn parse_field(input: &str) -> Result<(Field, &str), ParseError> {
    let trimmed = input.trim();
    // A field token is the run of identifier/scope characters. Operators and
    // whitespace terminate it.
    let end = trimmed
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == ':'))
        .unwrap_or(trimmed.len());
    let token = trimmed.get(..end).unwrap_or("");
    let rest = trimmed.get(end..).unwrap_or("");

    if token.is_empty() {
        return Err(ParseError::Syntax(format!(
            "expected an attribute or intrinsic, got: '{}'",
            preview(trimmed)
        )));
    }

    let field = classify_field(token)?;
    Ok((field, rest))
}

fn classify_field(token: &str) -> Result<Field, ParseError> {
    // Scope-qualified intrinsics use a `:` separator: `span:name`, `trace:id`.
    if let Some((scope, name)) = token.split_once(':') {
        return match (scope, name) {
            ("span", "name") => Ok(Field::Intrinsic(Intrinsic::SpanName)),
            ("span", "duration") => Ok(Field::Intrinsic(Intrinsic::SpanDuration)),
            ("span", "status") => Ok(Field::Intrinsic(Intrinsic::SpanStatus)),
            ("span", "kind") => Ok(Field::Intrinsic(Intrinsic::SpanKind)),
            ("trace", "id") => Ok(Field::Intrinsic(Intrinsic::TraceId)),
            ("trace", "duration") => Ok(Field::Intrinsic(Intrinsic::TraceDuration)),
            _ => Err(ParseError::Unsupported(format!(
                "intrinsic '{token}' is not in the supported subset"
            ))),
        };
    }

    // Scoped attributes use a `.` separator: `resource.<name>`, `span.<name>`.
    if let Some(name) = token.strip_prefix("resource.") {
        return attribute(AttributeScope::Resource, name);
    }
    if let Some(name) = token.strip_prefix("span.") {
        return attribute(AttributeScope::Span, name);
    }
    if token.starts_with("event.") || token.starts_with("link.") {
        return Err(ParseError::Unsupported(format!(
            "event/link scope ('{token}') is not in the supported subset"
        )));
    }

    // Bare intrinsics emitted by Grafana's Search query builder, accepted as
    // aliases of the span-scoped intrinsics. Note these have no
    // `.`/`:` separator, so they cannot collide with scoped attributes above.
    match token {
        "name" => return Ok(Field::Intrinsic(Intrinsic::SpanName)),
        "duration" => return Ok(Field::Intrinsic(Intrinsic::SpanDuration)),
        "status" => return Ok(Field::Intrinsic(Intrinsic::SpanStatus)),
        "kind" => return Ok(Field::Intrinsic(Intrinsic::SpanKind)),
        _ => {}
    }

    Err(ParseError::Unsupported(format!(
        "unscoped or unknown field '{token}'; use a 'resource.'/'span.' attribute or a supported intrinsic"
    )))
}

fn attribute(scope: AttributeScope, name: &str) -> Result<Field, ParseError> {
    if name.is_empty() {
        return Err(ParseError::Syntax(
            "empty attribute name after scope".to_string(),
        ));
    }
    Ok(Field::Attribute {
        scope,
        name: name.to_string(),
    })
}

/// Parse a comparison operator at the start of `input`. Returns `(op, rest)`.
fn parse_comparison_op(input: &str) -> Result<(ComparisonOp, &str), ParseError> {
    // Two-character operators first.
    if let Some(r) = input.strip_prefix("=~") {
        return Ok((ComparisonOp::Re, r));
    }
    if let Some(r) = input.strip_prefix("!~") {
        return Ok((ComparisonOp::Nre, r));
    }
    if let Some(r) = input.strip_prefix("!=") {
        return Ok((ComparisonOp::Ne, r));
    }
    if let Some(r) = input.strip_prefix(">=") {
        return Ok((ComparisonOp::Gte, r));
    }
    if let Some(r) = input.strip_prefix("<=") {
        return Ok((ComparisonOp::Lte, r));
    }
    // `==` is a common mistake — TraceQL uses `=`.
    if input.starts_with("==") {
        return Err(ParseError::Syntax(
            "'==' is not a TraceQL comparison operator; use '='".to_string(),
        ));
    }
    // Structural operators share a prefix with `>`/`<`; reject before them.
    if input.starts_with(">>") {
        return Err(ParseError::Unsupported(
            "structural operator '>>' is not supported".to_string(),
        ));
    }
    if input.starts_with("<<") {
        return Err(ParseError::Unsupported(
            "structural operator '<<' is not supported".to_string(),
        ));
    }
    // Single-character operators.
    if let Some(r) = input.strip_prefix('=') {
        return Ok((ComparisonOp::Eq, r));
    }
    if let Some(r) = input.strip_prefix('>') {
        return Ok((ComparisonOp::Gt, r));
    }
    if let Some(r) = input.strip_prefix('<') {
        return Ok((ComparisonOp::Lt, r));
    }
    Err(ParseError::Syntax(format!(
        "expected a comparison operator, got: '{}'",
        preview(input)
    )))
}

/// Validate that a value's type is compatible with its comparison operator.
fn validate_comparison(op: ComparisonOp, value: &Value) -> Result<(), ParseError> {
    // The parser is purely syntactic: a `=~`/`!~` pattern is stored verbatim and
    // not compiled here. RE2 compatibility (rejecting lookaround, backreferences,
    // and other non-RE2 constructs) is left to the caller, which compiles the
    // pattern against its own regex engine; that keeps RE2-engine knowledge out
    // of this crate. We only check that the operand is a string.
    if op.is_regex() && !matches!(value, Value::String(_)) {
        return Err(ParseError::Validation(
            "regex operators ('=~', '!~') require a string value".to_string(),
        ));
    }
    if op.is_ordering()
        && !matches!(
            value,
            Value::Integer(_) | Value::Number(_) | Value::Duration(_)
        )
    {
        return Err(ParseError::Validation(
            "ordering operators ('<', '<=', '>', '>=') require a numeric or duration value"
                .to_string(),
        ));
    }
    if matches!(value, Value::Status(_) | Value::Kind(_))
        && !matches!(op, ComparisonOp::Eq | ComparisonOp::Ne)
    {
        return Err(ParseError::Validation(
            "status/kind enum values support only '=' and '!='".to_string(),
        ));
    }
    Ok(())
}

// ===========================================================================
// Value literals
// ===========================================================================

fn parse_value<'a>(input: &'a str, field: &Field) -> Result<(Value, &'a str), ParseError> {
    let input = input.trim();
    let first = input.chars().next().ok_or_else(|| {
        ParseError::Syntax("expected a value after comparison operator".to_string())
    })?;

    // The `status` and `span:kind` intrinsics take enum keywords. Grafana's
    // Search builder emits them unquoted (`status = error`); Tempo also accepts
    // quoted spellings. Resolve both forms against the enum and
    // reject unknown keywords with a typed Validation error rather than treating
    // them as free-text strings.
    if let Field::Intrinsic(intrinsic @ (Intrinsic::SpanStatus | Intrinsic::SpanKind)) = field {
        let (text, rest) = read_value_token(input, first)?;
        return resolve_enum_value(*intrinsic, &text, rest);
    }

    if first == '"' || first == '`' {
        let (s, rest) = parse_quoted_string(input, first)?;
        return Ok((Value::String(s), rest));
    }

    // Unquoted token: boolean, duration, or number. Terminates at whitespace,
    // a boolean operator, or a closing paren.
    let (token, rest) = read_bareword(input);

    if token == "true" {
        return Ok((Value::Bool(true), rest));
    }
    if token == "false" {
        return Ok((Value::Bool(false), rest));
    }

    if let Some(duration) = try_parse_duration(token) {
        return Ok((Value::Duration(duration), rest));
    }

    // Integral literals parse to i64 first so a comparison against a large
    // integer attribute stays exact (f64 rounds above 2^53). `parse::<i64>()`
    // rejects decimals/scientific/out-of-range, which fall through to f64.
    if let Ok(int) = token.parse::<i64>() {
        return Ok((Value::Integer(int), rest));
    }
    if let Ok(num) = token.parse::<f64>() {
        return Ok((Value::Number(num), rest));
    }

    Err(ParseError::Syntax(format!(
        "expected a string, number, duration, or boolean value, got: '{}'",
        preview(token)
    )))
}

/// Read the next value token as text, accepting either a quoted string (whose
/// unescaped contents are returned) or an unquoted bareword. Used for enum
/// intrinsics where both spellings are valid.
///
/// An empty bareword (e.g. the value position is missing or starts at a
/// delimiter such as `}`) is a [`ParseError::Syntax`] error, not a value to be
/// resolved against the enum — otherwise the stray delimiter would surface as a
/// misleading "unknown enum value" [`ParseError::Validation`].
fn read_value_token(input: &str, first: char) -> Result<(String, &str), ParseError> {
    if first == '"' || first == '`' {
        return parse_quoted_string(input, first);
    }
    let (token, rest) = read_bareword(input);
    if token.is_empty() {
        return Err(ParseError::Syntax(
            "expected a value after comparison operator".to_string(),
        ));
    }
    Ok((token.to_string(), rest))
}

/// Split off the leading bareword token, terminating at whitespace, a boolean
/// operator, or either bracket. `}` and `)` can never be part of a value, so
/// they bound the token rather than being absorbed into it. Returns
/// `(token, remaining)`.
fn read_bareword(input: &str) -> (&str, &str) {
    let end = input
        .find(|c: char| c.is_whitespace() || matches!(c, ')' | '}' | '&' | '|'))
        .unwrap_or(input.len());
    (
        input.get(..end).unwrap_or(""),
        input.get(end..).unwrap_or(""),
    )
}

/// Resolve an enum keyword for the `status`/`kind` intrinsics into a typed
/// [`Value`]. Unknown keywords yield a [`ParseError::Validation`] listing the
/// accepted set.
fn resolve_enum_value<'a>(
    intrinsic: Intrinsic,
    text: &str,
    rest: &'a str,
) -> Result<(Value, &'a str), ParseError> {
    match intrinsic {
        Intrinsic::SpanStatus => match StatusValue::parse(text) {
            Some(v) => Ok((Value::Status(v), rest)),
            None => Err(ParseError::Validation(format!(
                "unknown status value '{}'; expected one of {:?}",
                preview(text),
                StatusValue::KEYWORDS
            ))),
        },
        Intrinsic::SpanKind => match KindValue::parse(text) {
            Some(v) => Ok((Value::Kind(v), rest)),
            None => Err(ParseError::Validation(format!(
                "unknown kind value '{}'; expected one of {:?}",
                preview(text),
                KindValue::KEYWORDS
            ))),
        },
        _ => Err(ParseError::Syntax(
            "enum value resolution called for a non-enum intrinsic".to_string(),
        )),
    }
}

/// Parse a quoted string starting with `quote` (`"` with escapes, or a raw
/// backtick string). Returns `(value, remaining)`.
fn parse_quoted_string(input: &str, quote: char) -> Result<(String, &str), ParseError> {
    let value_start = quote.len_utf8();
    let body = input.get(value_start..).unwrap_or("");

    if quote == '`' {
        // Raw string: no escape processing.
        let end = body
            .find('`')
            .ok_or_else(|| ParseError::Syntax("unterminated raw string literal".to_string()))?;
        let value = body.get(..end).unwrap_or("").to_string();
        let rest = body.get(end + 1..).unwrap_or("");
        return Ok((value, rest));
    }

    // Double-quoted string with backslash escapes.
    let mut value = String::new();
    let mut escape = false;
    let mut consumed = 0usize;
    let mut closed = false;
    for (i, ch) in body.char_indices() {
        if escape {
            let mapped = match ch {
                'n' => '\n',
                't' => '\t',
                'r' => '\r',
                other => other,
            };
            value.push(mapped);
            escape = false;
            continue;
        }
        if ch == '\\' {
            escape = true;
            continue;
        }
        if ch == '"' {
            consumed = i + ch.len_utf8();
            closed = true;
            break;
        }
        value.push(ch);
    }

    if !closed {
        return Err(ParseError::Syntax(
            "unterminated string literal".to_string(),
        ));
    }

    let rest = body.get(consumed..).unwrap_or("");
    Ok((value, rest))
}

/// Try to parse a duration literal such as `100ms`, `1s`, `5m`, `1h`.
///
/// Returns `None` if the token is not a well-formed `<number><unit>` duration,
/// in which case the caller falls back to numeric parsing.
fn try_parse_duration(token: &str) -> Option<Duration> {
    let num_end = token.find(|c: char| !c.is_ascii_digit() && c != '.')?;
    if num_end == 0 {
        return None;
    }
    let num_str = token.get(..num_end)?;
    let unit = token.get(num_end..)?;
    let value: f64 = num_str.parse().ok()?;
    if value < 0.0 {
        return None;
    }
    let secs = match unit {
        "ns" => value / 1_000_000_000.0,
        "us" | "µs" => value / 1_000_000.0,
        "ms" => value / 1_000.0,
        "s" => value,
        "m" => value * 60.0,
        "h" => value * 3_600.0,
        _ => return None,
    };
    // `try_from_secs_f64` rejects non-finite and out-of-range values, so an
    // overlong duration token falls through to numeric parsing instead of
    // silently saturating a lossy `f64 as u64` nanosecond cast.
    Duration::try_from_secs_f64(secs).ok()
}

// ===========================================================================
// Shared helpers
// ===========================================================================

/// Truncate a snippet for inclusion in error messages.
fn preview(s: &str) -> String {
    const MAX: usize = 40;
    s.chars().take(MAX).collect()
}

/// Tracks quote/escape state while scanning TraceQL input byte-by-byte.
struct QuoteScanner {
    quote: Option<char>,
    escape: bool,
}

impl QuoteScanner {
    fn new() -> Self {
        Self {
            quote: None,
            escape: false,
        }
    }

    /// Returns `true` if the character is inside (or opens) a quoted string.
    /// Must be called for every character in order.
    fn advance(&mut self, ch: char) -> bool {
        if let Some(q) = self.quote {
            if q == '`' {
                if ch == '`' {
                    self.quote = None;
                }
            } else if self.escape {
                self.escape = false;
            } else if ch == '\\' {
                self.escape = true;
            } else if ch == q {
                self.quote = None;
            }
            return true;
        }

        if matches!(ch, '"' | '`') {
            self.quote = Some(ch);
            return true;
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comparison(spanset: &Spanset) -> &Comparison {
        match spanset.filter.as_ref() {
            Some(FieldExpr::Comparison(c)) => c,
            other => panic!("expected a single comparison, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // Empty spanset
    // -----------------------------------------------------------------------

    #[test]
    fn parse_empty_spanset() {
        let spanset = parse("{}").unwrap();
        assert_eq!(spanset.filter, None);
    }

    #[test]
    fn parse_empty_spanset_with_whitespace() {
        let spanset = parse("{   }").unwrap();
        assert_eq!(spanset.filter, None);
    }

    // -----------------------------------------------------------------------
    // Scoped attributes
    // -----------------------------------------------------------------------

    #[test]
    fn parse_resource_service_name() {
        let spanset = parse(r#"{ resource.service.name = "api" }"#).unwrap();
        let c = comparison(&spanset);
        assert_eq!(
            c.field,
            Field::Attribute {
                scope: AttributeScope::Resource,
                name: "service.name".to_string(),
            }
        );
        assert_eq!(c.op, ComparisonOp::Eq);
        assert_eq!(c.value, Value::String("api".to_string()));
    }

    #[test]
    fn parse_span_attribute_numeric() {
        let spanset = parse(r#"{ span.http.status_code = 500 }"#).unwrap();
        let c = comparison(&spanset);
        assert_eq!(
            c.field,
            Field::Attribute {
                scope: AttributeScope::Span,
                name: "http.status_code".to_string(),
            }
        );
        assert_eq!(c.value, Value::Integer(500));
    }

    #[test]
    fn parse_not_equal() {
        let spanset = parse(r#"{ resource.region != "us-east-1" }"#).unwrap();
        assert_eq!(comparison(&spanset).op, ComparisonOp::Ne);
    }

    #[test]
    fn parse_regex_match() {
        let spanset = parse(r#"{ span.name =~ "GET /.*" }"#).unwrap();
        assert_eq!(comparison(&spanset).op, ComparisonOp::Re);
    }

    #[test]
    fn parse_regex_non_match() {
        let spanset = parse(r#"{ span.http.target !~ "/health.*" }"#).unwrap();
        assert_eq!(comparison(&spanset).op, ComparisonOp::Nre);
    }

    // -----------------------------------------------------------------------
    // Intrinsics
    // -----------------------------------------------------------------------

    #[test]
    fn parse_span_name_intrinsic() {
        let spanset = parse(r#"{ span:name = "HTTP GET" }"#).unwrap();
        assert_eq!(
            comparison(&spanset).field,
            Field::Intrinsic(Intrinsic::SpanName)
        );
    }

    #[test]
    fn parse_trace_id_intrinsic() {
        let spanset = parse(r#"{ trace:id = "0af7651916cd43dd8448eb211c80319c" }"#).unwrap();
        assert_eq!(
            comparison(&spanset).field,
            Field::Intrinsic(Intrinsic::TraceId)
        );
    }

    #[test]
    fn parse_span_duration_intrinsic() {
        let spanset = parse("{ span:duration > 100ms }").unwrap();
        let c = comparison(&spanset);
        assert_eq!(c.field, Field::Intrinsic(Intrinsic::SpanDuration));
        assert_eq!(c.op, ComparisonOp::Gt);
        assert_eq!(c.value, Value::Duration(Duration::from_millis(100)));
    }

    #[test]
    fn parse_trace_duration_intrinsic() {
        let spanset = parse("{ trace:duration >= 1s }").unwrap();
        let c = comparison(&spanset);
        assert_eq!(c.field, Field::Intrinsic(Intrinsic::TraceDuration));
        assert_eq!(c.value, Value::Duration(Duration::from_secs(1)));
    }

    #[test]
    fn parse_span_status_intrinsic() {
        let spanset = parse("{ span:status = error }").unwrap();
        let c = comparison(&spanset);
        assert_eq!(c.field, Field::Intrinsic(Intrinsic::SpanStatus));
        assert_eq!(c.value, Value::Status(StatusValue::Error));
    }

    #[test]
    fn parse_span_kind_intrinsic() {
        let spanset = parse("{ span:kind = server }").unwrap();
        let c = comparison(&spanset);
        assert_eq!(c.field, Field::Intrinsic(Intrinsic::SpanKind));
        assert_eq!(c.value, Value::Kind(KindValue::Server));
    }

    // -----------------------------------------------------------------------
    // Bare-intrinsic aliases emitted by Grafana's Search query builder
    // -----------------------------------------------------------------------

    #[test]
    fn bare_name_aliases_span_name() {
        let spanset = parse(r#"{ name = "POST /api/orders" }"#).unwrap();
        let c = comparison(&spanset);
        assert_eq!(c.field, Field::Intrinsic(Intrinsic::SpanName));
        assert_eq!(c.value, Value::String("POST /api/orders".to_string()));
    }

    #[test]
    fn bare_duration_aliases_span_duration() {
        let spanset = parse("{ duration > 500ms }").unwrap();
        let c = comparison(&spanset);
        assert_eq!(c.field, Field::Intrinsic(Intrinsic::SpanDuration));
        assert_eq!(c.value, Value::Duration(Duration::from_millis(500)));
    }

    #[test]
    fn bare_status_enum_unquoted() {
        let spanset = parse("{ status = error }").unwrap();
        let c = comparison(&spanset);
        assert_eq!(c.field, Field::Intrinsic(Intrinsic::SpanStatus));
        assert_eq!(c.value, Value::Status(StatusValue::Error));
    }

    #[test]
    fn bare_status_enum_quoted() {
        let spanset = parse(r#"{ status = "ok" }"#).unwrap();
        assert_eq!(comparison(&spanset).value, Value::Status(StatusValue::Ok));
    }

    #[test]
    fn bare_kind_enum_unquoted() {
        let spanset = parse("{ kind = client }").unwrap();
        let c = comparison(&spanset);
        assert_eq!(c.field, Field::Intrinsic(Intrinsic::SpanKind));
        assert_eq!(c.value, Value::Kind(KindValue::Client));
    }

    #[test]
    fn grafana_search_builder_and_combination() {
        // Representative of what Grafana's Search builder emits.
        let spanset = parse(r#"{ resource.service.name = "frontend" && status = error }"#).unwrap();
        let Some(FieldExpr::Binary {
            op: BoolOp::And,
            lhs,
            rhs,
        }) = spanset.filter
        else {
            panic!("expected top-level And");
        };
        assert!(matches!(
            *lhs,
            FieldExpr::Comparison(Comparison {
                field: Field::Attribute { .. },
                ..
            })
        ));
        assert!(matches!(
            *rhs,
            FieldExpr::Comparison(Comparison {
                value: Value::Status(StatusValue::Error),
                ..
            })
        ));
    }

    #[test]
    fn unknown_status_enum_is_validation_error() {
        let err = parse("{ status = bogus }").unwrap_err();
        assert!(matches!(err, ParseError::Validation(_)), "got {err:?}");
    }

    #[test]
    fn unknown_kind_enum_is_validation_error() {
        let err = parse("{ kind = wormhole }").unwrap_err();
        assert!(matches!(err, ParseError::Validation(_)), "got {err:?}");
    }

    #[test]
    fn status_enum_with_ordering_op_is_validation_error() {
        let err = parse("{ status > error }").unwrap_err();
        assert!(matches!(err, ParseError::Validation(_)), "got {err:?}");
    }

    #[test]
    fn broader_unscoped_field_still_unsupported() {
        // Only the four documented bare intrinsics are aliased; other unscoped
        // identifiers remain Unsupported.
        let err = parse(r#"{ service = "frontend" }"#).unwrap_err();
        assert!(matches!(err, ParseError::Unsupported(_)), "got {err:?}");
    }

    // -----------------------------------------------------------------------
    // Regex patterns are stored verbatim; RE2 validation is deferred to the
    // caller's regex engine. The parser must accept any string pattern,
    // including constructs the RE2 engine will later reject.
    // -----------------------------------------------------------------------

    #[test]
    fn regex_patterns_parse_verbatim_without_re2_validation() {
        // Including non-RE2 constructs (lookaround, backreferences): the syntactic
        // parser does not compile the pattern, so these parse here and are
        // rejected by the caller's regex engine — not as a parser error.
        for query in [
            r#"{ span.name =~ "(?:GET|POST) /.*" }"#,
            r#"{ span.name =~ "(?P<verb>GET|POST)" }"#,
            r#"{ span.name =~ "(?i)get" }"#,
            r#"{ span.path =~ "\\d+" }"#,
            r#"{ span.name =~ "[(?=]" }"#, // char class, NOT lookahead
            r#"{ span.name =~ "foo(?=bar)" }"#, // lookahead — deferred
            r#"{ span.name !~ "(?<=foo)bar" }"#, // lookbehind — deferred
            r"{ span.name =~ `(foo)\1` }", // backreference — deferred
        ] {
            let spanset = parse(query).unwrap_or_else(|e| panic!("{query}: {e:?}"));
            assert!(matches!(
                comparison(&spanset).op,
                ComparisonOp::Re | ComparisonOp::Nre
            ));
        }
    }

    // -----------------------------------------------------------------------
    // Literals
    // -----------------------------------------------------------------------

    #[test]
    fn parse_boolean_literal() {
        let spanset = parse("{ span.ok = true }").unwrap();
        assert_eq!(comparison(&spanset).value, Value::Bool(true));
    }

    #[test]
    fn parse_integer_literal() {
        let spanset = parse("{ span.retries = 3 }").unwrap();
        assert_eq!(comparison(&spanset).value, Value::Integer(3));
    }

    #[test]
    fn parse_large_integer_literal_is_exact() {
        // 2^53 + 1 is not representable in f64; it must survive as an exact i64.
        let spanset = parse("{ span.id = 9007199254740993 }").unwrap();
        assert_eq!(
            comparison(&spanset).value,
            Value::Integer(9_007_199_254_740_993)
        );
    }

    #[test]
    fn parse_decimal_literal_stays_float() {
        let spanset = parse("{ span.ratio = 0.5 }").unwrap();
        assert_eq!(comparison(&spanset).value, Value::Number(0.5));
    }

    #[test]
    fn parse_float_literal() {
        let spanset = parse("{ span.ratio = 0.5 }").unwrap();
        assert_eq!(comparison(&spanset).value, Value::Number(0.5));
    }

    #[test]
    fn parse_string_with_escape() {
        let spanset = parse(r#"{ span.msg = "say \"hi\"" }"#).unwrap();
        assert_eq!(
            comparison(&spanset).value,
            Value::String("say \"hi\"".to_string())
        );
    }

    #[test]
    fn parse_various_duration_units() {
        for (text, expected) in [
            ("500ns", Duration::from_nanos(500)),
            ("250us", Duration::from_micros(250)),
            ("100ms", Duration::from_millis(100)),
            ("2s", Duration::from_secs(2)),
            ("5m", Duration::from_secs(300)),
            ("1h", Duration::from_secs(3600)),
        ] {
            let query = format!("{{ span:duration > {text} }}");
            let spanset = parse(&query).unwrap();
            assert_eq!(
                comparison(&spanset).value,
                Value::Duration(expected),
                "duration {text}"
            );
        }
    }

    #[test]
    fn overlong_duration_does_not_silently_saturate() {
        // Out of `Duration`'s range: must error rather than clamp to a bogus
        // ~584-year value via a lossy `f64 as u64` cast.
        let err = parse("{ span:duration > 999999999999999999999h }").unwrap_err();
        assert!(matches!(err, ParseError::Syntax(_)), "got {err:?}");
    }

    // -----------------------------------------------------------------------
    // Boolean combinations
    // -----------------------------------------------------------------------

    #[test]
    fn parse_and_combination() {
        let spanset =
            parse(r#"{ span.http.status_code >= 400 && span.http.status_code < 600 }"#).unwrap();
        match spanset.filter {
            Some(FieldExpr::Binary {
                op: BoolOp::And, ..
            }) => {}
            other => panic!("expected And binary, got {other:?}"),
        }
    }

    #[test]
    fn parse_or_combination() {
        let spanset =
            parse(r#"{ resource.service.name = "api" || resource.service.name = "web" }"#).unwrap();
        match spanset.filter {
            Some(FieldExpr::Binary { op: BoolOp::Or, .. }) => {}
            other => panic!("expected Or binary, got {other:?}"),
        }
    }

    #[test]
    fn and_binds_tighter_than_or() {
        // a || b && c  ==>  a || (b && c)
        let spanset = parse(r#"{ span.a = "1" || span.b = "2" && span.c = "3" }"#).unwrap();
        let Some(FieldExpr::Binary {
            op: BoolOp::Or,
            rhs,
            ..
        }) = spanset.filter
        else {
            panic!("expected top-level Or");
        };
        assert!(
            matches!(
                *rhs,
                FieldExpr::Binary {
                    op: BoolOp::And,
                    ..
                }
            ),
            "right operand of || should be the && group"
        );
    }

    #[test]
    fn parentheses_override_precedence() {
        // (a || b) && c  ==>  And(Or(a, b), c)
        let spanset = parse(r#"{ (span.a = "1" || span.b = "2") && span.c = "3" }"#).unwrap();
        let Some(FieldExpr::Binary {
            op: BoolOp::And,
            lhs,
            ..
        }) = spanset.filter
        else {
            panic!("expected top-level And");
        };
        assert!(
            matches!(*lhs, FieldExpr::Binary { op: BoolOp::Or, .. }),
            "left operand of && should be the parenthesized Or group"
        );
    }

    // -----------------------------------------------------------------------
    // Unsupported (deferred subset) — must be typed Unsupported, not Syntax
    // -----------------------------------------------------------------------

    #[test]
    fn structural_descendant_is_unsupported() {
        let err = parse(r#"{ span.a = "1" } >> { span.b = "2" }"#).unwrap_err();
        assert!(matches!(err, ParseError::Unsupported(_)), "got {err:?}");
    }

    /// Every structural operator — not just the two-character ones — is valid
    /// TraceQL outside the subset, so all must classify as `Unsupported`. The
    /// single-character child/parent/sibling forms previously fell through to
    /// `Syntax` ("unexpected trailing input"), telling a user their structural
    /// query was malformed rather than unsupported (found by the
    /// compatibility corpus).
    #[test]
    fn every_structural_operator_is_unsupported() {
        for op in [
            ">>", "<<", ">", "<", "~", "!>>", "!<<", "!>", "!<", "!~", "&>>", "&<<", "&>", "&<",
            "&~",
        ] {
            let query = format!(r#"{{ span.a = "1" }} {op} {{ span.b = "2" }}"#);
            let err = parse(&query).unwrap_err();
            assert!(
                matches!(err, ParseError::Unsupported(_)),
                "operator {op:?} should be Unsupported, got {err:?}"
            );
        }
    }

    /// A longer operator must not be reported as its shorter prefix.
    #[test]
    fn structural_operator_reported_at_full_width() {
        let err = parse(r#"{ span.a = "1" } >> { span.b = "2" }"#).unwrap_err();
        let ParseError::Unsupported(msg) = err else {
            panic!("expected Unsupported");
        };
        assert!(msg.contains("'>>'"), "got {msg}");
    }

    #[test]
    fn metrics_pipe_is_unsupported() {
        let err = parse(r#"{ span.a = "1" } | rate()"#).unwrap_err();
        assert!(matches!(err, ParseError::Unsupported(_)), "got {err:?}");
    }

    #[test]
    fn hints_are_unsupported() {
        let err = parse(r#"{ span.a = "1" } with (most_recent=true)"#).unwrap_err();
        assert!(matches!(err, ParseError::Unsupported(_)), "got {err:?}");
    }

    #[test]
    fn event_scope_is_unsupported() {
        let err = parse(r#"{ event.name = "exception" }"#).unwrap_err();
        assert!(matches!(err, ParseError::Unsupported(_)), "got {err:?}");
    }

    #[test]
    fn spanset_combining_is_unsupported() {
        let err = parse(r#"{ span.a = "1" } && { span.b = "2" }"#).unwrap_err();
        assert!(matches!(err, ParseError::Unsupported(_)), "got {err:?}");
    }

    #[test]
    fn unknown_intrinsic_is_unsupported() {
        let err = parse(r#"{ span:foo = "x" }"#).unwrap_err();
        assert!(matches!(err, ParseError::Unsupported(_)), "got {err:?}");
    }

    #[test]
    fn unscoped_attribute_is_unsupported() {
        let err = parse(r#"{ .name = "x" }"#).unwrap_err();
        assert!(matches!(err, ParseError::Unsupported(_)), "got {err:?}");
    }

    // -----------------------------------------------------------------------
    // Syntax errors
    // -----------------------------------------------------------------------

    #[test]
    fn empty_query_is_syntax_error() {
        let err = parse("").unwrap_err();
        assert!(matches!(err, ParseError::Syntax(_)), "got {err:?}");
    }

    #[test]
    fn missing_open_brace_is_syntax_error() {
        let err = parse(r#"resource.service.name = "api""#).unwrap_err();
        assert!(matches!(err, ParseError::Syntax(_)), "got {err:?}");
    }

    #[test]
    fn unterminated_spanset_is_syntax_error() {
        let err = parse(r#"{ span.a = "1""#).unwrap_err();
        assert!(matches!(err, ParseError::Syntax(_)), "got {err:?}");
    }

    #[test]
    fn double_equals_is_syntax_error() {
        let err = parse(r#"{ span.name == "x" }"#).unwrap_err();
        assert!(matches!(err, ParseError::Syntax(_)), "got {err:?}");
    }

    #[test]
    fn missing_value_is_syntax_error() {
        let err = parse("{ span:duration > }").unwrap_err();
        assert!(matches!(err, ParseError::Syntax(_)), "got {err:?}");
    }

    #[test]
    fn missing_field_is_syntax_error() {
        let err = parse(r#"{ = "x" }"#).unwrap_err();
        assert!(matches!(err, ParseError::Syntax(_)), "got {err:?}");
    }

    #[test]
    fn missing_status_enum_value_is_syntax_error() {
        let err = parse("{ status = }").unwrap_err();
        assert!(matches!(err, ParseError::Syntax(_)), "got {err:?}");
    }

    #[test]
    fn missing_kind_enum_value_is_syntax_error() {
        let err = parse("{ kind = }").unwrap_err();
        assert!(matches!(err, ParseError::Syntax(_)), "got {err:?}");
    }

    #[test]
    fn missing_scoped_status_enum_value_is_syntax_error() {
        let err = parse("{ span:status = }").unwrap_err();
        assert!(matches!(err, ParseError::Syntax(_)), "got {err:?}");
    }

    #[test]
    fn missing_scoped_kind_enum_value_is_syntax_error() {
        let err = parse("{ span:kind = }").unwrap_err();
        assert!(matches!(err, ParseError::Syntax(_)), "got {err:?}");
    }

    #[test]
    fn read_value_token_rejects_delimiter_only_rhs_as_syntax() {
        // The public parser strips the closing brace before reaching here, but
        // the helper must classify a stray delimiter / empty value position as
        // Syntax (not let it fall through to enum Validation).
        for input in ["}", ")", "&& span.x = 1", "|| span.y = 2"] {
            let first = input.chars().next().unwrap();
            let err = read_value_token(input, first).unwrap_err();
            assert!(
                matches!(err, ParseError::Syntax(_)),
                "input {input:?}: {err:?}"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Validation errors (semantic type mismatch)
    // -----------------------------------------------------------------------

    #[test]
    fn regex_against_number_is_validation_error() {
        let err = parse("{ span.code =~ 500 }").unwrap_err();
        assert!(matches!(err, ParseError::Validation(_)), "got {err:?}");
    }

    #[test]
    fn ordering_against_string_is_validation_error() {
        let err = parse(r#"{ span.name > "x" }"#).unwrap_err();
        assert!(matches!(err, ParseError::Validation(_)), "got {err:?}");
    }

    #[test]
    fn empty_attribute_name_is_syntax_error() {
        let err = parse(r#"{ span. = "x" }"#).unwrap_err();
        assert!(matches!(err, ParseError::Syntax(_)), "got {err:?}");
    }

    // -----------------------------------------------------------------------
    // Nesting depth bound
    // -----------------------------------------------------------------------

    /// Query shapes whose nesting grows with `depth`. Each costs one unit of
    /// budget per level: the parenthesised form adds a parser frame per level,
    /// the operator chains deepen the left-deep tree one node per operator.
    fn nested_inputs(depth: usize) -> Vec<(&'static str, String)> {
        let leaf = "span.a = 1";
        vec![
            (
                "parentheses",
                format!("{{ {}{leaf}{} }}", "(".repeat(depth), ")".repeat(depth)),
            ),
            (
                "conjunct chain",
                format!("{{ {leaf}{} }}", format!(" && {leaf}").repeat(depth)),
            ),
            (
                "disjunct chain",
                format!("{{ {leaf}{} }}", format!(" || {leaf}").repeat(depth)),
            ),
            (
                "mixed chain",
                format!(
                    "{{ {leaf}{} }}",
                    format!(" && {leaf} || {leaf}").repeat(depth)
                ),
            ),
        ]
    }

    #[test]
    fn nesting_beyond_the_limit_is_a_syntax_error() {
        for (name, query) in nested_inputs(MAX_NESTING_DEPTH + 1) {
            match parse(&query) {
                Err(ParseError::Syntax(msg)) => assert!(
                    msg.contains("maximum depth"),
                    "{name}: unexpected message {msg}"
                ),
                Err(other) => panic!("{name}: expected depth error, got {other:?}"),
                // Deliberately not printing the tree: at these depths its
                // `Debug` form is megabytes long.
                Ok(_) => panic!("{name}: expected depth error, but the query parsed"),
            }
        }
    }

    #[test]
    fn nesting_well_within_the_limit_still_parses() {
        for (name, query) in nested_inputs(MAX_NESTING_DEPTH / 4) {
            assert!(parse(&query).is_ok(), "{name} rejected: {query}");
        }
    }

    #[test]
    fn rejected_query_releases_its_nesting_budget() {
        for (_, query) in nested_inputs(MAX_NESTING_DEPTH * 4) {
            assert!(parse(&query).is_err());
            assert_eq!(NESTING_DEPTH.with(Cell::get), 0);
        }
        assert!(parse("{ span.a = 1 && span.b = 2 }").is_ok());
        assert_eq!(NESTING_DEPTH.with(Cell::get), 0);
    }

    /// Hostile depths must fail cleanly rather than overflow: a stack overflow
    /// aborts the process serving the Tempo and Jaeger query endpoints. A 2 MiB
    /// thread has headroom at the limit, and query threads (8 MiB) have far
    /// more. Accepted trees are also formatted and dropped, since both recurse
    /// over the tree.
    #[test]
    fn hostile_nesting_does_not_overflow_a_small_stack() {
        std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(|| {
                for depth in [MAX_NESTING_DEPTH, 10_000, 100_000] {
                    for (_, query) in nested_inputs(depth) {
                        if let Ok(spanset) = parse(&query) {
                            let _ = format!("{spanset:?}");
                        }
                    }
                }
            })
            .unwrap()
            .join()
            .expect("parser overflowed its stack on nested input");
    }
}
