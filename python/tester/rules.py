from lib_sf import engine


def make_ruleset_havoc():
    return engine.StrategyBuilder.random_sampler(
        3,
        5,
        [
            engine.StrategyBuilder.scheduled(
                engine.StrategyBuilder.tree_mutate_stmt(engine.TreeMutatorOperation.null_random())
            ),
            engine.StrategyBuilder.scheduled(
                engine.StrategyBuilder.tree_mutate_expr(engine.TreeMutatorOperation.null_random())
            ),
            engine.StrategyBuilder.scheduled(engine.StrategyBuilder.type_cast()),
            engine.StrategyBuilder.scheduled(engine.StrategyBuilder.null_inject()),
            engine.StrategyBuilder.scheduled(engine.StrategyBuilder.op_flip()),
            engine.StrategyBuilder.scheduled(engine.StrategyBuilder.recursive_expand_expr()),
        ],
    )


def make_ruleset_semantic():
    return engine.StrategyBuilder.random_sampler(
        1,
        3,
        [
            engine.StrategyBuilder.scheduled(engine.StrategyBuilder.num_bounds()),
            engine.StrategyBuilder.scheduled(engine.StrategyBuilder.op_flip()),
            engine.StrategyBuilder.scheduled(engine.StrategyBuilder.null_inject()),
            engine.StrategyBuilder.scheduled(engine.StrategyBuilder.type_cast()),
            engine.StrategyBuilder.scheduled(engine.StrategyBuilder.relation_shuffle()),
            engine.StrategyBuilder.scheduled(engine.StrategyBuilder.expr_shuffle()),
        ],
    )


def make_ruleset_structural():
    return engine.StrategyBuilder.random_sampler(
        1,
        3,
        [
            engine.StrategyBuilder.scheduled(
                engine.StrategyBuilder.tree_mutate_stmt(engine.TreeMutatorOperation.shuffle_two())
            ),
            engine.StrategyBuilder.scheduled(
                engine.StrategyBuilder.tree_mutate_expr(engine.TreeMutatorOperation.shuffle_two())
            ),
            engine.StrategyBuilder.scheduled(engine.StrategyBuilder.set_ops()),
            engine.StrategyBuilder.scheduled(engine.StrategyBuilder.sub_query()),
            engine.StrategyBuilder.scheduled(engine.StrategyBuilder.recursive_expand_expr()),
        ],
    )


def make_ruleset_havoc_hooked(hook: engine.SchedulerHook):
    return engine.StrategyBuilder.random_sampler(
        3,
        5,
        [
            engine.StrategyBuilder.hooked_scheduled(
                engine.StrategyBuilder.tree_mutate_stmt(engine.TreeMutatorOperation.null_random()),
                hook,
            ),
            engine.StrategyBuilder.hooked_scheduled(
                engine.StrategyBuilder.tree_mutate_expr(engine.TreeMutatorOperation.null_random()),
                hook,
            ),
            engine.StrategyBuilder.hooked_scheduled(engine.StrategyBuilder.type_cast(), hook),
            engine.StrategyBuilder.hooked_scheduled(engine.StrategyBuilder.null_inject(), hook),
            engine.StrategyBuilder.hooked_scheduled(engine.StrategyBuilder.op_flip(), hook),
            engine.StrategyBuilder.hooked_scheduled(
                engine.StrategyBuilder.recursive_expand_expr(), hook
            ),
        ],
    )


def make_ruleset_semantic_hooked(hook: engine.SchedulerHook):
    return engine.StrategyBuilder.random_sampler(
        1,
        3,
        [
            engine.StrategyBuilder.hooked_scheduled(engine.StrategyBuilder.num_bounds(), hook),
            engine.StrategyBuilder.hooked_scheduled(engine.StrategyBuilder.op_flip(), hook),
            engine.StrategyBuilder.hooked_scheduled(engine.StrategyBuilder.null_inject(), hook),
            engine.StrategyBuilder.hooked_scheduled(engine.StrategyBuilder.type_cast(), hook),
            engine.StrategyBuilder.hooked_scheduled(
                engine.StrategyBuilder.relation_shuffle(), hook
            ),
            engine.StrategyBuilder.hooked_scheduled(engine.StrategyBuilder.expr_shuffle(), hook),
        ],
    )


def make_ruleset_structural_hooked(hook: engine.SchedulerHook):
    return engine.StrategyBuilder.random_sampler(
        1,
        3,
        [
            engine.StrategyBuilder.hooked_scheduled(
                engine.StrategyBuilder.tree_mutate_stmt(engine.TreeMutatorOperation.shuffle_two()),
                hook,
            ),
            engine.StrategyBuilder.hooked_scheduled(
                engine.StrategyBuilder.tree_mutate_expr(engine.TreeMutatorOperation.shuffle_two()),
                hook,
            ),
            engine.StrategyBuilder.hooked_scheduled(engine.StrategyBuilder.set_ops(), hook),
            engine.StrategyBuilder.hooked_scheduled(engine.StrategyBuilder.sub_query(), hook),
            engine.StrategyBuilder.hooked_scheduled(
                engine.StrategyBuilder.recursive_expand_expr(), hook
            ),
        ],
    )
