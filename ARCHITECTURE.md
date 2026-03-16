# General

Basic idea is to use a genetic algorithm to mutate a corpus of seeds into a suite of testcases.
This entails several components:

## Oracle

We shoud be able to determine
a) wether a test passes or fails
b) wether this is a duplicate or not
c) what the 'quality'/'fitness' of a specifc testcase is

## Engine

We should be able to generate + organize a large corpus of optimally generic test cases based on some strategy in an efficient way.

## Strategy/Ruleset

In our case mutations should generate a valild query, following our grammar.
However there may be multiple strategies for generating such queries.
Further the engine/orcale should be generic over the strategy and the strategy may be dynamic.
It could for example be informed by the orcale.


