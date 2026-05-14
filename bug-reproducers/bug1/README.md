## Summary

Executing this query causes SQLite to crash because it reads an unknown address. SQLite should handle this safely by executing the query or returning a normal SQL error instead of triggering a SEGV.

## Minimized query

``` sql
CREATE TABLE IF NOT EXISTS t1 (w INT GENERATED ALWAYS AS (CAST(1 AS NUMERIC)), x TEXT, y TEXT, a INT, b TEXT, c ANY);CREATE TABLE IF NOT EXISTS t0 (c0 PRIMARY KEY, c1, c2 AS (CAST(1 AS NUMERIC) - c3) REFERENCES t0, c3);PRAGMA foreign_keys = 'ON';CREATE TABLE IF NOT EXISTS t1 (x INTEGER PRIMARY KEY);INSERT INTO t0 VALUES (CAST('1000000 10' AS TEXT), NULL, NULL), (NULL, NULL, CAST(NULL AS NUMERIC));UPDATE t0 SET c1 = CAST('count.test cases for NOT INDEXED' AS REAL), c3 = c0;INSERT INTO t1 (a, b, c) VALUES (CAST(NULL AS NUMERIC), CAST('abcdef' AS REAL), NULL), ('abcdef', NULL, NULL);SELECT NULL, CAST(NULL AS BOOLEAN), y, '|' FROM t1 ORDER BY CAST(NULL AS BOOLEAN);SELECT *, CAST(NULL AS NUMERIC) FROM t0 ORDER BY +c0;
```
## Actual output

```
UndefinedBehaviorSanitizer:DEADLYSIGNAL
==2012304==ERROR: UndefinedBehaviorSanitizer: SEGV on unknown address 0x00000000007f (pc 0x7f59aef5d8d8 bp 0x000000000001 sp 0x7ffecb0bb8b8 T2012304)
==2012304==The signal is caused by a READ memory access.
==2012304==Hint: address points to the zero page.
    #0 0x7f59aef5d8d8  (/lib/x86_64-linux-gnu/libc.so.6+0x1ae8d8) (BuildId: 095c7ba148aeca81668091f718047078d57efddb)
    #1 0x55cd11d42d36 in vdbeRecordCompareString sqlite3.c
    #2 0x55cd11d33178 in sqlite3BtreeIndexMoveto sqlite3.c
    #3 0x55cd11d1b024 in sqlite3VdbeExec sqlite3.c
    #4 0x55cd11caad71 in sqlite3_step (/home/test/sqlite3-src/build/sqlite3+0x54d71) (BuildId: c83ed9f0fa1d5c8a3c52dd8e324103c801ffd639)
    #5 0x55cd11f4eb7a in exec_prepared_stmt shell.c
    #6 0x55cd11f2dc7b in shell_exec shell.c
    #7 0x55cd11f52485 in runOneSqlLine shell.c
    #8 0x55cd11f2e59c in process_input shell.c
    #9 0x55cd11f1e4da in main (/home/test/sqlite3-src/build/sqlite3+0x2c84da) (BuildId: c83ed9f0fa1d5c8a3c52dd8e324103c801ffd639)
    #10 0x7f59aedd8d8f  (/lib/x86_64-linux-gnu/libc.so.6+0x29d8f) (BuildId: 095c7ba148aeca81668091f718047078d57efddb)
    #11 0x7f59aedd8e3f in __libc_start_main (/lib/x86_64-linux-gnu/libc.so.6+0x29e3f) (BuildId: 095c7ba148aeca81668091f718047078d57efddb)
    #12 0x55cd11c70984 in _start (/home/test/sqlite3-src/build/sqlite3+0x1a984) (BuildId: c83ed9f0fa1d5c8a3c52dd8e324103c801ffd639)

UndefinedBehaviorSanitizer can not provide additional info.
SUMMARY: UndefinedBehaviorSanitizer: SEGV (/lib/x86_64-linux-gnu/libc.so.6+0x1ae8d8) (BuildId: 095c7ba148aeca81668091f718047078d57efddb)
==2012304==ABORTING
```

## Expectation

SQLite should execute the statements or reject them with a normal SQL error or
constraint error. It should not  crash.

### Reference Output (SQLite version 3.39.4)

The output of the reference version for the same query is:

```
||||
||||
|0.0|||
1000000 10|0.0|-999999|1000000 10|
```

