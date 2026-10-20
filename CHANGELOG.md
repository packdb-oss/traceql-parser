# Changelog

## 0.1.2

- Replace the LICENSE file with the verbatim Apache License 2.0 text. Earlier
  releases shipped an altered text: the Trademarks clause (section 6) was
  missing, sections 7 to 9 were renumbered 6 to 8, the copyright grant wording
  differed, and section 9 was retitled. The crate's licensing (Apache-2.0, as
  declared in `Cargo.toml` and the source headers) is unchanged. Earlier
  release archives keep their original text; this release supersedes them.

## 0.1.1

First release as a standalone crate.

- TraceQL parser covering the constructs in COMPATIBILITY.md, with the
  rejection category (`Unsupported` / `Syntax` / `Validation`) as part of the
  contract.
- Nesting depth is bounded by `MAX_NESTING_DEPTH`; deeper input returns
  `ParseError::Syntax` instead of overflowing the stack.
- Compatibility corpus, acceptance/rejection matrix and adversarial input tests.
