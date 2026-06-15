# callimachus-adapter-docs

A [Callimachus](https://github.com/thehammer/callimachus) adapter for Apple
DocC JSON documentation. Converts Apple's `.json` documentation archive format
into a structured Callimachus index with a rich entity taxonomy and four edge
kinds.

## What this adapter does

Given a directory of Apple DocC JSON files (the layout produced by
`fetch-apple-docs.py --format json`), the adapter:

- **Discovers** every `.json` file recursively (one `DiscoveredSource` per file).
- **Chunks** each file into a page-grain chunk (full rendered markdown) and one
  section-grain chunk per `primaryContentSections` entry with `kind == "content"`.
- **Extracts structured entities and edges** from each DocC page (no LLM required):
  - **Entities** — one per page, with kind mapped from `metadata.symbolKind` /
    `roleHeading`: `class`, `struct`, `enum`, `protocol`, `method`, `property`,
    `initializer`, `enum_case`, `notification`, `typealias`, `constant`,
    `docs_topic`.
  - **`inherits_from`** edges from `relationships[type == "inheritsFrom"]`.
  - **`conforms_to`** edges from `relationships[type == "conformsTo"]`.
  - **`references_type`** edges from declaration token `typeIdentifier` tokens
    (de-duplicated per page).
  - **`member_of`** edges from `topicSections[].identifiers[]` (child → parent).
  - Availability text injected into entity descriptions (macOS introduced /
    deprecated versions).
- **Summarizes** at section and page depth via LLM (optional; not required for
  structure extraction).

## Dependency on `callimachus-adapter-contract`

This crate depends on
[`callimachus-adapter-contract`](https://github.com/thehammer/callimachus),
the thin seam crate that defines `SourceAdapter`, `AdapterRegistry`, and the
plain-data type closure (`Chunk`, `Entity`, `Edge`, `Location`, …).

The dependency is **pinned by git rev** rather than a semver range because the
contract crate is pre-1.0 and its API is not yet stable:

```toml
callimachus-adapter-contract = {
    git = "https://github.com/thehammer/callimachus",
    rev = "3d557436147be45b28abe9f0f4b4a4d1a418133e"
}
```

When the contract stabilises and publishes to crates.io, the dependency will
move to a semver range. Until then, bumping the pinned rev is the upgrade path.

The adapter also depends on `callimachus-llm` (same rev) because the
`summarize` pass uses `LlmProvider` and `CompletionRequest` directly.

**The library crate does not depend on `callimachus-core`** (the engine with
`rusqlite` and storage). Core enters only as an example/dev-dependency for the
composing binary.

## Running the tests

```bash
cargo test
```

The test suite lives in `tests/adapter_smoke.rs` and exercises discover →
chunk → extract_structure against three handwritten DocC JSON fixtures
(`tests/fixtures/AppKit/`).  No network access, no LLM, no API key needed.

## Composing a binary

The `examples/index_docs.rs` binary demonstrates the full open-core composition
pattern: it wires `callimachus-core` (engine) to this adapter via
`AdapterRegistry`, then runs the LLM-free Chunk + Structure passes against a
directory of DocC JSON:

```bash
cargo run --example index_docs -- <docc-json-dir> <out.pinakes>

# Example using the test fixtures:
cargo run --example index_docs -- tests/fixtures /tmp/appkit.pinakes
```

The binary exits 0 if the resulting `.pinakes` file contains at least one
entity and one edge; exits non-zero on an empty index (broken adapter or wrong
input path).

### Composition pattern

```rust
use std::sync::Arc;
use callimachus_adapter_contract::AdapterRegistry;
use callimachus_core::{Corpus, IndexOptions, IndexPipeline, Pass, SqliteBackend, StorageBackend};
use callimachus_llm::DryRunProvider;

// Registry — compose the adapter at build time, looked up by kind string.
let mut registry = AdapterRegistry::new();
registry.register(Arc::new(callimachus_adapter_docs::DocsAdapter::with_root(source_dir)));

let adapter = registry.get("docs").expect("registered above");

// Engine.
let db: Arc<dyn StorageBackend> = Arc::new(SqliteBackend::open(pinakes_path)?);
let llm: Arc<dyn callimachus_llm::LlmProvider> = Arc::new(DryRunProvider::new());

let pipeline = IndexPipeline { db: Arc::clone(&db), adapter, llm, embedder: None };
let opts = IndexOptions {
    passes: vec![Pass::Chunk, Pass::Structure],
    ..IndexOptions::default()
};
let result = pipeline.run(&corpus, opts).await?;
```

## This repo as a template

This repository is the **first external adapter** for Callimachus and is
intended as the template future adapters copy:

1. Depend on `callimachus-adapter-contract` pinned by git rev (not a path dep).
2. Implement `SourceAdapter` — `kind()`, `discover()`, `chunk()`,
   `extract_structure()`.
3. Provide an `examples/` binary that wires `callimachus-core` + the adapter
   via `AdapterRegistry` to prove end-to-end composition.
4. Keep `callimachus-core` out of normal `[dependencies]` — only in
   `[dev-dependencies]` for the example.

The open-core seam ensures that adapter authors don't need access to the
private `callimachus-core` source to ship; the contract crate is the only
build-time dependency.
