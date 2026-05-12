use std::sync::Arc;

use lsf_core::entry::{Meta, RawEntry};
use lsf_cov::ipc::SharedMemHandle;
use lsf_engine::Engine;
use lsf_feedback::{
    TestableEntry,
    mab::{MABBody, MABConfig},
};
use lsf_mutate::{
    ArbitraryGenerator,
    ExprShuffle,
    HoistExpr,
    MABScheduler,
    MutationStrategy,
    NOOP,
    NumericBounds,
    OperatorFlip,
    Randomly,
    RecursiveExpandExpr,
    RelShuffle,
    Repeat,
    SetOps,
    SpliceIn,
    SpliceOut,
    SubQuery,
    TableGuard,
    TreeMutator,
    TypeCast,
};
use rand::RngExt;
use sqlparser::ast::{Expr, Statement};

pub fn short_running_config() -> MABConfig {
    MABConfig {
        exploration_constant: 0.25,
        max_accepted_syntax_err: 0.1,
        ..Default::default()
    }
}

pub fn virtual_run_test(
    mut test: TestableEntry<RawEntry>,
    engine: &mut Engine,
    token_queue: &SharedMemHandle,
    rng: &mut dyn rand::Rng,
) {
    let Ok(mut token) = token_queue.rx().recv() else {
        return;
    };
    let meta = random_meta(rng);

    let shmem_map = token.as_mut_slice();
    for _ in 0..meta.new_cov_nodes {
        let rd_idx = rng.random_range(..shmem_map.len());
        shmem_map[rd_idx] = 1;
    }

    test.fire_rule_hooks(
        if meta.new_cov_nodes > 0 {
            lsf_feedback::TestOutcome::Accepted(lsf_feedback::AcceptanceReason::CovIncrease(
                meta.new_cov_nodes,
            ))
        } else {
            lsf_feedback::TestOutcome::Accepted(lsf_feedback::AcceptanceReason::IsDiverse)
        },
        &meta,
    );
    engine.commit_test_result(test, meta, token);
}

pub fn random_meta(rng: &mut dyn rand::Rng) -> Meta {
    Meta {
        triggers_bug: rng.random_bool(0.01),
        is_valid_syntax: rng.random_bool(0.7),
        exec_time: rng.random_range(100..u32::MAX / 2),
        new_cov_nodes: rng.random_range(0..5),
        query_size: rng.random_range(10..100),
    }
}

pub fn apply_default_short_ruleset(engine: &mut Engine) {
    let config = short_running_config();
    let strats = [
        make_ruleset_specific,
        make_ruleset_increase,
        make_ruleset_semantic,
        make_ruleset_shuffle,
    ];
    for ruleset in strats {
        let body: Arc<MABBody> = MABBody::new().with_config(config.clone()).into();
        engine.add_mab_body(body.clone());
        engine.add_strategy(ruleset(body));
    }
    engine.add_strategy(Box::new(Randomly::new(Box::new(TableGuard {}), 0.7)));
}

pub fn apply_default_long_ruleset(engine: &mut Engine) {
    let strats = [
        make_ruleset_specific,
        make_ruleset_increase,
        make_ruleset_semantic,
        make_ruleset_shuffle,
        make_ruleset_reduce,
        make_ruleset_generate,
    ];
    for ruleset in strats {
        let body: Arc<MABBody> = MABBody::new().into();
        engine.add_mab_body(body.clone());
        engine.add_strategy(ruleset(body));
    }
    engine.add_strategy(Box::new(Randomly::new(Box::new(TableGuard {}), 0.7)));
}

pub fn apply_default_generic_ruleset(engine: &mut Engine) {
    let havoc_rule_scheduler_body: Arc<MABBody> = MABBody::new().into();
    let rule_scheduler_body: Arc<MABBody> = MABBody::new().into();
    let struct_rule_scheduler_body: Arc<MABBody> = MABBody::new().into();
    let sem_rule_scheduler_body: Arc<MABBody> = MABBody::new().into();
    let generator_body: Arc<MABBody> = MABBody::new().into();
    let reducer_body: Arc<MABBody> = MABBody::new().into();
    let increaser_body: Arc<MABBody> = MABBody::new().into();

    engine.add_mab_body(havoc_rule_scheduler_body.clone());
    engine.add_mab_body(rule_scheduler_body.clone());
    engine.add_mab_body(sem_rule_scheduler_body.clone());
    engine.add_mab_body(struct_rule_scheduler_body.clone());
    engine.add_mab_body(generator_body.clone());
    engine.add_mab_body(increaser_body.clone());
    engine.add_mab_body(reducer_body.clone());

    let strat = make_generic_ruleset(
        rule_scheduler_body,
        struct_rule_scheduler_body,
        sem_rule_scheduler_body,
        generator_body,
        reducer_body,
        increaser_body,
    );

    engine.add_strategy(strat);
    engine.add_strategy(Box::new(Randomly::new(Box::new(TableGuard {}), 0.7)));
}

