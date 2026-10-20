// Copyright 2026 PackDB Contributors
//
// Licensed under the Apache License, Version 2.0

//! Error types for the TraceQL parser.

/// Errors produced during TraceQL parsing.
///
/// The three categories mirror `logql-parser`'s `ParseError` so callers can
/// treat parser failures uniformly across query languages:
///
/// - [`Syntax`](ParseError::Syntax): malformed input.
/// - [`Unsupported`](ParseError::Unsupported): valid TraceQL that falls outside
///   the supported subset — surfaced explicitly so users see a clear
///   error instead of a silent miscompilation.
/// - [`Validation`](ParseError::Validation): syntactically valid input that
///   fails a semantic rule (e.g. a duration comparison against a string).
#[derive(Debug, Clone, thiserror::Error)]
pub enum ParseError {
    /// Syntax error in the TraceQL input.
    #[error("parse error: {0}")]
    Syntax(String),

    /// The query uses a TraceQL feature outside the supported subset.
    #[error("unsupported TraceQL feature: {0}")]
    Unsupported(String),

    /// Semantic validation failed (e.g., duration comparison against a string).
    #[error("validation error: {0}")]
    Validation(String),
}
