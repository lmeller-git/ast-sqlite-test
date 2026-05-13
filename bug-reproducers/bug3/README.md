## Summary
<!--
Explain briefly what goes wrong and explain why you believe this is a
bug and not the intended behavior of SQLite (if it is not a crash).
-->
## Minimized query
``` sql
CREATE VIRTUAL TABLE t2 USING rtree;
```
## Actual output

Executing the query crashes the SQLite process (segmentation fault) under AddressSanitizer with a null pointer read in `sqlite3GetToken`.

```sql
AddressSanitizer:DEADLYSIGNAL                                                                                                             
=================================================================                                                                         
==27==ERROR: AddressSanitizer: SEGV on unknown address 0x000000000000 (pc 0x563ab3cdd832 bp 0x7ffce02a6310 sp 0x7ffce02a62f0 T0)          
==27==The signal is caused by a READ memory access.                                                                                       
==27==Hint: address points to the zero page.                                                                                              
    #0 0x563ab3cdd832 in sqlite3GetToken /home/test/sqlite3-src/build/sqlite3.c:169324:19                                                 
    #1 0x563ab3fa9b00 in rtreeTokenLength /home/test/sqlite3-src/build/sqlite3.c:201425:10                                                
    #2 0x563ab3fa9b00 in rtreeInit /home/test/sqlite3-src/build/sqlite3.c:201494:23                                                       
    #3 0x563ab3fa4173 in rtreeCreate /home/test/sqlite3-src/build/sqlite3.c:198695:10                                                     
    #4 0x563ab3cdc988 in vtabCallConstructor /home/test/sqlite3-src/build/sqlite3.c:147608:8                                              
    #5 0x563ab3caaea9 in sqlite3VtabCallCreate /home/test/sqlite3-src/build/sqlite3.c:147782:10                                           
    #6 0x563ab3c73413 in sqlite3VdbeExec /home/test/sqlite3-src/build/sqlite3.c:96079:10                                                  
    #7 0x563ab3bb06fe in sqlite3Step /home/test/sqlite3-src/build/sqlite3.c:86664:10                                                      
    #8 0x563ab3bb06fe in sqlite3_step /home/test/sqlite3-src/build/sqlite3.c:86725:16                                                     
    #9 0x563ab404b226 in exec_prepared_stmt /home/test/sqlite3-src/build/shell.c:14664:8                                                  
    #10 0x563ab401814a in shell_exec /home/test/sqlite3-src/build/shell.c:14985:7                                                         
    #11 0x563ab404f196 in runOneSqlLine /home/test/sqlite3-src/build/shell.c:22529:8                                                      
    #12 0x563ab40199da in process_input /home/test/sqlite3-src/build/shell.c:22675:15                                                     
    #13 0x563ab4001d2f in main /home/test/sqlite3-src/build/shell.c:23492:12                                                              
    #14 0x7f1a8feecd8f  (/lib/x86_64-linux-gnu/libc.so.6+0x29d8f) (BuildId: 095c7ba148aeca81668091f718047078d57efddb)                     
    #15 0x7f1a8feece3f in __libc_start_main (/lib/x86_64-linux-gnu/libc.so.6+0x29e3f) (BuildId: 095c7ba148aeca81668091f718047078d57efddb) 
    #16 0x563ab3ae0564 in _start (/tmp/sqlite3_asan_plain+0x96564) (BuildId: 1b54b7ea90fe93ccc7e40ab2e279652d23d7b50a)                    
                                                                                                                                          
AddressSanitizer can not provide additional info.                                                                                         
SUMMARY: AddressSanitizer: SEGV /home/test/sqlite3-src/build/sqlite3.c:169324:19 in sqlite3GetToken                                       
==27==ABORTING                                                                                                                            
```
## Expectation
SQLite should not crash when executing a malformed `CREATE VIRTUAL TABLE ... USING rtree` statement without column arguments. It should reject the statement gracefully, for example by returning a normal SQL error indicating that the rtree table declaration is invalid or incomplete.
