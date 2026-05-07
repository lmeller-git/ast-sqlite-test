from lib_sf import engine


def make_ruleset_havoc(body: engine.MABBody):
    return engine.StrategyBuilder.ucb1(
        body,
        [
            engine.StrategyBuilder.tree_mutate_stmt(engine.TreeMutatorOperation.null_random()),
            engine.StrategyBuilder.tree_mutate_expr(engine.TreeMutatorOperation.null_random()),
            engine.StrategyBuilder.type_cast(),
            engine.StrategyBuilder.sub_query(),
        ],
        2
    )


def make_ruleset_semantic(body: engine.MABBody):
    return engine.StrategyBuilder.ucb1(
        body,
        [
            engine.StrategyBuilder.num_bounds(),
            engine.StrategyBuilder.op_flip(),
            engine.StrategyBuilder.null_inject(),
            engine.StrategyBuilder.type_cast(),
            engine.StrategyBuilder.relation_shuffle(),
            engine.StrategyBuilder.expr_shuffle(),
        ],
        2
    )


def make_ruleset_structural(body: engine.MABBody):
    return engine.StrategyBuilder.ucb1(
        body,
        [
            engine.StrategyBuilder.tree_mutate_stmt(engine.TreeMutatorOperation.shuffle_two()),
            engine.StrategyBuilder.tree_mutate_expr(engine.TreeMutatorOperation.shuffle_two()),
            engine.StrategyBuilder.set_ops(),
            engine.StrategyBuilder.recursive_expand_expr(),
        ],
        2
    )
