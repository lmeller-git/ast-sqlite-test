use std::{sync::Arc, time::Instant};

use dry::{
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

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn main() {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();

    println!("long config: ");
    let (engine, shmem_queue) = setup_engine_long();
    perf_10k_rules(engine, shmem_queue);
    let (engine, shmem_queue) = setup_engine_long();
    perf_10k_engine(engine, shmem_queue, true);

    println!("short config: ");
    let (engine, shmem_queue) = setup_engine_short();
    perf_10k_rules(engine, shmem_queue);
    let (engine, shmem_queue) = setup_engine_short();
    // no gc for short runs
    perf_10k_engine(engine, shmem_queue, false);

    // profile();
}

#[allow(dead_code)]
fn perf_10k_engine(mut engine: Engine, shmem_queue: SharedMemHandle, do_gc: bool) {
    let mut total = 0;
    let mut epoch: usize = 0;
    let batch_size = 64; // roughly batch_size in python
    let mut rng = SmallRng::seed_from_u64(42);

    let now = Instant::now();

    loop {
        let mut batch = engine.mutate_batch(batch_size.min(10000 - total));
        total += batch.members().len();
        if total >= 10000 {
            break;
        }

        for entry in batch.drain(..) {
            virtual_run_test(entry, &mut engine, &shmem_queue, &mut rng);
        }

        epoch += 1;
        if do_gc && epoch.is_multiple_of(2000) {
            engine.chore();
        }
    }

    let duration = now.elapsed();

    let qpm = (total as f64 / duration.as_secs_f64()) * 60.;

    println!("queries per minute from 10k for engine: {:.3}", qpm);
}

#[allow(dead_code)]
fn perf_10k_rules(mut engine: Engine, _shmem_queue: SharedMemHandle) {
    let mut total = 0;
    let batch_size = 64; // roughly batch_size in python

    let now = Instant::now();

    loop {
        let batch = engine.mutate_batch(batch_size.min(10000 - total));
        total += batch.members().len();
        if total >= 10000 {
            break;
        }
    }

    let duration = now.elapsed();

    let qpm = (total as f64 / duration.as_secs_f64()) * 60.;

    println!("queries per minute from 10k for rules: {:.3}", qpm);
}

fn setup_engine_short() -> (Engine, SharedMemHandle) {
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

    (engine, shmem_queue)
}

fn setup_engine_long() -> (Engine, SharedMemHandle) {
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

    apply_default_long_ruleset(&mut engine);
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

    (engine, shmem_queue)
}

#[allow(dead_code)]
fn profile() {
    let scheduler_body = Arc::new(MABBody::new());
    let scheduler = SchedulerBatcher::new(Box::new(FastProbabilisticMABScheduler::new(
        scheduler_body.clone(),
    )));

    let corpus_handler = DynamicCorpus::new(Box::new(BinaryBlob::new("../temp_out".into())));

    let shmem_queue = SharedMemHandle::new(4, 2_usize.pow(14));

    let mut engine = Engine::new(
        Box::new(scheduler),
        Box::new(corpus_handler),
        Box::new(GreedyCoverage::new(2_usize.pow(14))),
        vec![],
        shmem_queue.clone(),
        vec![scheduler_body],
        42,
    );

    apply_default_short_ruleset(&mut engine);

    engine.populate(vec![Box::new(SeedDirReader::new("../../seeds".into()))]);

    // simulate same workflow as in main.py
    let snapshot = engine.snapshot();
    engine.clear();

    let mut rng = SmallRng::seed_from_u64(42);

    for entry in snapshot {
        virtual_run_test(
            TestableEntry::new(entry.raw),
            &mut engine,
            &shmem_queue,
            &mut rng,
        );
    }

    println!("entering loop");
    fuzz_loop(&mut engine, &shmem_queue);

    println!("corpus size: {}", engine.corpus_size())
}

fn fuzz_loop(engine: &mut Engine, token_queue: &SharedMemHandle) {
    let mut rng = SmallRng::seed_from_u64(42);
    let mut epoch: i32 = 0;
    loop {
        let mut batch = engine.mutate_batch(64);
        for item in batch.drain(..) {
            virtual_run_test(item, engine, token_queue, &mut rng);
        }

        let size = engine.corpus_size();
        if epoch % 1000 == 0 {
            engine.chore();
            println!(
                "corpus size: {}, {}% done",
                size,
                epoch as f64 / 2_i32.pow(14) as f64 * 100.
            );
        }

        if epoch >= 2_i32.pow(14) {
            break;
        }
        epoch += 1;
    }
}
