// Copyright 2026 PackDB Contributors
//
// Licensed under the Apache License, Version 2.0

//! Syntax-level AST for the supported TraceQL subset.
//!
//! These types represent the direct output of the parser before any semantic
//! analysis, lowering, or query planning. They map to documented TraceQL
//! grammar productions and carry no evaluation semantics.

use std::time::Duration;

/// A parsed TraceQL spanset filter: the contents of a single `{ ... }` block.
///
/// `{}` (the empty spanset selector) parses to `Spanset { filter: None }` and
/// matches every span.
#[derive(Debug, Clone, PartialEq)]
pub struct Spanset {
    /// The field expression inside the braces, or `None` for `{}`.
    pub filter: Option<FieldExpr>,
}

/// A field expression: a single comparison or a boolean combination.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldExpr {
    /// A leaf comparison: `span.http.status_code >= 400`.
    Comparison(Comparison),
    /// A boolean combination: `<lhs> && <rhs>` or `<lhs> || <rhs>`.
    Binary {
        /// The combining operator.
        op: BoolOp,
        /// The left operand.
        lhs: Box<FieldExpr>,
        /// The right operand.
        rhs: Box<FieldExpr>,
    },
}

/// Boolean combination operators within a spanset filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoolOp {
    /// `&&`
    And,
    /// `||`
    Or,
}

/// A single comparison between a field and a literal value.
#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    /// The attribute or intrinsic being compared.
    pub field: Field,
    /// The comparison operator.
    pub op: ComparisonOp,
    /// The literal the field is compared against.
    pub value: Value,
}

/// The left-hand side of a comparison: a scoped attribute or an intrinsic.
#[derive(Debug, Clone, PartialEq)]
pub enum Field {
    /// A scoped attribute: `resource.<name>` or `span.<name>`.
    Attribute {
        /// Whether the attribute is `resource`- or `span`-scoped.
        scope: AttributeScope,
        /// The attribute name after the scope, e.g. `http.status_code`.
        name: String,
    },
    /// A built-in intrinsic field (`span:name`, `trace:id`, ...).
    Intrinsic(Intrinsic),
}

/// Attribute scope prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeScope {
    /// `resource.<name>`
    Resource,
    /// `span.<name>`
    Span,
}

/// Supported TraceQL intrinsic fields.
///
/// Grafana's Search query builder emits the unscoped spellings `name`,
/// `duration`, `status`, and `kind`; these are accepted as aliases of the
/// span-scoped intrinsics. `SpanName`/`SpanDuration` therefore
/// cover both `name` and `span:name`, etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intrinsic {
    /// `name` / `span:name`
    SpanName,
    /// `trace:id`
    TraceId,
    /// `duration` / `span:duration`
    SpanDuration,
    /// `trace:duration`
    TraceDuration,
    /// `status` / `span:status`
    SpanStatus,
    /// `kind` / `span:kind`
    SpanKind,
}

/// Comparison operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonOp {
    /// `=`
    Eq,
    /// `!=`
    Ne,
    /// `=~` (regex match)
    Re,
    /// `!~` (regex non-match)
    Nre,
    /// `>`
    Gt,
    /// `>=`
    Gte,
    /// `<`
    Lt,
    /// `<=`
    Lte,
}

impl ComparisonOp {
    /// Returns `true` for the ordering operators (`< <= > >=`), which require a
    /// numeric or duration right-hand side.
    pub fn is_ordering(self) -> bool {
        matches!(self, Self::Gt | Self::Gte | Self::Lt | Self::Lte)
    }

    /// Returns `true` for the regex operators (`=~`, `!~`), which require a
    /// string right-hand side.
    pub fn is_regex(self) -> bool {
        matches!(self, Self::Re | Self::Nre)
    }
}

/// A literal value on the right-hand side of a comparison.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A quoted string literal: `"api"`.
    String(String),
    /// An integral numeric literal: `500`, `-3`, `9007199254740993`. Kept as
    /// `i64` (not folded into [`Value::Number`]) so a comparison against a large
    /// integer attribute stays exact — `f64` would round values above `2^53`.
    Integer(i64),
    /// A non-integral numeric literal: `0.5`, `1e3`.
    Number(f64),
    /// A duration literal: `100ms`, `1s`.
    Duration(Duration),
    /// A boolean literal: `true`, `false`.
    Bool(bool),
    /// A span-status enum literal: `status = error`.
    Status(StatusValue),
    /// A span-kind enum literal: `kind = server`.
    Kind(KindValue),
}

/// Span-status enum values (`status = ok | error | unset`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusValue {
    /// `unset`: no status was recorded.
    Unset,
    /// `ok`: the span completed successfully.
    Ok,
    /// `error`: the span failed.
    Error,
}

impl StatusValue {
    /// Parse a status enum keyword. Returns `None` for unknown values.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "unset" => Some(Self::Unset),
            "ok" => Some(Self::Ok),
            "error" => Some(Self::Error),
            _ => None,
        }
    }

    /// The set of accepted status keywords, for error messages.
    pub const KEYWORDS: &'static [&'static str] = &["ok", "error", "unset"];
}

/// Span-kind enum values (`kind = server | client | ...`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KindValue {
    /// `unspecified`: no kind was recorded.
    Unspecified,
    /// `internal`: work inside a service, with no remote peer.
    Internal,
    /// `server`: handling an inbound request.
    Server,
    /// `client`: issuing an outbound request.
    Client,
    /// `producer`: publishing a message.
    Producer,
    /// `consumer`: receiving a message.
    Consumer,
}

impl KindValue {
    /// Parse a kind enum keyword. Returns `None` for unknown values.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "unspecified" => Some(Self::Unspecified),
            "internal" => Some(Self::Internal),
            "server" => Some(Self::Server),
            "client" => Some(Self::Client),
            "producer" => Some(Self::Producer),
            "consumer" => Some(Self::Consumer),
            _ => None,
        }
    }

    /// The set of accepted kind keywords, for error messages.
    pub const KEYWORDS: &'static [&'static str] = &[
        "unspecified",
        "internal",
        "server",
        "client",
        "producer",
        "consumer",
    ];
}
