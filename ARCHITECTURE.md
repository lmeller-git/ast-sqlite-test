# General

Basic idea is to use a genetic algorithm to mutate a corpus of seeds into a suite of testcases.
This entails several components:

## Executor

The executor module is used to run tests and collect relevant data of these runs. it should:

- connect to the specific sqlite versions we require
- do so in a "protected environment", i.e. a subprocess, such as to protect from crashes like segfault, ...
- spawn N connection async and stream the collected data back to the caller
- to collect coverage data, we may look into https://clang.llvm.org/docs/SanitizerCoverage.html
- this does not read the coverage memory. It only waits for the process to exit, collects metadata (exit code, execution time, stdout/stderr), and returns this data alongside the original IPC Token.

## Oracle

Given the testing data from the executor, the oracle should determine wether a test has found a bug or not. If a bug was found this should be recorded and the testcase fed back into the mutation loop, if applicable (i.e. maybe skip crashes).
More specfically it should:

- determine wether a test ran as exepected or not
- if a bug was found, it should be recorded and potentially deduplicated (based on query, stack frames, coverage (hashed edges for example), ...)
- the reported bug may be passed along to a test minifier
- feed back relevant tests into the mutator if possible this should also happen nonblocking on a different thread, however since the overead is likely not very high (except IO), this has lowere priority

## Engine

Given a corpus and the data from the executor + oracle the engine should select the next batch to mutate and gc bad testcases. I.e.:

- determine which testcases to use for the next batch based on fitness + scheduler choice -- Done (add scheduler using fitness)
- apply muations to each batch and return this to the executor -- Done
- cull "bad tests" periodically, i.e. gc tests that didd not find new coverage/bugs

## Strategy/Ruleset

- In our case mutations should generate a valild query, following our grammar. -- Done
- However there may be multiple strategies for generating such queries. -- Done
- Further the engine/orcale should be generic over the strategy and the strategy may be dynamic. It could for example be informed by the orcale. -- Done


## Event Loop

The top level event loop should dispatch work to the executor workers and start engine gc and chore passes, while waiting for the executors to finish.

queues: `TODO queue` SPMC or SPSC queue for incoming mutated tests, which gets consumed by executor threads, `DONE queue` MPSC or SPSC queue for test results from executor threads, `TOKEN queue` SPMC or SPSC queue for "IPC Tokens"

the main event loop pulls batches of mutated testcases from the mutation engine. This returns testcases with attached `IPC Tokens`, which may be used to communicate with the spawned subprocess of the sqlite connection. These are then put into the `TODO queue`.
the `DONE` queue is regularily polled and emptied by the event loop, tests are, together with their produced metadata and `IPC Tokens`, put back into the engine and/or go through the oracle.
if at some point the `TODO` queue is full enough and the `DONE` queue is empty enough, chore passes may be dispatched (gc, ...).

executors are managed by a separate free-threaded event-loop, which handles dispatching work from the `TODO` queue to idle workers.


the IPC tokens are env variables, which tell the subprocess how to communicate  with the fuzzer.

IMPORTANT: IPC TOKENS are strictly !Copy and !Clone and must never be leaked. in other words each token must be returned exactly once per cycle


## API changes (to current engine api):

Engine will be initialized with n_workers: int. this creates a static number of IPC tokens

mutate_batch will return CropusEntrys along with attached tokens.

submit_selected will take CorpusEntry along with the attached tokens.

tokens of invalid queries may be returned individually via retrun_token
