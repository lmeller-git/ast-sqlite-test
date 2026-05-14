## Summary
Executing this query causes SQLite to crash under AddressSanitizer because it calls `memcmp` with a negative size. SQLite should handle this safely by executing the query or returning a normal SQL error instead of triggering a memory-safety failure.     

## Minimized query
``` sql
PRAGMA foreign_keys = 'ON';CREATE TABLE t0 (c0 PRIMARY KEY, c1, c2 AS (c0 + c1 - c3) REFERENCES t0, c3);INSERT INTO t0 VALUES ('9223372036854775807', 0, NULL), (NULL, 5, 5);UPDATE t0 SET c3 = c0;
```

## Actual output
```sql
=================================================================
==44==ERROR: AddressSanitizer: negative-size-param: (size=-1094795586)
    #0 0x5570b59699e5 in MemcmpInterceptorCommon(void*, int (*)(void const*, void const*, unsigned long), void const*, void const*, unsigned long) (/tmp/sqlite3_asan_plain+0xb09e5) (BuildId: 1b54b7ea90fe93ccc7e40ab2e279652d23d7b50a)
    #1 0x5570b5969fb9 in memcmp (/tmp/sqlite3_asan_plain+0xb0fb9) (BuildId: 1b54b7ea90fe93ccc7e40ab2e279652d23d7b50a)
    #2 0x5570b5b25871 in vdbeRecordCompareString /home/test/sqlite3-src/build/sqlite3.c:85490:11
    #3 0x5570b5b04ba9 in sqlite3BtreeIndexMoveto /home/test/sqlite3-src/build/sqlite3.c
    #4 0x5570b5addc12 in sqlite3VdbeExec /home/test/sqlite3-src/build/sqlite3.c:93324:10
    #5 0x5570b5a1f6fe in sqlite3Step /home/test/sqlite3-src/build/sqlite3.c:86664:10
    #6 0x5570b5a1f6fe in sqlite3_step /home/test/sqlite3-src/build/sqlite3.c:86725:16
    #7 0x5570b5eba226 in exec_prepared_stmt /home/test/sqlite3-src/build/shell.c:14664:8
    #8 0x5570b5e8714a in shell_exec /home/test/sqlite3-src/build/shell.c:14985:7
    #9 0x5570b5ebe196 in runOneSqlLine /home/test/sqlite3-src/build/shell.c:22529:8
    #10 0x5570b5e88734 in process_input /home/test/sqlite3-src/build/shell.c:22657:17
    #11 0x5570b5e70d2f in main /home/test/sqlite3-src/build/shell.c:23492:12
    #12 0x7f95881b8d8f  (/lib/x86_64-linux-gnu/libc.so.6+0x29d8f) (BuildId: 095c7ba148aeca81668091f718047078d57efddb)
    #13 0x7f95881b8e3f in __libc_start_main (/lib/x86_64-linux-gnu/libc.so.6+0x29e3f) (BuildId: 095c7ba148aeca81668091f718047078d57efddb)
    #14 0x5570b594f564 in _start (/tmp/sqlite3_asan_plain+0x96564) (BuildId: 1b54b7ea90fe93ccc7e40ab2e279652d23d7b50a)

0x63200001397d is located 78205 bytes inside of 87208-byte region [0x632000000800,0x632000015ca8)
allocated by thread T0 here:
    #0 0x5570b59d23ae in __interceptor_malloc (/tmp/sqlite3_asan_plain+0x1193ae) (BuildId: 1b54b7ea90fe93ccc7e40ab2e279652d23d7b50a)
    #1 0x5570b5d318f5 in sqlite3MemMalloc /home/test/sqlite3-src/build/sqlite3.c:25323:7
    #2 0x5570b5a10068 in mallocWithAlarm /home/test/sqlite3-src/build/sqlite3.c:29011:7
    #3 0x5570b5a10068 in sqlite3Malloc /home/test/sqlite3-src/build/sqlite3.c:29041:5
    #4 0x5570b5d33fa1 in pcache1InitBulk /home/test/sqlite3-src/build/sqlite3.c:52206:27
    #5 0x5570b5d33fa1 in pcache1AllocPage /home/test/sqlite3-src/build/sqlite3.c:52327:45
    #6 0x5570b5d33fa1 in pcache1FetchStage2 /home/test/sqlite3-src/build/sqlite3.c:52827:13
    #7 0x5570b5d321d2 in pcache1FetchNoMutex /home/test/sqlite3-src/build/sqlite3.c:52931:12
    #8 0x5570b5d321d2 in pcache1Fetch /home/test/sqlite3-src/build/sqlite3.c:52973:34
    #9 0x5570b5aa083b in sqlite3PcacheFetch /home/test/sqlite3-src/build/sqlite3.c:51403:10
    #10 0x5570b5aa083b in getPageNormal /home/test/sqlite3-src/build/sqlite3.c:59350:11
    #11 0x5570b5a27092 in sqlite3PagerGet /home/test/sqlite3-src/build/sqlite3.c:59527:10
    #12 0x5570b5a27092 in btreeGetPage /home/test/sqlite3-src/build/sqlite3.c:69011:8
    #13 0x5570b5a27092 in lockBtree /home/test/sqlite3-src/build/sqlite3.c:69981:8
    #14 0x5570b5a27092 in sqlite3BtreeBeginTrans /home/test/sqlite3-src/build/sqlite3.c:70371:47
    #15 0x5570b5b11b60 in sqlite3InitOne /home/test/sqlite3-src/build/sqlite3.c:134668:10
    #16 0x5570b5a5a7a3 in sqlite3Init /home/test/sqlite3-src/build/sqlite3.c:134855:10
    #17 0x5570b5b68031 in sqlite3ReadSchema /home/test/sqlite3-src/build/sqlite3.c:134881:10
    #18 0x5570b5b68031 in sqlite3StartTable /home/test/sqlite3-src/build/sqlite3.c:116361:20
    #19 0x5570b5b5c443 in yy_reduce /home/test/sqlite3-src/build/sqlite3.c:166771:4
    #20 0x5570b5a4b315 in sqlite3Parser /home/test/sqlite3-src/build/sqlite3.c:168420:15
    #21 0x5570b5a4b315 in sqlite3RunParser /home/test/sqlite3-src/build/sqlite3.c:169718:5
    #22 0x5570b5b40999 in sqlite3Prepare /home/test/sqlite3-src/build/sqlite3.c:135177:5
    #23 0x5570b5a47b12 in sqlite3LockAndPrepare /home/test/sqlite3-src/build/sqlite3.c:135252:10
    #24 0x5570b5a1ef0b in sqlite3_prepare_v2 /home/test/sqlite3-src/build/sqlite3.c:135338:8
    #25 0x5570b5e86db0 in shell_exec /home/test/sqlite3-src/build/shell.c:14889:10
    #26 0x5570b5ebe196 in runOneSqlLine /home/test/sqlite3-src/build/shell.c:22529:8
    #27 0x5570b5e88734 in process_input /home/test/sqlite3-src/build/shell.c:22657:17
    #28 0x5570b5e70d2f in main /home/test/sqlite3-src/build/shell.c:23492:12
    #29 0x7f95881b8d8f  (/lib/x86_64-linux-gnu/libc.so.6+0x29d8f) (BuildId: 095c7ba148aeca81668091f718047078d57efddb)

SUMMARY: AddressSanitizer: negative-size-param (/tmp/sqlite3_asan_plain+0xb09e5) (BuildId: 1b54b7ea90fe93ccc7e40ab2e279652d23d7b50a) in MemcmpInterceptorCommon(void*, int (*)(void const*, void const*, unsigned long), void const*, void const*, unsigned long)
==44==ABORTING
```

## Expectation
SQLite should execute the statements or reject them with a normal SQL error or
constraint error. It should not call `memcmp` with a negative size, crash, or
trigger an AddressSanitizer memory-safety failure.
