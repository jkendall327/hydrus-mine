//! Measure write-path throughput by generating a synthetic library.
//!
//! `cargo run --release -p hydrus-store --example synth_throughput -- [files] [tags_per_file]`

use std::time::Instant;

use hydrus_store::Store;
use hydrus_store::synth::{SynthSpec, populate};

fn main() -> hydrus_store::Result<()> {
    let mut args = std::env::args()
        .skip(1)
        .map(|a| a.parse::<u32>().expect("a number"));
    let files = args.next().unwrap_or(100_000);
    let tags_per_file = args.next().unwrap_or(20);
    let dir = tempfile::tempdir()?;
    let store = Store::open(dir.path())?;
    let spec = SynthSpec {
        files,
        tags: (files / 4).max(100),
        tags_per_file,
        siblings: files / 50,
        parents: files / 50,
        ..SynthSpec::default()
    };
    let started = Instant::now();
    let lib = populate(&store, spec)?;
    let elapsed = started.elapsed().as_secs_f64();
    let mappings = f64::from(files) * f64::from(tags_per_file);
    println!(
        "{} files, {} tags, ~{mappings:.0} mappings in {elapsed:.1}s: {:.0} files/s, {:.0} mappings/s",
        lib.hashes.len(),
        lib.tags.len(),
        f64::from(files) / elapsed,
        mappings / elapsed
    );
    let started = Instant::now();
    store.write(|ctx| hydrus_store::counts::rebuild_all(ctx.conn()))?;
    println!(
        "rebuilding every count: {:.1}s",
        started.elapsed().as_secs_f64()
    );
    let size = std::fs::metadata(dir.path().join(hydrus_store::store::DB_FILE_NAME))?.len();
    println!("database: {:.0} MiB", size as f64 / f64::from(1u32 << 20));
    Ok(())
}
