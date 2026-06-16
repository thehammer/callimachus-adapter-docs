//! Composing example: index a directory of Apple DocC JSON into a Callimachus pinakes.
//!
//! Proves the open-core seam end-to-end: `callimachus-core` (engine) +
//! `callimachus-adapter-docs` (external adapter) composed at build time via
//! `callimachus_adapter_contract::AdapterRegistry`.  No bespoke dispatch;
//! the adapter is looked up by kind string, exactly as the CLI does.
//!
//! Usage:
//!   cargo run --example index_docs -- <docc-json-dir> [<out.pinakes>]
//!
//! Runs LLM-free Chunk + Structure passes only, so no API key is needed.
//! Exits 0 if the resulting pinakes contains at least one entity and one edge;
//! exits non-zero otherwise (empty index = broken adapter or wrong input).

use std::path::PathBuf;
use std::sync::Arc;

use callimachus_adapter_contract::AdapterRegistry;
use callimachus_core::{Corpus, IndexOptions, IndexPipeline, Pass, SqliteBackend, StorageBackend};
use callimachus_llm::DryRunProvider;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ── Args ──────────────────────────────────────────────────────────────────

    let mut args = std::env::args().skip(1);
    let source_dir = args.next().unwrap_or_else(|| "tests/fixtures".to_string());
    let pinakes_path = args
        .next()
        .unwrap_or_else(|| "/tmp/apple-docs-example.pinakes".to_string());

    println!("Source:  {source_dir}");
    println!("Pinakes: {pinakes_path}");

    // ── Storage ───────────────────────────────────────────────────────────────

    // Remove any stale pinakes file so corpus_insert starts fresh each run.
    let pinakes_pb = PathBuf::from(&pinakes_path);
    if pinakes_pb.exists() {
        std::fs::remove_file(&pinakes_pb)?;
    }

    let db: Arc<dyn StorageBackend> = Arc::new(SqliteBackend::open(pinakes_pb.as_path())?);

    // ── Adapter registry ──────────────────────────────────────────────────────

    let mut registry = AdapterRegistry::new();
    // Use `with_root` so the Structure pass can reconstruct the JSON file path
    // from each chunk's location URI (docs/<Framework>/<Slug>).
    registry.register(Arc::new(callimachus_adapter_docs::DocsAdapter::with_root(
        &source_dir,
    )));

    let adapter = registry
        .get("docs")
        .expect("docs adapter was just registered; this must never fail");

    println!("Registered kinds: {:?}", registry.list());

    // ── Corpus row ────────────────────────────────────────────────────────────

    let corpus = Corpus::new(
        "apple-docs".to_string(),
        "Apple Docs".to_string(),
        "docs".to_string(),
        source_dir.clone(),
    );
    db.corpus_insert(&corpus)?;

    // ── LLM provider (dry run — no API key needed) ────────────────────────────

    let llm: Arc<dyn callimachus_llm::LlmProvider> = Arc::new(DryRunProvider::new());

    // ── Pipeline (Chunk + Structure only; no LLM passes) ─────────────────────

    let pipeline = IndexPipeline {
        db: Arc::clone(&db),
        adapter,
        llm,
        embedder: None,
    };

    let opts = IndexOptions {
        passes: vec![Pass::Chunk, Pass::Structure],
        ..IndexOptions::default()
    };

    println!("\nRunning Chunk + Structure passes…");
    let result = pipeline.run(&corpus, opts).await?;

    // ── Report ────────────────────────────────────────────────────────────────

    println!();
    println!("Chunks:   {}", result.total_chunks);
    println!("Entities: {}", result.total_entities);
    println!("Edges:    {}", result.total_edges);

    // ── Assert populated index ────────────────────────────────────────────────

    if result.total_entities == 0 {
        eprintln!(
            "\nERROR: index is empty (0 entities). Check that the source directory contains valid DocC JSON files."
        );
        std::process::exit(1);
    }

    if result.total_edges == 0 {
        eprintln!(
            "\nERROR: index has no edges (0 edges). The Structure pass should produce inherits_from / conforms_to / member_of edges for DocC JSON with relationships."
        );
        std::process::exit(1);
    }

    println!("\nOK — populated index produced via AdapterRegistry (kind=\"docs\").");
    Ok(())
}
