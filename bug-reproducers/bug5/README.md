## Summary

This query returns a different outtput to the reference SQLite version 3.51.1.


## Minimized query

``` sql
CREATE TABLE t0(c0);INSERT INTO t0 VALUES (NULL);CREATE INDEX i0 ON t0(9223372036854775807) WHERE c0 IS NOT NULL;SELECT 1 FROM t0 WHERE (t0.c0 IS FALSE) IS FALSE;
```
## Actual output

```
```

## Expectation


### Reference Output (SQLite version 3.51.1)

The output of the reference version for the same query is:

```
1
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

Create the reduced test case:

```bash
printf '%s\n' "CREATE TABLE t0(c0);INSERT INTO t0 VALUES (NULL);CREATE INDEX i0 ON t0(9223372036854775807) WHERE c0 IS NOT NULL;SELECT 1 FROM t0 WHERE (t0.c0 IS FALSE) IS FALSE;" > /tmp/bug5_reduced.sql
```

Run the reduced test case against a fresh database:

```bash
rm -f /tmp/bug5.db
/usr/bin/sqlite3-3.39.4 /tmp/bug5.db < /tmp/bug5_reduced.sql
```
