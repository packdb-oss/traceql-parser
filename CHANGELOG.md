# Changelog

## 0.1.1

First release as a standalone crate.

- TraceQL parser covering the constructs in COMPATIBILITY.md, with the
  rejection category (`Unsupported` / `Syntax` / `Validation`) as part of the
  contract.
- Nesting depth is bounded by `MAX_NESTING_DEPTH`; deeper input returns
  `ParseError::Syntax` instead of overflowing the stack.
- Compatibility corpus, acceptance/rejection matrix and adversarial input tests.
