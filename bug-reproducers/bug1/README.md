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
printf '%s\n' "CREATE TABLE IF NOT EXISTS t1 (w INT GENERATED ALWAYS AS (CAST(1 AS NUMERIC)), x TEXT, y TEXT, a INT, b TEXT, c ANY);CREATE TABLE IF NOT EXISTS t0 (c0 PRIMARY KEY, c1, c2 AS (CAST(1 AS NUMERIC) - c3) REFERENCES t0, c3);PRAGMA foreign_keys = 'ON';CREATE TABLE IF NOT EXISTS t1 (x INTEGER PRIMARY KEY);INSERT INTO t0 VALUES (CAST('1000000 10' AS TEXT), NULL, NULL), (NULL, NULL, CAST(NULL AS NUMERIC));UPDATE t0 SET c1 = CAST('count.test cases for NOT INDEXED' AS REAL), c3 = c0;INSERT INTO t1 (a, b, c) VALUES (CAST(NULL AS NUMERIC), CAST('abcdef' AS REAL), NULL), ('abcdef', NULL, NULL);SELECT NULL, CAST(NULL AS BOOLEAN), y, '|' FROM t1 ORDER BY CAST(NULL AS BOOLEAN);SELECT *, CAST(NULL AS NUMERIC) FROM t0 ORDER BY +c0;" > /tmp/bug1_reduced.sql
```

Run the reduced test case against a fresh database:

```bash
rm -f /tmp/bug1.db
ASAN_OPTIONS=detect_leaks=0 /tmp/sqlite3_asan_plain /tmp/bug1.db < /tmp/bug1_reduced.sql
```

This should produce the following output:

```
=================================================================
==21==ERROR: AddressSanitizer: negative-size-param: (size=-1094795586)
    #0 0x560def6429e5 in MemcmpInterceptorCommon(void*, int (*)(void const*, void const*, unsigned long), void const*, void const*, unsigned long) (/tmp/sqlite3_asan_plain+0xb09e5) (BuildId: 1b54b7ea90fe93ccc7e40ab2e279652d23d7b50a)
    #1 0x560def642fb9 in memcmp (/tmp/sqlite3_asan_plain+0xb0fb9) (BuildId: 1b54b7ea90fe93ccc7e40ab2e279652d23d7b50a)
    #2 0x560def7fe871 in vdbeRecordCompareString /home/test/sqlite3-src/build/sqlite3.c:85490:11
    #3 0x560def7ddba9 in sqlite3BtreeIndexMoveto /home/test/sqlite3-src/build/sqlite3.c
    #4 0x560def7b6c12 in sqlite3VdbeExec /home/test/sqlite3-src/build/sqlite3.c:93324:10
    #5 0x560def6f86fe in sqlite3Step /home/test/sqlite3-src/build/sqlite3.c:86664:10
    #6 0x560def6f86fe in sqlite3_step /home/test/sqlite3-src/build/sqlite3.c:86725:16
    #7 0x560defb93226 in exec_prepared_stmt /home/test/sqlite3-src/build/shell.c:14664:8
    #8 0x560defb6014a in shell_exec /home/test/sqlite3-src/build/shell.c:14985:7
    #9 0x560defb97196 in runOneSqlLine /home/test/sqlite3-src/build/shell.c:22529:8
    #10 0x560defb61734 in process_input /home/test/sqlite3-src/build/shell.c:22657:17
    #11 0x560defb49d2f in main /home/test/sqlite3-src/build/shell.c:23492:12
    #12 0x7f7af7b4dd8f  (/lib/x86_64-linux-gnu/libc.so.6+0x29d8f) (BuildId: 095c7ba148aeca81668091f718047078d57efddb)
    #13 0x7f7af7b4de3f in __libc_start_main (/lib/x86_64-linux-gnu/libc.so.6+0x29e3f) (BuildId: 095c7ba148aeca81668091f718047078d57efddb)
    #14 0x560def628564 in _start (/tmp/sqlite3_asan_plain+0x96564) (BuildId: 1b54b7ea90fe93ccc7e40ab2e279652d23d7b50a)

