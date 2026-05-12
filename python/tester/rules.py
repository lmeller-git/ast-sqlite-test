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
        body,
        [
            engine.StrategyBuilder.hoist_expr(),
            engine.StrategyBuilder.splice_out(),
            engine.StrategyBuilder.tree_mutate_stmt(engine.TreeMutatorOperation.null_random()),
            engine.StrategyBuilder.tree_mutate_expr(engine.TreeMutatorOperation.null_random()),
        ],
        1,
    )


def make_ruleset_increase(body: engine.MABBody) -> engine.StrategyBuilder:
    return engine.StrategyBuilder.ucb1(
        body,
        [engine.StrategyBuilder.recursive_expand_expr(), engine.StrategyBuilder.splice_in()],
        1,
    )


def make_ruleset_specific(body: engine.MABBody) -> engine.StrategyBuilder:
    return engine.StrategyBuilder.ucb1(
        body,
        [
            engine.StrategyBuilder.type_cast(),
            engine.StrategyBuilder.sub_query(),
            engine.StrategyBuilder.set_ops(),
        ],
        1,
    )


def make_ruleset_semantic(body: engine.MABBody) -> engine.StrategyBuilder:
    return engine.StrategyBuilder.ucb1(
        body, [engine.StrategyBuilder.num_bounds(), engine.StrategyBuilder.op_flip()], 1
    )


def make_ruleset_shuffle(body: engine.MABBody) -> engine.StrategyBuilder:
    return engine.StrategyBuilder.ucb1(
        body,
        [
            engine.StrategyBuilder.tree_mutate_stmt(engine.TreeMutatorOperation.shuffle_two()),
            engine.StrategyBuilder.tree_mutate_expr(engine.TreeMutatorOperation.shuffle_two()),
            engine.StrategyBuilder.expr_shuffle(),
            engine.StrategyBuilder.relation_shuffle(),
        ],
        1,
    )


def make_full_ruleset(
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
            make_ruleset_specific(havoc_rule_scheduler_body),
            make_ruleset_semantic(sem_rule_scheduler_body),
            make_ruleset_shuffle(struct_rule_scheduler_body),
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
            make_ruleset_specific(havoc_rule_scheduler_body),
            make_ruleset_semantic(sem_rule_scheduler_body),
            make_ruleset_shuffle(struct_rule_scheduler_body),
        ],
        2,
    )


def make_generic_ruleset(
    rule_scheduler_body: engine.MABBody,
    shuffle_rule_scheduler_body: engine.MABBody,
    sem_rule_scheduler_body: engine.MABBody,
    generator_body: engine.MABBody,
    reducer_body: engine.MABBody,
    increaser_body: engine.MABBody,
) -> engine.StrategyBuilder:
    return engine.StrategyBuilder.ucb1(
        rule_scheduler_body,
        [
            make_ruleset_reduce(reducer_body),
            make_ruleset_generate(generator_body),
            make_ruleset_increase(increaser_body),
            make_ruleset_semantic(sem_rule_scheduler_body),
            make_ruleset_shuffle(shuffle_rule_scheduler_body),
            engine.StrategyBuilder.noop()
        ],
        2,
    )


# might not want to share bodies here
def make_aggressive_ruleset(
    outer_body: engine.MABBody,
    rule_scheduler_body: engine.MABBody,
    shuffle_rule_scheduler_body: engine.MABBody,
    sem_rule_scheduler_body: engine.MABBody,
    generator_body: engine.MABBody,
    reducer_body: engine.MABBody,
    increaser_body: engine.MABBody,
) -> engine.StrategyBuilder:
    return engine.StrategyBuilder.ucb1(
        outer_body,
        [
            engine.StrategyBuilder.repeat(
                make_generic_ruleset(
                    rule_scheduler_body,
                    shuffle_rule_scheduler_body,
                    sem_rule_scheduler_body,
                    generator_body,
                    reducer_body,
                    increaser_body,
                ),
                1,
            ),
            engine.StrategyBuilder.repeat(
                make_generic_ruleset(
                    rule_scheduler_body,
                    shuffle_rule_scheduler_body,
                    sem_rule_scheduler_body,
                    generator_body,
                    reducer_body,
                    increaser_body,
                ),
                5,
            ),
            engine.StrategyBuilder.repeat(
                make_generic_ruleset(
                    rule_scheduler_body,
                    shuffle_rule_scheduler_body,
                    sem_rule_scheduler_body,
                    generator_body,
                    reducer_body,
                    increaser_body,
                ),
                15,
            ),
        ],
        1,
    )
