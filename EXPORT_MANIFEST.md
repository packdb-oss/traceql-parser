# Export manifest

This repository was created from the TraceQL parser in the PackDB project. Its
history starts with a single reviewed commit; earlier development history is not
included.

## Origin and attribution

- `src/` is the PackDB TraceQL parser, written by PackDB contributors and
  licensed under Apache-2.0. Copyright notices are kept in each source file.
- `tests/corpus/parser_cases.jsonl` is the PackDB TraceQL compatibility corpus
  and `tests/acceptance_matrix.rs` its hand-maintained parity table.
  COMPATIBILITY.md describes their provenance. No file is copied from Grafana
  Tempo, whose source is AGPL-3.0; the corpus was checked against Tempo's
  `pkg/traceql` tree and no query occurs there.
- `LICENSE` is the Apache License 2.0.
- Policy documents come from the PackDB open source standards kit.

Changes made during extraction: standalone package metadata (published under
the name `packdb-traceql-parser`), design-doc citations and internal milestone
labels replaced by their public equivalents in comments, a standalone corpus
harness replacing the one shared with PackDB's other upstream suites,
adversarial input tests, a repository CI adapter, and the policy documents.
Parsing behaviour is unchanged.

## Inventory

42 files, excluding this manifest. SHA-256 of each file:

