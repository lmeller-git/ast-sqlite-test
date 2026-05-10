use std::sync::Arc;

use dry::{apply_default_ruleset, virtual_run_test};
use lsf_cov::ipc::SharedMemHandle;
use lsf_engine::{
    BinaryBlob,
    DynamicCorpus,
    Engine,
    FastProbabilisticMABScheduler,
    GreedyCoverage,
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

    apply_default_ruleset(&mut engine);

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
        let mut batch = engine.mutate_batch(16);
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
