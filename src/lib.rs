// Copyright 2026 PackDB Contributors
//
// Licensed under the Apache License, Version 2.0

//! `packdb-traceql-parser` — a parser for the TraceQL trace query language.
//!
//! This crate provides a clean, dependency-light TraceQL parser for the
//! Tempo-compatible search subset listed in COMPATIBILITY.md. It is a
//! clean-room behavioral implementation authored from public TraceQL
//! documentation; upstream Grafana Tempo `pkg/traceql` (AGPL-3.0) is used only
//! as a behavioral reference, never as copied source.
//!
//! # Supported subset
//!
//! - Empty spanset selector: `{}`
//! - Scoped attributes: `resource.<name>`, `span.<name>`
//! - Intrinsics: `span:name`, `trace:id`, `span:duration`, `trace:duration`,
//!   `span:status`, `span:kind`, plus the bare aliases Grafana's Search builder
//!   emits (`name`, `duration`, `status`, `kind`)
//! - Comparisons: `=`, `!=`, `=~`, `!~`, `<`, `<=`, `>`, `>=`
//! - Boolean combinations: `&&`, `||`, parentheses
//! - Literals: strings, numbers, durations, booleans, and `status`/`kind` enum
//!   keywords (e.g. `status = error`, `kind = server`)
//!
//! Regex patterns for `=~`/`!~` are stored verbatim and not compiled here; RE2
//! validation (rejecting lookaround, backreferences, etc.) is left to the
//! caller, which compiles the pattern against its own regex engine.
//!
//! Errors are typed: valid TraceQL outside the subset (structural operators,
//! metrics, hints, event/link scope) is rejected with
//! [`ParseError::Unsupported`]; malformed input with [`ParseError::Syntax`]; and
//! an operator/value-type or unknown enum mismatch with
//! [`ParseError::Validation`] — never silently mis-parsed.
//!
//! A field expression may nest at most [`MAX_NESTING_DEPTH`] levels deep;
//! beyond that the query is a [`ParseError::Syntax`] error rather than a
//! stack overflow, which would abort the process.
//!
//! # Public API
//!
//! ```
//! use packdb_traceql_parser::parse;
//!
//! let spanset = parse(r#"{ resource.service.name = "api" }"#).unwrap();
//! println!("{spanset:?}");
//! ```

#![cfg_attr(not(test), deny(clippy::unwrap_used))]
#![cfg_attr(not(test), deny(clippy::expect_used))]
#![cfg_attr(not(test), deny(clippy::indexing_slicing))]
#![cfg_attr(not(test), deny(clippy::panic))]
#![cfg_attr(not(test), deny(clippy::unreachable))]

pub mod ast;
pub mod error;
mod parser;

pub use error::ParseError;
pub use parser::{MAX_NESTING_DEPTH, parse};
