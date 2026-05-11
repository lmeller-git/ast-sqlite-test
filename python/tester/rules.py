from lib_sf import engine


def make_ruleset_generate(body: engine.MABBody) -> engine.StrategyBuilder:
    return engine.StrategyBuilder.ucb1(
        body,
        [
            engine.StrategyBuilder.arbitrary_expr_generator(),
            engine.StrategyBuilder.arbitrary_stmt_generator(),
        ],
        1,
    )


def make_ruleset_reduce(body: engine.MABBody) -> engine.StrategyBuilder:
    return engine.StrategyBuilder.ucb1(
        body, [engine.StrategyBuilder.hoist_expr(), engine.StrategyBuilder.splice_out()], 1
    )


def make_ruleset_increase(body: engine.MABBody) -> engine.StrategyBuilder:
    return engine.StrategyBuilder.ucb1(
        body,
        [engine.StrategyBuilder.recursive_expand_expr(), engine.StrategyBuilder.splice_in()],
        1,
    )


def make_ruleset_havoc(body: engine.MABBody) -> engine.StrategyBuilder:
    return engine.StrategyBuilder.ucb1(
        body,
        [
            engine.StrategyBuilder.tree_mutate_stmt(engine.TreeMutatorOperation.null_random()),
            engine.StrategyBuilder.tree_mutate_expr(engine.TreeMutatorOperation.null_random()),
            engine.StrategyBuilder.type_cast(),
            engine.StrategyBuilder.sub_query(),
        ],
        1,
    )


def make_ruleset_semantic(body: engine.MABBody) -> engine.StrategyBuilder:
    return engine.StrategyBuilder.ucb1(
        body,
        [
            engine.StrategyBuilder.num_bounds(),
            engine.StrategyBuilder.op_flip(),
            engine.StrategyBuilder.null_inject(),
            engine.StrategyBuilder.relation_shuffle(),
            engine.StrategyBuilder.expr_shuffle(),
        ],
        2,
    )


def make_ruleset_structural(body: engine.MABBody) -> engine.StrategyBuilder:
    return engine.StrategyBuilder.ucb1(
        body,
        [
            engine.StrategyBuilder.tree_mutate_stmt(engine.TreeMutatorOperation.shuffle_two()),
            engine.StrategyBuilder.tree_mutate_expr(engine.TreeMutatorOperation.shuffle_two()),
            engine.StrategyBuilder.set_ops(),
        ],
        1,
    )


def make_longrunning_ruleset(
    rule_scheduler_body: engine.MABBody,
    havoc_rule_scheduler_body: engine.MABBody,
    struct_rule_scheduler_body: engine.MABBody,
    sem_rule_scheduler_body: engine.MABBody,
    generator_body: engine.MABBody,
    reducer_body: engine.MABBody,
    increaser_body: engine.MABBody,
) -> engine.StrategyBuilder:
    return engine.StrategyBuilder.ucb1(
        rule_scheduler_body,
        [
            make_ruleset_reduce(reducer_body),
            make_ruleset_increase(increaser_body),
            make_ruleset_generate(generator_body),
            make_ruleset_havoc(havoc_rule_scheduler_body),
            make_ruleset_semantic(sem_rule_scheduler_body),
            make_ruleset_structural(struct_rule_scheduler_body),
        ],
        2,
    )


def make_shortrunning_ruleset(
    rule_scheduler_body: engine.MABBody,
    havoc_rule_scheduler_body: engine.MABBody,
    struct_rule_scheduler_body: engine.MABBody,
    sem_rule_scheduler_body: engine.MABBody,
    increaser_body: engine.MABBody,
) -> engine.StrategyBuilder:
    return engine.StrategyBuilder.ucb1(
        rule_scheduler_body,
        [
            make_ruleset_increase(increaser_body),
            make_ruleset_havoc(havoc_rule_scheduler_body),
            make_ruleset_semantic(sem_rule_scheduler_body),
            make_ruleset_structural(struct_rule_scheduler_body),
        ],
        2,
    )