pub fn apply_default_aggressive_ruleset(engine: &mut Engine) {
    let outer_body: Arc<MABBody> = MABBody::new().into();
    let havoc_rule_scheduler_body: Arc<MABBody> = MABBody::new().into();
    let rule_scheduler_body: Arc<MABBody> = MABBody::new().into();
    let struct_rule_scheduler_body: Arc<MABBody> = MABBody::new().into();
    let sem_rule_scheduler_body: Arc<MABBody> = MABBody::new().into();
    let generator_body: Arc<MABBody> = MABBody::new().into();
    let reducer_body: Arc<MABBody> = MABBody::new().into();
    let increaser_body: Arc<MABBody> = MABBody::new().into();

    engine.add_mab_body(outer_body.clone());
    engine.add_mab_body(havoc_rule_scheduler_body.clone());
    engine.add_mab_body(rule_scheduler_body.clone());
    engine.add_mab_body(sem_rule_scheduler_body.clone());
    engine.add_mab_body(struct_rule_scheduler_body.clone());
    engine.add_mab_body(generator_body.clone());
    engine.add_mab_body(increaser_body.clone());
    engine.add_mab_body(reducer_body.clone());

    let strat = make_aggressive_ruleset(
        outer_body,
        rule_scheduler_body,
        struct_rule_scheduler_body,
        sem_rule_scheduler_body,
        generator_body,
        reducer_body,
        increaser_body,
    );

    engine.add_strategy(strat);
    engine.add_strategy(Box::new(Randomly::new(Box::new(TableGuard {}), 0.7)));
}

pub fn make_ruleset_specific(body: Arc<MABBody>) -> Box<dyn MutationStrategy> {
    Box::new(MABScheduler::new(
        body,
        [
            Box::new(TypeCast {
                mutation_chance: 0.3,
            }) as Box<dyn MutationStrategy>,
            Box::new(SubQuery {
                mutation_chance: 0.3,
            }),
            Box::new(SetOps {}),
        ]
        .into_iter(),
        1,
    ))
}

pub fn make_ruleset_semantic(body: Arc<MABBody>) -> Box<dyn MutationStrategy> {
    Box::new(MABScheduler::new(
        body,
        [
            Box::new(NumericBounds { mutate_chance: 0.3 }) as Box<dyn MutationStrategy>,
            Box::new(OperatorFlip { flip_chance: 0.3 }),
        ]
        .into_iter(),
        2,
    ))
}

pub fn make_ruleset_shuffle(body: Arc<MABBody>) -> Box<dyn MutationStrategy> {
    Box::new(MABScheduler::new(
        body,
        [
            Box::new(TreeMutator {
                chance_per_node: 0.3,
                chance_per_field: 0.1,
                operation: lsf_mutate::FieldOperation::ShuffleTwo,
                _phantom: std::marker::PhantomData::<Statement>,
            }) as Box<dyn MutationStrategy>,
            Box::new(TreeMutator {
                chance_per_node: 0.3,
                chance_per_field: 0.1,
                operation: lsf_mutate::FieldOperation::ShuffleTwo,
                _phantom: std::marker::PhantomData::<Expr>,
            }),
            Box::new(RelShuffle {
                chance_per_node: 0.3,
            }),
            Box::new(ExprShuffle {
                chance_per_node: 0.3,
            }),
        ]
        .into_iter(),
        2,
    ))
}

pub fn make_ruleset_increase(body: Arc<MABBody>) -> Box<dyn MutationStrategy> {
    Box::new(MABScheduler::new(
        body,
        [
            Box::new(RecursiveExpandExpr {
                max_depth: 3,
                chance_per_node: 0.2,
                chance_per_level: 0.5,
            }) as Box<dyn MutationStrategy>,
            Box::new(SpliceIn { p_extend: 0.5 }),
        ]
        .into_iter(),
        1,
    ))
}

pub fn make_ruleset_reduce(body: Arc<MABBody>) -> Box<dyn MutationStrategy> {
    Box::new(MABScheduler::new(
        body,
        [
            Box::new(HoistExpr {
                chance_per_node: 0.2,
            }) as Box<dyn MutationStrategy>,
            Box::new(SpliceOut { p_extend: 0.5 }),
            Box::new(TreeMutator {
                chance_per_node: 0.3,
                chance_per_field: 0.1,
                operation: lsf_mutate::FieldOperation::NullRandom,
                _phantom: std::marker::PhantomData::<Statement>,
            }) as Box<dyn MutationStrategy>,
            Box::new(TreeMutator {
                chance_per_node: 0.3,
                chance_per_field: 0.1,
                operation: lsf_mutate::FieldOperation::NullRandom,
                _phantom: std::marker::PhantomData::<Expr>,
            }),
        ]
        .into_iter(),
        1,
    ))
}