## Reproduction Steps

Note: We added an additional section to outline the steps we took to run the minimised query for ease of reproducability.

Start an interactive Docker container from the repository root:

```bash
just run-docker-it
```

or

```bash
just run-docker-it-windows
```

Inside the container, build a plain AddressSanitizer-instrumented SQLite binary without the fuzzer pc_guard hooks:

```bash
cd /home/test/sqlite3-src/build
clang -O1 -g -DSQLITE_ENABLE_MATH_FUNCTIONS=1 -DSQLITE_ENABLE_FTS4=1 -DSQLITE_ENABLE_FTS5=1 -DSQLITE_ENABLE_GEOPOLY=1 -DSQLITE_ENABLE_RTREE=1 -DSQLITE_ENABLE_SESSION=1 -DSQLITE_ENABLE_PREUPDATE_HOOK=1 -DSQLITE_ENABLE_FTS3=1 -DSQLITE_ENABLE_FTS3_PARENTHESIS=1 -DSQLITE_ENABLE_JSON1=1 -DSQLITE_ENABLE_STAT4=1 -DSQLITE_ENABLE_UPDATE_DELETE_LIMIT=1 -DSQLITE_ENABLE_COLUMN_METADATA=1 -DSQLITE_ENABLE_DBSTAT_VTAB=1 -DSQLITE_ENABLE_EXPLAIN_COMMENTS=1 -DSQLITE_ENABLE_UNKNOWN_SQL_FUNCTION=1 -DSQLITE_ENABLE_STMTVTAB=1 -DSQLITE_ENABLE_DBPAGE_VTAB=1 -DSQLITE_ENABLE_BYTECODE_VTAB=1 -DSQLITE_ENABLE_OFFSET_SQL_FUNC=1 -fsanitize=address -fno-omit-frame-pointer -o /tmp/sqlite3_asan_plain sqlite3.c shell.c
```
Create the reduced test case:

```bash
printf '%s\n' "PRAGMA foreign_keys = 'ON';CREATE TABLE t0 (c0 PRIMARY KEY, c1, c2 AS (c0 + c1 - c3) REFERENCES t0, c3);INSERT INTO t0 VALUES ('9223372036854775807', 0, NULL), (NULL, 5, 5);UPDATE t0 SET c3 = c0;" > /tmp/bug2_reduced.sql
```

Run the reduced test case against a fresh database:

```bash
rm -f /tmp/bug1.db
ASAN_OPTIONS=detect_leaks=0 /tmp/sqlite3_asan_plain /tmp/bug1.db < /tmp/bug1_reduced.sql
```

This should produce the following output:

