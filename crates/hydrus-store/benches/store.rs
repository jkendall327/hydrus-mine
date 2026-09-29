//! Store hot paths on a synthetic 20k-file library.
//!
//! `cargo bench -p hydrus-store`

use std::hint::black_box;
use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};

use hydrus_core::HashId;
use hydrus_core::tag::Tag;
use hydrus_store::Store;
use hydrus_store::content::MappingAction;
use hydrus_store::synth::{SynthLibrary, SynthSpec, populate};

struct Fixture {
    _dir: tempfile::TempDir,
    store: Arc<Store>,
    lib: SynthLibrary,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let lib = populate(
        &store,
        SynthSpec {
            files: 20_000,
            tags: 5_000,
            ..SynthSpec::default()
        },
    )
    .unwrap();
    Fixture {
        _dir: dir,
        store,
        lib,
    }
}

fn benches(c: &mut Criterion) {
    let f = fixture();
    let ids: Vec<HashId> = f.lib.hashes.iter().map(|(id, _)| *id).collect();
    let service = f.lib.tag_service;

    // one tag on many files and back, as a bulk tag edit does
    let bulk: Vec<HashId> = ids.iter().copied().step_by(20).collect();
    let tag = f.lib.tags[0]; // the most popular tag, likely in the display graph
    c.bench_function("tag 1000 files, then untag", |b| {
        b.iter(|| {
            for action in [MappingAction::Add, MappingAction::Delete] {
                let files = bulk.clone();
                f.store
                    .write_content(move |w| w.update_mappings(service, &action, tag, &files))
                    .unwrap();
            }
        });
    });

    // many tags on one file, as an import does
    let file = ids[123];
    let new_tags: Vec<_> = f
        .store
        .write(|ctx| {
            (0..30)
                .map(|i| {
                    hydrus_store::master::intern_tag(
                        ctx.conn(),
                        &Tag::from_parts("bench", &format!("t{i}")),
                    )
                })
                .collect::<hydrus_store::Result<Vec<_>>>()
        })
        .unwrap();
    c.bench_function("30 tags on one file, then remove", |b| {
        b.iter(|| {
            let tags = new_tags.clone();
            f.store
                .write_content(move |w| {
                    for action in [MappingAction::Add, MappingAction::Delete] {
                        for &t in &tags {
                            w.update_mappings(service, &action, t, &[file])?;
                        }
                    }
                    Ok(())
                })
                .unwrap();
        });
    });

    // what /get_files/file_metadata does for 100 files
    let page: Vec<HashId> = ids.iter().copied().step_by(200).take(100).collect();
    c.bench_function("load metadata for 100 files", |b| {
        b.iter(|| {
            let snap = f.store.snapshot();
            let batch = f
                .store
                .read(|conn| {
                    hydrus_store::media::load(conn, &snap.services, Some(&snap.display), &page)
                })
                .unwrap();
            black_box(batch);
        });
    });

    let mut group = c.benchmark_group("slow");
    group.sample_size(10);
    group.bench_function("rebuild every count (400k mappings)", |b| {
        b.iter(|| {
            f.store
                .write(|ctx| hydrus_store::counts::rebuild_all(ctx.conn()))
                .unwrap();
        });
    });
    group.finish();
}

criterion_group!(store, benches);
criterion_main!(store);
