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
    NumericBounds,
    OperatorFlip,
    RecursiveExpandExpr,
    RelShuffle,
    SetOps,
    SpliceIn,
    SpliceOut,
    SubQuery,
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
        make_ruleset_havoc,
        make_ruleset_increase,
        make_ruleset_semantic,
        make_ruleset_structural,
    ];
    for ruleset in strats {
        let body: Arc<MABBody> = MABBody::new().with_config(config.clone()).into();
        engine.add_mab_body(body.clone());
        engine.add_strategy(ruleset(body));
    }
}

pub fn apply_default_long_ruleset(engine: &mut Engine) {
    let strats = [
        make_ruleset_havoc,
        make_ruleset_increase,
        make_ruleset_semantic,
        make_ruleset_structural,
        make_ruleset_reduce,
        make_ruleset_generate,
    ];
    for ruleset in strats {
        let body: Arc<MABBody> = MABBody::new().into();
        engine.add_mab_body(body.clone());
        engine.add_strategy(ruleset(body));
    }
}

pub fn make_ruleset_havoc(body: Arc<MABBody>) -> Box<dyn MutationStrategy> {
    Box::new(MABScheduler::new(
        body,
        [
            Box::new(TreeMutator {
                chance_per_node: 0.2,
                chance_per_field: 0.1,
                operation: lsf_mutate::FieldOperation::NullRandom,
                _phantom: std::marker::PhantomData::<Statement>,
            }) as Box<dyn MutationStrategy>,
            Box::new(TreeMutator {
                chance_per_node: 0.2,
                chance_per_field: 0.1,
                operation: lsf_mutate::FieldOperation::NullRandom,
                _phantom: std::marker::PhantomData::<Expr>,
            }),
            Box::new(TypeCast {
                mutation_chance: 0.5,
            }),
            Box::new(SubQuery {
                mutation_chance: 0.5,
            }),
        ]
        .into_iter(),
        1,
    ))
}

pub fn make_ruleset_structural(body: Arc<MABBody>) -> Box<dyn MutationStrategy> {
    Box::new(MABScheduler::new(
        body,
        [
            Box::new(TreeMutator {
                chance_per_node: 0.2,
                chance_per_field: 0.1,
                operation: lsf_mutate::FieldOperation::ShuffleTwo,
                _phantom: std::marker::PhantomData::<Statement>,
            }) as Box<dyn MutationStrategy>,
            Box::new(TreeMutator {
                chance_per_node: 0.2,
                chance_per_field: 0.1,
                operation: lsf_mutate::FieldOperation::ShuffleTwo,
                _phantom: std::marker::PhantomData::<Expr>,
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
            Box::new(NumericBounds { mutate_chance: 0.5 }) as Box<dyn MutationStrategy>,
            Box::new(OperatorFlip { flip_chance: 0.5 }),
            Box::new(RelShuffle {
                chance_per_node: 0.5,
            }),
            Box::new(ExprShuffle {
                chance_per_node: 0.4,
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
                chance_per_node: 0.5,
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
                chance_per_node: 0.5,
            }) as Box<dyn MutationStrategy>,
            Box::new(SpliceOut { p_extend: 0.5 }),
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
            make_ruleset_havoc(havoc_rule_scheduler_body),
            make_ruleset_generate(generator_body),
            make_ruleset_increase(increaser_body),
            make_ruleset_reduce(reducer_body),
            make_ruleset_semantic(sem_rule_scheduler_body),
            make_ruleset_structural(struct_rule_scheduler_body),
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
            make_ruleset_havoc(havoc_rule_scheduler_body),
            make_ruleset_increase(increaser_body),
            make_ruleset_semantic(sem_rule_scheduler_body),
            make_ruleset_structural(struct_rule_scheduler_body),
        ]
        .into_iter(),
        2,
    ))
}