0x63200001287e is located 73854 bytes inside of 87208-byte region [0x632000000800,0x632000015ca8)
allocated by thread T0 here:
    #0 0x560def6ab3ae in __interceptor_malloc (/tmp/sqlite3_asan_plain+0x1193ae) (BuildId: 1b54b7ea90fe93ccc7e40ab2e279652d23d7b50a)
    #1 0x560defa0a8f5 in sqlite3MemMalloc /home/test/sqlite3-src/build/sqlite3.c:25323:7
    #2 0x560def6e9068 in mallocWithAlarm /home/test/sqlite3-src/build/sqlite3.c:29011:7
    #3 0x560def6e9068 in sqlite3Malloc /home/test/sqlite3-src/build/sqlite3.c:29041:5
    #4 0x560defa0cfa1 in pcache1InitBulk /home/test/sqlite3-src/build/sqlite3.c:52206:27
    #5 0x560defa0cfa1 in pcache1AllocPage /home/test/sqlite3-src/build/sqlite3.c:52327:45
    #6 0x560defa0cfa1 in pcache1FetchStage2 /home/test/sqlite3-src/build/sqlite3.c:52827:13
    #7 0x560defa0b1d2 in pcache1FetchNoMutex /home/test/sqlite3-src/build/sqlite3.c:52931:12
    #8 0x560defa0b1d2 in pcache1Fetch /home/test/sqlite3-src/build/sqlite3.c:52973:34
    #9 0x560def77983b in sqlite3PcacheFetch /home/test/sqlite3-src/build/sqlite3.c:51403:10
    #10 0x560def77983b in getPageNormal /home/test/sqlite3-src/build/sqlite3.c:59350:11
    #11 0x560def700092 in sqlite3PagerGet /home/test/sqlite3-src/build/sqlite3.c:59527:10
    #12 0x560def700092 in btreeGetPage /home/test/sqlite3-src/build/sqlite3.c:69011:8
    #13 0x560def700092 in lockBtree /home/test/sqlite3-src/build/sqlite3.c:69981:8
    #14 0x560def700092 in sqlite3BtreeBeginTrans /home/test/sqlite3-src/build/sqlite3.c:70371:47
    #15 0x560def7eab60 in sqlite3InitOne /home/test/sqlite3-src/build/sqlite3.c:134668:10
    #16 0x560def7337a3 in sqlite3Init /home/test/sqlite3-src/build/sqlite3.c:134855:10
    #17 0x560def841031 in sqlite3ReadSchema /home/test/sqlite3-src/build/sqlite3.c:134881:10
    #18 0x560def841031 in sqlite3StartTable /home/test/sqlite3-src/build/sqlite3.c:116361:20
    #19 0x560def835443 in yy_reduce /home/test/sqlite3-src/build/sqlite3.c:166771:4
    #20 0x560def724315 in sqlite3Parser /home/test/sqlite3-src/build/sqlite3.c:168420:15
    #21 0x560def724315 in sqlite3RunParser /home/test/sqlite3-src/build/sqlite3.c:169718:5
    #22 0x560def819999 in sqlite3Prepare /home/test/sqlite3-src/build/sqlite3.c:135177:5
    #23 0x560def720b12 in sqlite3LockAndPrepare /home/test/sqlite3-src/build/sqlite3.c:135252:10
    #24 0x560def6f7f0b in sqlite3_prepare_v2 /home/test/sqlite3-src/build/sqlite3.c:135338:8
    #25 0x560defb5fdb0 in shell_exec /home/test/sqlite3-src/build/shell.c:14889:10
    #26 0x560defb97196 in runOneSqlLine /home/test/sqlite3-src/build/shell.c:22529:8
    #27 0x560defb61734 in process_input /home/test/sqlite3-src/build/shell.c:22657:17
    #28 0x560defb49d2f in main /home/test/sqlite3-src/build/shell.c:23492:12
    #29 0x7f7af7b4dd8f  (/lib/x86_64-linux-gnu/libc.so.6+0x29d8f) (BuildId: 095c7ba148aeca81668091f718047078d57efddb)

SUMMARY: AddressSanitizer: negative-size-param (/tmp/sqlite3_asan_plain+0xb09e5) (BuildId: 1b54b7ea90fe93ccc7e40ab2e279652d23d7b50a) in MemcmpInterceptorCommon(void*, int (*)(void const*, void const*, unsigned long), void const*, void const*, unsigned long)
==21==ABORTING
```

This output is different from the crash recorded by our fuzzer and is identical to bug2. To completely reproduce this bug, some state needs to be set up from a previous query. We do not know what state triggers this exact bug.
