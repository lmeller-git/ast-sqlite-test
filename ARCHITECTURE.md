# General

Basic idea is to use a genetic algorithm to mutate a corpus of seeds into a suite of testcases.
This entails several components:

## Executor

The executor module is used to run tests and collect relevant data of these runs. it should:

a) connect to the specifci sqlite versions we require
b) do so in a "protected environment", i.e. a subprocess, such as to protect from crashes like segfault, ...
c) spawn N connection async and stream the collected data back to the caller
d) to collect coverage data, we may look into https://clang.llvm.org/docs/SanitizerCoverage.html

## Oracle

Given the testing data from the executor, the oracle should determine wether a test has found a bug or not. If a bug was found this should be recorded and the testcase fed back into the mutation loop, if applicable (i.e. maybe skip crashes).
More specfically it should:

a) determinr wether a test ran as exepected or not
b) if a bug was found, it should be recorded and potentially deduplicated (based on query, stack frames, coverage (hashed edges for example), ...)
c) the reported bug may be passed along to a test minifier
d) feed back relevant tests into the mutator if possible this should also happen nonblocking on a different thread, however since the overead is likely not very high (except IO), this has lowere priority

## Engine

Given a corpus and the data from the executor + oracle the engine should select the next batch to mutate and gc bad testcases. I.e.:

a) determine which testcases to use for the next batch based on fitness + scheduler choice
b) apply muations to each batch and return this to tehe executor
c) cull "bad tests" periodically, i.e. gc tests that didd not find new coverage/bugs

## Strategy/Ruleset

In our case mutations should generate a valild query, following our grammar.
However there may be multiple strategies for generating such queries.
Further the engine/orcale should be generic over the strategy and the strategy may be dynamic.
It could for example be informed by the orcale.


