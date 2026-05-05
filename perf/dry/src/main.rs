use std::sync::Arc;

use lsf_core::entry::{Meta, RawEntry};
use lsf_cov::ipc::SharedMemHandle;
use lsf_engine::{DynamicCorpus, Engine, InMemoryCorpus, ProbabilisticMABScheduler, SeedDirReader};
use lsf_feedback::{TestableEntry, mab::MABBody};
use lsf_mutate::{
    ExprShuffle,
    MutationStrategy,
    NullInject,
    OperatorFlip,
    RelShuffle,
    SpliceIn,
    TreeMutator,
};
use rand::{RngExt, SeedableRng, rngs::SmallRng};
use sqlparser::ast::{Expr, Statement};

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn main() {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();

    let scheduler_body = Arc::new(MABBody::new());
    let scheduler = ProbabilisticMABScheduler::new(scheduler_body.clone());

    let corpus_handler = InMemoryCorpus::new(); //DynamicCorpus::new("../temp_out".into());

    let shmem_queue = Arc::new(SharedMemHandle::new(8, 2_usize.pow(14)));

    let top_level_strategy = Arc::new(MABBody::new());
    let scheduler1 = Arc::new(MABBody::new());
    let scheduler2 = Arc::new(MABBody::new());

    let ruleset = lsf_mutate::MABScheduler::new(
        top_level_strategy.clone(),
        vec![
            Box::new(SpliceIn {}) as Box<dyn MutationStrategy>,
            Box::new(lsf_mutate::MABScheduler::new(
                scheduler1.clone(),
                vec![
                    Box::new(TreeMutator {
                        chance_per_node: 0.3,
                        chance_per_field: 0.5,
                        operation: lsf_mutate::FieldOperation::ShuffleTwo,
                        _phantom: std::marker::PhantomData::<Statement>,
                    }) as Box<dyn MutationStrategy>,
                    Box::new(ExprShuffle {
                        chance_per_node: 0.5,
                    }),
                    Box::new(TreeMutator {
                        chance_per_node: 0.3,
                        chance_per_field: 0.5,
                        operation: lsf_mutate::FieldOperation::ShuffleTwo,
                        _phantom: std::marker::PhantomData::<Expr>,
                    }),
                ]
                .into_iter(),
                2,
            )),
            Box::new(lsf_mutate::MABScheduler::new(
                scheduler2.clone(),
                vec![
                    Box::new(RelShuffle {
                        chance_per_node: 0.5,
                    }) as Box<dyn MutationStrategy>,
                    Box::new(OperatorFlip { flip_chance: 0.5 }),
                    Box::new(NullInject {
                        mutation_chance: 0.3,
                    }),
                    Box::new(TreeMutator {
                        chance_per_node: 0.3,
                        chance_per_field: 0.5,
                        operation: lsf_mutate::FieldOperation::NullRandom,
                        _phantom: std::marker::PhantomData::<Expr>,
                    }),
                ]
                .into_iter(),
                2,
            )),
        ]
        .into_iter(),
        1,
    );

    let mut engine = Engine::new(
        Box::new(scheduler),
        Box::new(corpus_handler),
        vec![Box::new(ruleset)],
        shmem_queue.clone(),
        vec![scheduler_body, top_level_strategy, scheduler1, scheduler2],
        42,
    );

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
}

fn fuzz_loop(engine: &mut Engine, token_queue: &SharedMemHandle) {
    let mut rng = SmallRng::seed_from_u64(42);
    for i in 0..2_usize.pow(11) {
        let mut batch = engine.mutate_batch(16);
        for item in batch.drain(..) {
            virtual_run_test(item, engine, token_queue, &mut rng);
        }

        if i.is_multiple_of(200) {
            println!("{}% done", i as f64 / 2_usize.pow(11) as f64 * 100.);
        }
    }
}

fn virtual_run_test(
    test: TestableEntry<RawEntry>,
    engine: &mut Engine,
    token_queue: &SharedMemHandle,
    rng: &mut dyn rand::Rng,
) {
    #[allow(clippy::never_loop)]
    let token = loop {
        if let Some(token) = token_queue.pop() {
            break token;
        } else {
            panic!("cannot happen in sequential case")
        }
    };
    engine.commit_test_result(test, random_meta(rng), token);
}

fn random_meta(rng: &mut dyn rand::Rng) -> Meta {
    Meta {
        triggers_bug: rng.random_bool(0.01),
        is_valid_syntax: rng.random_bool(0.7),
        exec_time: rng.random_range(100..u32::MAX / 2),
        new_cov_nodes: rng.random_range(1..10),
    }
}
