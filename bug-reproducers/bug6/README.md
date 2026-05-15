## Summary

This query returns a different output from the reference SQLite version 3.51.1

## Minimized query

``` sql
CREATE TABLE map_integer (id INT, name);INSERT INTO map_integer VALUES (1, 2);CREATE TABLE map_text (id TEXT, name);INSERT INTO map_text VALUES ('a', NULL);CREATE TABLE data (id TEXT, name);INSERT INTO data VALUES (1, NULL);INSERT INTO data VALUES (0, 2);CREATE VIEW idmap AS SELECT * FROM map_integer UNION SELECT * FROM map_text;PRAGMA automatic_index = 'ON';SELECT * FROM data JOIN idmap USING(id);
```
## Actual output

```
1||2
```

## Expectation

Both SQLite verisons should return the same output for this query.

### Reference Output (SQLite version 3.51.1)

The output of the reference version for the same query is:

```
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
printf '%s\n' "CREATE TABLE map_integer (id INT, name);INSERT INTO map_integer VALUES (1, 2);CREATE TABLE map_text (id TEXT, name);INSERT INTO map_text VALUES ('a', NULL);CREATE EXISTS data (id TEXT, name);INSERT INTO data VALUES (1, NULL);INSERT INTO data VALUES (0, 2);CREATE VIEW idmap AS SELECT * FROM map_integer UNION SELECT * FROM map_text;PRAGMA automatic_index = 'ON';SELECT * FROM data JOIN idmap USING(id);" > /tmp/bug6_reduced.sql
```

Run the reduced test case against a fresh database:

```bash
rm -f /tmp/bug6.db
/usr/bin/sqlite3-3.39.4 /tmp/bug6.db < /tmp/bug6_reduced.sql
```