pub fn make_ruleset_generate(body: Arc<MABBody>) -> Box<dyn MutationStrategy> {
    Box::new(MABScheduler::new(
        body,
        [
            Box::new(ArbitraryGenerator::<Expr>::new()) as Box<dyn MutationStrategy>,
            Box::new(ArbitraryGenerator::<Statement>::new()),
        ]
        .into_iter(),
        2,
    ))
}

pub fn make_longrunning_ruleset(
    rule_scheduler_body: Arc<MABBody>,
    havoc_rule_scheduler_body: Arc<MABBody>,
    struct_rule_scheduler_body: Arc<MABBody>,
    sem_rule_scheduler_body: Arc<MABBody>,
    generator_body: Arc<MABBody>,
    reducer_body: Arc<MABBody>,
    increaser_body: Arc<MABBody>,
) -> Box<dyn MutationStrategy> {
    Box::new(MABScheduler::new(
        rule_scheduler_body,
        [
            make_ruleset_specific(havoc_rule_scheduler_body),
            make_ruleset_generate(generator_body),
            make_ruleset_increase(increaser_body),
            make_ruleset_reduce(reducer_body),
            make_ruleset_semantic(sem_rule_scheduler_body),
            make_ruleset_shuffle(struct_rule_scheduler_body),
        ]
        .into_iter(),
        2,
    ))
}

pub fn make_shortrunning_ruleset(
    rule_scheduler_body: Arc<MABBody>,
    havoc_rule_scheduler_body: Arc<MABBody>,
    struct_rule_scheduler_body: Arc<MABBody>,
    sem_rule_scheduler_body: Arc<MABBody>,
    increaser_body: Arc<MABBody>,
) -> Box<dyn MutationStrategy> {
    Box::new(MABScheduler::new(
        rule_scheduler_body,
        [
            make_ruleset_specific(havoc_rule_scheduler_body),
            make_ruleset_increase(increaser_body),
            make_ruleset_semantic(sem_rule_scheduler_body),
            make_ruleset_shuffle(struct_rule_scheduler_body),
        ]
        .into_iter(),
        2,
    ))
}

pub fn make_generic_ruleset(
    rule_scheduler_body: Arc<MABBody>,
    struct_rule_scheduler_body: Arc<MABBody>,
    sem_rule_scheduler_body: Arc<MABBody>,
    generator_body: Arc<MABBody>,
    reducer_body: Arc<MABBody>,
    increaser_body: Arc<MABBody>,
) -> Box<dyn MutationStrategy> {
    Box::new(MABScheduler::new(
        rule_scheduler_body,
        [
            make_ruleset_reduce(reducer_body),
            make_ruleset_generate(generator_body),
            make_ruleset_increase(increaser_body),
            make_ruleset_semantic(sem_rule_scheduler_body),
            make_ruleset_shuffle(struct_rule_scheduler_body),
            Box::new(NOOP),
        ]
        .into_iter(),
        2,
    ))
}

pub fn make_aggressive_ruleset(
    outer_body: Arc<MABBody>,
    rule_scheduler_body: Arc<MABBody>,
    struct_rule_scheduler_body: Arc<MABBody>,
    sem_rule_scheduler_body: Arc<MABBody>,
    generator_body: Arc<MABBody>,
    reducer_body: Arc<MABBody>,
    increaser_body: Arc<MABBody>,
) -> Box<dyn MutationStrategy> {
    Box::new(MABScheduler::new(
        outer_body,
        [
            Box::new(Repeat {
                up_to: 1,
                inner: make_generic_ruleset(
                    rule_scheduler_body.clone(),
                    struct_rule_scheduler_body.clone(),
                    sem_rule_scheduler_body.clone(),
                    generator_body.clone(),
                    reducer_body.clone(),
                    increaser_body.clone(),
                ),
            }) as Box<dyn MutationStrategy>,
            Box::new(Repeat {
                up_to: 5,
                inner: make_generic_ruleset(
                    rule_scheduler_body.clone(),
                    struct_rule_scheduler_body.clone(),
                    sem_rule_scheduler_body.clone(),
                    generator_body.clone(),
                    reducer_body.clone(),
                    increaser_body.clone(),
                ),
            }),
            Box::new(Repeat {
                up_to: 15,
                inner: make_generic_ruleset(
                    rule_scheduler_body.clone(),
                    struct_rule_scheduler_body.clone(),
                    sem_rule_scheduler_body.clone(),
                    generator_body.clone(),
                    reducer_body.clone(),
                    increaser_body.clone(),
                ),
            }),
        ]
        .into_iter(),
        1,
    ))
}
