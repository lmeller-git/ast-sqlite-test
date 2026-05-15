## Summary

This query does not return the same output as the reference SQLite version 3.51.1

## Minimized query

``` sql
CREATE TABLE t1(a, b, c);INSERT INTO t1 VALUES (1, 9223372036854775807, 9223372036854775807);INSERT INTO t1 VALUES (-1, 9223372036854775807, -9223372036854775808);INSERT INTO t1 VALUES (0, 'ccc', 'ddd');SELECT c, sum(c) OVER (ORDER BY b) FROM t1;
```
## Actual output

```
9223372036854775807|-1
-9223372036854775808|-1
ddd|0.0
```

## Expectation


### Reference Output (SQLite version 3.51.1)

The output of the reference version for the same query is:

```
9223372036854775807|-1
-9223372036854775808|-1
ddd|-1.0
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
printf '%s\n' "CREATE TABLE t1(a, b, c);INSERT INTO t1 VALUES (1, 9223372036854775807, 9223372036854775807);INSERT INTO t1 VALUES (-1, 9223372036854775807, -9223372036854775808);INSERT INTO t1 VALUES (0, 'ccc', 'ddd');SELECT c, sum(c) OVER (ORDER BY b) FROM t1;" > /tmp/bug4_reduced.sql
```

Run the reduced test case against a fresh database:

```bash
rm -f /tmp/bug4.db
/usr/bin/sqlite3-3.39.4 /tmp/bug4.db < /tmp/bug4_reduced.sql
```