```
=================================================================
==395484==ERROR: AddressSanitizer: negative-size-param: (size=-1094795586)
    #0 0x7f0d3c531959  (/usr/lib/libasan.so.8+0x131959) (BuildId: 608e8cc27ef37fce08a131b9c728e96d945e208b)
    #1 0x7f0d3c532cb7 in memcmp (/usr/lib/libasan.so.8+0x132cb7) (BuildId: 608e8cc27ef37fce08a131b9c728e96d945e208b)
    #2 0x564a19d80e68 in vdbeRecordCompareString /home/test/sqlite3-src/build/sqlite3.c:85490
    #3 0x564a19d437c5 in sqlite3BtreeIndexMoveto /home/test/sqlite3-src/build/sqlite3.c:72696
    #4 0x564a19d9f7d8 in sqlite3VdbeExec /home/test/sqlite3-src/build/sqlite3.c:93324
    #5 0x564a19d859bf in sqlite3Step /home/test/sqlite3-src/build/sqlite3.c:86664
    #6 0x564a19d860a9 in sqlite3_step /home/test/sqlite3-src/build/sqlite3.c:86725
    #7 0x564a19c95b2b in exec_prepared_stmt /home/test/sqlite3-src/build/shell.c:14664
    #8 0x564a19c97aaf in shell_exec /home/test/sqlite3-src/build/shell.c:14985
    #9 0x564a19cc87d6 in runOneSqlLine /home/test/sqlite3-src/build/shell.c:22529
    #10 0x564a19cc946b in process_input /home/test/sqlite3-src/build/shell.c:22657
    #11 0x564a19ccceb8 in main /home/test/sqlite3-src/build/shell.c:23484
    #12 0x7f0d3c027c4d  (/usr/lib/libc.so.6+0x27c4d) (BuildId: 5326486e02bff53b5719ee67170f5be40683f240)
    #13 0x7f0d3c027d8a in __libc_start_main (/usr/lib/libc.so.6+0x27d8a) (BuildId: 5326486e02bff53b5719ee67170f5be40683f240)
    #14 0x564a19c4e944 in _start (/home/test/sqlite3-src/build/sqlite3+0x56944) (BuildId: c07ce8ef7bedb3d3200bde03aeb789c52ee51db7)

0x7d1d3adf78f6 is located 4086 bytes inside of 4360-byte region [0x7d1d3adf6900,0x7d1d3adf7a08)
allocated by thread T0 here:
    #0 0x7f0d3c58a5b1 in malloc (/usr/lib/libasan.so.8+0x18a5b1) (BuildId: 608e8cc27ef37fce08a131b9c728e96d945e208b)
    #1 0x564a19cd895b in sqlite3MemMalloc /home/test/sqlite3-src/build/sqlite3.c:25313
    #2 0x564a19cd9a6e in mallocWithAlarm /home/test/sqlite3-src/build/sqlite3.c:29011
    #3 0x564a19cd9bd1 in sqlite3Malloc /home/test/sqlite3-src/build/sqlite3.c:29041
    #4 0x564a19d03959 in pcache1Alloc /home/test/sqlite3-src/build/sqlite3.c:52254
    #5 0x564a19d03c46 in pcache1AllocPage /home/test/sqlite3-src/build/sqlite3.c:52351
    #6 0x564a19d05c31 in pcache1FetchStage2 /home/test/sqlite3-src/build/sqlite3.c:52827
    #7 0x564a19d0608d in pcache1FetchNoMutex /home/test/sqlite3-src/build/sqlite3.c:52931
    #8 0x564a19d060bc in pcache1Fetch /home/test/sqlite3-src/build/sqlite3.c:52973
    #9 0x564a19d01321 in sqlite3PcacheFetch /home/test/sqlite3-src/build/sqlite3.c:51403
    #10 0x564a19d18930 in getPageNormal /home/test/sqlite3-src/build/sqlite3.c:59350
    #11 0x564a19d1977f in sqlite3PagerGet /home/test/sqlite3-src/build/sqlite3.c:59527
    #12 0x564a19d35193 in btreeGetPage /home/test/sqlite3-src/build/sqlite3.c:69011
    #13 0x564a19d3594a in btreeGetUnusedPage /home/test/sqlite3-src/build/sqlite3.c:69156
    #14 0x564a19d46e2d in allocateBtreePage /home/test/sqlite3-src/build/sqlite3.c:73325
    #15 0x564a19d5bc8d in btreeCreateTable /home/test/sqlite3-src/build/sqlite3.c:76557
    #16 0x564a19d5beb6 in sqlite3BtreeCreateTable /home/test/sqlite3-src/build/sqlite3.c:76576
    #17 0x564a19da68aa in sqlite3VdbeExec /home/test/sqlite3-src/build/sqlite3.c:94906
    #18 0x564a19d859bf in sqlite3Step /home/test/sqlite3-src/build/sqlite3.c:86664
    #19 0x564a19d860a9 in sqlite3_step /home/test/sqlite3-src/build/sqlite3.c:86725
    #20 0x564a19c95b2b in exec_prepared_stmt /home/test/sqlite3-src/build/shell.c:14664
    #21 0x564a19c97aaf in shell_exec /home/test/sqlite3-src/build/shell.c:14985
    #22 0x564a19cc87d6 in runOneSqlLine /home/test/sqlite3-src/build/shell.c:22529
    #23 0x564a19cc946b in process_input /home/test/sqlite3-src/build/shell.c:22657
    #24 0x564a19ccceb8 in main /home/test/sqlite3-src/build/shell.c:23484
    #25 0x7f0d3c027c4d  (/usr/lib/libc.so.6+0x27c4d) (BuildId: 5326486e02bff53b5719ee67170f5be40683f240)

SUMMARY: AddressSanitizer: negative-size-param /home/test/sqlite3-src/build/sqlite3.c:85490 in vdbeRecordCompareString
==395484==ABORTING
```

This output is different from the crash recorded by our fuzzer and is identical to bug2. To completely reproduce this bug, some state needs to be set up from a previous query. We do not know what state triggers this exact bug.