| File | SHA-256 |
| --- | --- |
| `.github/CODEOWNERS` | `5a4dbd4836353e4a9f151a6be0b4dbb7dedf0e14d66ccf47f8398234e988515b` |
| `.github/ISSUE_TEMPLATE/bug_report.md` | `ce575ab185eab1d319b89ad41b414da89a1e836a93f01295cffbca83b7ec32c4` |
| `.github/ISSUE_TEMPLATE/feature_request.md` | `866952ad8d2940cecabcf978e6c972838cb1b0d6cbaa99241ec026ee0095ca9d` |
| `.github/PULL_REQUEST_TEMPLATE.md` | `8be2381baf9196a5451bbb3ef80f014fc42348c283d5ddeacbfe64263acc4e03` |
| `.github/dependabot.yml` | `6a082e0c22e36e7ae754cb611bc1b8cbb96924355b87480dae1cbb7e57a8bcb2` |
| `.github/workflows/oss-candidate.yml` | `786e3c9950aced190e339b03905fce4583fa0c85f786cf9dbdc71b1eec951bd6` |
| `.github/workflows/oss-ci.yml` | `3cd28c78e90058f9004d804181e7d45c6c6f22e9d77bdf89de3362f29998cdee` |
| `.github/workflows/oss-publish.yml` | `565d241f01e7e40e67ce02de1a4f1a530466e9bb7d6af4b19af79c0b0f6ed5f4` |
| `.github/workflows/oss-security.yml` | `49ed49d5ae22586e392832afc7ed0d8cba8dbb9024eaec4c968d66cb43578de5` |
| `.gitignore` | `4c41fc926fddfaae21cfa16fc3b3a86e406ad7ef4256e3c1490514edb21bc53f` |
| `AI_POLICY.md` | `f512a5c57f95f00664e21a855d4f223b68560097df02d2da9c69aa5c61d7b62c` |
| `CHANGELOG.md` | `f09e80d0f3f9d80df33ccf3f8b3e1830c019fd833632ddf96c3a239f1247e869` |
| `CODE_OF_CONDUCT.md` | `3bec071ae7672b006573d09a616bdfdc73dd8c1ca5ea713336fc8176c75a8bb0` |
| `CODING_STANDARDS.md` | `5380828dd9f6f9a8c19dd99d7a58a3073be2125ca6cba7d09562ff3ed8cffddd` |
| `COMPATIBILITY.md` | `f48aef70fbb8ad92cac53d9866c28b47dc4e8defa616278d265af0af9ad8e2e6` |
| `CONTRIBUTING.md` | `a77fa5dadbf534914871234b837cf4b5ee147381cb0c6d0e9473e556ca06983e` |
| `Cargo.lock` | `e8ca3104d79bdc058c4c2e2e523b923998a601242565ba5ae7aa2b9b75ad61ce` |
| `Cargo.toml` | `3466081e4f262920fcea4898efa283732184d445e938565e48a053c454ae542c` |
| `GOVERNANCE.md` | `c6dfa89a3f968e98d35b1e5d9f45e5a81898e08e645a41063b698d32e4ba3a47` |
| `LICENSE` | `e3ec3e590a85899de47ba64e68fbe540a1b3de2c7c1616edddddc5ca8733b2cc` |
| `MAINTAINERS.md` | `5b5519384484354399796ba094d927d2aad42529b3f1bd49ca15b5c83e44cf27` |
| `README.md` | `64b144986121198fcb3bbc12f88ebd41a52be09dcfc97cf2d52158773b6f996d` |
| `RELEASING.md` | `223a2be578cd53e891c0020d9e7aa071878f5cee378c39ec3ba28e7faf506477` |
| `SECURITY.md` | `d63898d56a6ade84cf748bb4e7448b4292a64e121a9d6e61d409e1a2cc205d6d` |
| `SUPPORT.md` | `a09f9ae1010c0f4761a7f798b490ae960e6dbb7867e86a4ab6ea89ecf9d44c04` |
| `deny.toml` | `dabb6b7126861c1a2846241484d07844e86db95392373b603c8616e828863a6b` |
| `docs/WORKFLOWS.md` | `45cf8e52f0d8d5fe56653e1eae6777e18eeb31daaf625bc21c78df1d6c21f926` |
| `rust-toolchain.toml` | `7dbcd3c9663e0120a47fdc32fa56cdcc3dbc075e3b8a1a8377036a9af1dec578` |
| `scripts/coverage_declaration_only.txt` | `073bcfa0066dbb08cf896ed8c0b8354bb9ac6aad27b11e95d04242217bc12e58` |
| `scripts/coverage_gate.py` | `4f99bdb02eb7cb2612c04e8b5cbc75d080ee3717ab74554f966431bf6f8e7bbf` |
| `scripts/export_clean.py` | `85544f53d1fabb69bf9855f95f6dcc3ba1bde08266c6545cab0e6a018169986a` |
| `scripts/oss-ci.sh` | `9a024dcf394c9bce4809637084e30a90280038bfeda73e9f447d1c02e085e8f6` |
| `src/ast/mod.rs` | `46e43b7eca312d813b931e0a9ffbef1c4b2157cc20da3d1f68a436af5dc22ece` |
| `src/ast/syntax.rs` | `008fb8b79ddf7960343ddb012cebbc83b9ed70957199061463bc71976d37efcf` |
| `src/error.rs` | `2f5c4808c3557a5d2f15c08fffa057b6c9b213e6197d78907c00c2a85d03633e` |
| `src/lib.rs` | `265451df5894ff90cde9ce24e7d820adf0e50b539eb87f51363a2a6c4ed5308a` |
| `src/parser.rs` | `d5911fe59b662e137dfbcad04d0bdf0cefc8ea34f9572c2a754eeb5399d0a46d` |
| `tests/acceptance_matrix.rs` | `d05f45053a9da89d58f606f88b3e3d5b3ffcaa9e24fe45679191bb42876fff19` |
| `tests/adversarial.rs` | `68f7c3c44195125cadd97e0dca057e6392bc4aeb1a0f546ef141dd473d216d73` |
| `tests/corpus.rs` | `c34aaac9e8444afb302921fa8985152de886207dc6b0382a372216df7a4cd308` |
| `tests/corpus/MANIFEST.sha256` | `147d2a86b49c2728770dd00dca15b76246f34e70aa661d38101da5fb23b3f1e9` |
| `tests/corpus/parser_cases.jsonl` | `c46f929539e770a77871a8849b345bdd3883127e843da03d1bc0d4f8fc2f8d75` |
