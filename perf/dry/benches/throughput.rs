use std::{hint::black_box, sync::Arc};

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use dry::{
    apply_default_aggressive_ruleset,
    apply_default_generic_ruleset,
    apply_default_long_ruleset,
    apply_default_short_ruleset,
    short_running_config,
    virtual_run_test,
};
use lsf_cov::ipc::SharedMemHandle;
use lsf_engine::{
    BinaryBlob,
    DynamicCorpus,
    Engine,
    FastProbabilisticMABScheduler,
    GreedyCoverage,
    InMemory,
    SchedulerBatcher,
    SeedDirReader,
};
use lsf_feedback::{TestableEntry, mab::MABBody};
use rand::{SeedableRng, rngs::SmallRng};

fn setup_engine_short() -> (Engine, SharedMemHandle, SmallRng) {
    let config = short_running_config();
    let scheduler_body = Arc::new(MABBody::new().with_config(config));
    let scheduler = SchedulerBatcher::new(Box::new(FastProbabilisticMABScheduler::new(
        scheduler_body.clone(),
    )));

    let corpus_handler = InMemory::new();

    let shmem_queue = SharedMemHandle::new(4, 2_usize.pow(10));

    let mut engine = Engine::new(
        Box::new(scheduler),
        Box::new(corpus_handler),
        Box::new(GreedyCoverage::new(2_usize.pow(10))),
        vec![],
        shmem_queue.clone(),
        vec![scheduler_body],
        42,
    )
    .silent();

    apply_default_short_ruleset(&mut engine);

    engine.populate(vec![Box::new(SeedDirReader::new("../../seeds".into()))]);

    let snapshot = engine.snapshot();
    engine.clear();
    let mut rng = SmallRng::seed_from_u64(42);

    // warmup
    for entry in snapshot {
        virtual_run_test(
            TestableEntry::new(entry.raw),
            &mut engine,
            &shmem_queue,
            &mut rng,
        );
    }

    (engine, shmem_queue, rng)
}

fn setup_engine_long(strat: impl Fn(&mut Engine)) -> (Engine, SharedMemHandle, SmallRng) {
    let scheduler_body = Arc::new(MABBody::new());
    let scheduler = SchedulerBatcher::new(Box::new(FastProbabilisticMABScheduler::new(
        scheduler_body.clone(),
    )));

    let temp_dir = std::env::temp_dir().join("lsf_fuzz_bench_out");
    let _ = std::fs::create_dir_all(&temp_dir);
    let corpus_handler =
        DynamicCorpus::new(Box::new(BinaryBlob::new(temp_dir.to_str().unwrap().into())));

    let shmem_queue = SharedMemHandle::new(4, 2_usize.pow(10));

    let mut engine = Engine::new(
        Box::new(scheduler),
        Box::new(corpus_handler),
        Box::new(GreedyCoverage::new(2_usize.pow(10))),
        vec![],
        shmem_queue.clone(),
        vec![scheduler_body],
        42,
    )
    .silent();

    strat(&mut engine);

    engine.populate(vec![Box::new(SeedDirReader::new("../../seeds".into()))]);

    let snapshot = engine.snapshot();
    engine.clear();
    let mut rng = SmallRng::seed_from_u64(42);

    // warmup
    for entry in snapshot {
        virtual_run_test(
            TestableEntry::new(entry.raw),
            &mut engine,
            &shmem_queue,
            &mut rng,
        );
    }

    (engine, shmem_queue, rng)
}

fn bench_fuzzer_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("fuzzer_throughput");
    let batch_size = 64;

    group.throughput(Throughput::Elements(batch_size as u64));

    group.measurement_time(std::time::Duration::from_secs(10));

    // mutate + schedueler only
    let (mut engine_mut, _, _) = setup_engine_short();
    group.bench_function("mutate_only_short", |b| {
        b.iter(|| {
            let batch = engine_mut.mutate_batch(batch_size);
            black_box(batch);
        })
    });

    drop(engine_mut);

    // full engien (chore, corpus, ...)
    let (mut engine_full, shmem_queue, mut rng) = setup_engine_short();
    // let mut epoch: u64 = 0;

    group.bench_function("engine_full_short", |b| {
        b.iter(|| {
            let mut batch = engine_full.mutate_batch(batch_size);

            for item in batch.drain(..) {
                virtual_run_test(item, &mut engine_full, &shmem_queue, &mut rng);
            }

            // no gc for short runs
            // epoch += 1;
            // if epoch.is_multiple_of(1000) {
            //     engine_full.chore();
            // }
        })
    });

    drop(engine_full);
    drop(shmem_queue);

    let (mut engine_mut, _, _) = setup_engine_long(apply_default_long_ruleset);
    group.bench_function("mutate_only_long", |b| {
        b.iter(|| {
            let batch = engine_mut.mutate_batch(batch_size);
            black_box(batch);
        })
    });

    drop(engine_mut);

    // full engien (chore, corpus, ...)
    let (mut engine_full, shmem_queue, mut rng) = setup_engine_long(apply_default_long_ruleset);
    let mut epoch: u64 = 0;

    group.bench_function("engine_full_long_aggressive", |b| {
        b.iter(|| {
            let mut batch = engine_full.mutate_batch(batch_size);

            for item in batch.drain(..) {
                virtual_run_test(item, &mut engine_full, &shmem_queue, &mut rng);
            }

            epoch += 1;
            if epoch.is_multiple_of(2000) {
                engine_full.chore();
            }
        })
    });

    drop(engine_full);
    drop(shmem_queue);

    let (mut engine_mut, _, _) = setup_engine_long(apply_default_aggressive_ruleset);
    group.bench_function("mutate_only_long_aggressive", |b| {
        b.iter(|| {
            let batch = engine_mut.mutate_batch(batch_size);
            black_box(batch);
        })
    });

    drop(engine_mut);

    // full engien (chore, corpus, ...)
    let (mut engine_full, shmem_queue, mut rng) =
        setup_engine_long(apply_default_aggressive_ruleset);
    let mut epoch: u64 = 0;

    group.bench_function("engine_full_long_generic", |b| {
        b.iter(|| {
            let mut batch = engine_full.mutate_batch(batch_size);

            for item in batch.drain(..) {
                virtual_run_test(item, &mut engine_full, &shmem_queue, &mut rng);
            }

            epoch += 1;
            if epoch.is_multiple_of(2000) {
                engine_full.chore();
            }
        })
    });

    drop(engine_full);
    drop(shmem_queue);

    let (mut engine_mut, _, _) = setup_engine_long(apply_default_generic_ruleset);
    group.bench_function("mutate_only_long_generic", |b| {
        b.iter(|| {
            let batch = engine_mut.mutate_batch(batch_size);
            black_box(batch);
        })
    });

    drop(engine_mut);

    // full engien (chore, corpus, ...)
    let (mut engine_full, shmem_queue, mut rng) = setup_engine_long(apply_default_generic_ruleset);
    let mut epoch: u64 = 0;

    group.bench_function("engine_full_long_generic", |b| {
        b.iter(|| {
            let mut batch = engine_full.mutate_batch(batch_size);

            for item in batch.drain(..) {
                virtual_run_test(item, &mut engine_full, &shmem_queue, &mut rng);
            }

            epoch += 1;
            if epoch.is_multiple_of(2000) {
                engine_full.chore();
            }
        })
    });

    drop(engine_full);
    drop(shmem_queue);

    group.finish();
}

criterion_group!(benches, bench_fuzzer_throughput);
criterion_main!(benches);
