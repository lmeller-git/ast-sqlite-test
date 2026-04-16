from .lib_sf import CorpusEntry, RawEntry


class Generation:
    def into_members(self) -> list[RawEntry]: ...


class SchedulerBuilder:
    @staticmethod
    def fifo() -> SchedulerBuilder: ...


class StrategyBuilder:
    @staticmethod
    def uppercase() -> StrategyBuilder: ...
    @staticmethod
    def merger() -> StrategyBuilder: ...
    @staticmethod
    def random_sampler(
        max_choices: int, min_choixes: int, choices: list[StrategyBuilder]
    ) -> StrategyBuilder: ...
    @staticmethod
    def randomize(strategy: StrategyBuilder, probability: float) -> StrategyBuilder: ...
    @staticmethod
    def splice_in() -> StrategyBuilder: ...
    @staticmethod
    def table_scrambler() -> StrategyBuilder: ...
    @staticmethod
    def table_guard() -> StrategyBuilder: ...


class SeedGeneratorBuilder:
    @staticmethod
    def literal(lit: str) -> SeedGeneratorBuilder: ...
    @staticmethod
    def dir_reader(dir: str) -> SeedGeneratorBuilder: ...


class Engine:
    def __init__(
        self,
        scheduler: SchedulerBuilder,
        strategies: list[StrategyBuilder],
        shmem_queue: IPCTokenQueue,
        rng_seed: int = 42,
    ) -> None: ...
    def populate(self, seed_builders: list[SeedGeneratorBuilder]) -> None: ...
    def mutate_batch(self, batch_size: int) -> Generation: ...
    def commit_test_result(self, entry: RawEntry, result: TestResult) -> None: ...
    def snapshot(self) -> list[CorpusEntry]: ...
    def clear_strategies(self) -> None: ...
    def add_strategy(self, strategy: StrategyBuilder) -> None: ...


class IPCTokenQueue:
    def __init__(self, n_workers: int, max_edge: int) -> None: ...
    def pop(self) -> IPCTokenHandle | None: ...
    def push(self, token: IPCTokenHandle) -> IPCTokenHandle | None: ...


class IPCTokenHandle:
    def as_env(self) -> str: ...


class TestResult:
    triggers_bug: bool
    is_valid_syntax: bool
    exec_time: int
    token: IPCTokenHandle

    def __init__(
        self,
        exec_time: int,
        token: IPCTokenHandle,
        is_valid_syntax: bool = True,
        triggers_bug: bool = False,
    ) -> None: ...
