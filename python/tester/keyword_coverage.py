import argparse
import csv
import json
import re
from collections.abc import Iterable
from pathlib import Path


# SQLite keyword list from https://sqlite.org/lang_keywords.html
SQLITE_KEYWORDS = frozenset(
    {
        "ABORT",
        "ACTION",
        "ADD",
        "AFTER",
        "ALL",
        "ALTER",
        "ALWAYS",
        "ANALYZE",
        "AND",
        "AS",
        "ASC",
        "ATTACH",
        "AUTOINCREMENT",
        "BEFORE",
        "BEGIN",
        "BETWEEN",
        "BY",
        "CASCADE",
        "CASE",
        "CAST",
        "CHECK",
        "COLLATE",
        "COLUMN",
        "COMMIT",
        "CONFLICT",
        "CONSTRAINT",
        "CREATE",
        "CROSS",
        "CURRENT",
        "CURRENT_DATE",
        "CURRENT_TIME",
        "CURRENT_TIMESTAMP",
        "DATABASE",
        "DEFAULT",
        "DEFERRABLE",
        "DEFERRED",
        "DELETE",
        "DESC",
        "DETACH",
        "DISTINCT",
        "DO",
        "DROP",
        "EACH",
        "ELSE",
        "END",
        "ESCAPE",
        "EXCEPT",
        "EXCLUDE",
        "EXCLUSIVE",
        "EXISTS",
        "EXPLAIN",
        "FAIL",
        "FILTER",
        "FIRST",
        "FOLLOWING",
        "FOR",
        "FOREIGN",
        "FROM",
        "FULL",
        "GENERATED",
        "GLOB",
        "GROUP",
        "GROUPS",
        "HAVING",
        "IF",
        "IGNORE",
        "IMMEDIATE",
        "IN",
        "INDEX",
        "INDEXED",
        "INITIALLY",
        "INNER",
        "INSERT",
        "INSTEAD",
        "INTERSECT",
        "INTO",
        "IS",
        "ISNULL",
        "JOIN",
        "KEY",
        "LAST",
        "LEFT",
        "LIKE",
        "LIMIT",
        "MATCH",
        "MATERIALIZED",
        "NATURAL",
        "NO",
        "NOT",
        "NOTHING",
        "NOTNULL",
        "NULL",
        "NULLS",
        "OF",
        "OFFSET",
        "ON",
        "OR",
        "ORDER",
        "OTHERS",
        "OUTER",
        "OVER",
        "PARTITION",
        "PLAN",
        "PRAGMA",
        "PRECEDING",
        "PRIMARY",
        "QUERY",
        "RAISE",
        "RANGE",
        "RECURSIVE",
        "REFERENCES",
        "REGEXP",
        "REINDEX",
        "RELEASE",
        "RENAME",
        "REPLACE",
        "RESTRICT",
        "RETURNING",
        "RIGHT",
        "ROLLBACK",
        "ROW",
        "ROWS",
        "SAVEPOINT",
        "SELECT",
        "SET",
        "TABLE",
        "TEMP",
        "TEMPORARY",
        "THEN",
        "TIES",
        "TO",
        "TRANSACTION",
        "TRIGGER",
        "UNBOUNDED",
        "UNION",
        "UNIQUE",
        "UPDATE",
        "USING",
        "VACUUM",
        "VALUES",
        "VIEW",
        "VIRTUAL",
        "WHEN",
        "WHERE",
        "WINDOW",
        "WITH",
        "WITHOUT",
    }
)

IDENTIFIER_RE = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")


def sql_words(sql: str) -> Iterable[str]:
    """Yield unquoted SQL identifier-like words, skipping strings and comments."""
    i = 0
    n = len(sql)
    while i < n:
        ch = sql[i]
        nxt = sql[i + 1] if i + 1 < n else ""

        if ch == "-" and nxt == "-":
            i += 2
            while i < n and sql[i] not in "\r\n":
                i += 1
            continue

        if ch == "/" and nxt == "*":
            i += 2
            while i + 1 < n and not (sql[i] == "*" and sql[i + 1] == "/"):
                i += 1
            i = min(i + 2, n)
            continue

        if ch in {"'", '"', "`"}:
            quote = ch
            i += 1
            while i < n:
                if sql[i] == quote:
                    if i + 1 < n and sql[i + 1] == quote:
                        i += 2
                    else:
                        i += 1
                        break
                else:
                    i += 1
            continue

        if ch == "[":
            i += 1
            while i < n and sql[i] != "]":
                i += 1
            i = min(i + 1, n)
            continue

        match = IDENTIFIER_RE.match(sql, i)
        if match is not None:
            yield match.group(0).upper()
            i = match.end()
            continue

        i += 1


def keywords_in_query(sql: str, keywords: frozenset[str] = SQLITE_KEYWORDS) -> set[str]:
    return {word for word in sql_words(sql) if word in keywords}


def empty_counts(keywords: frozenset[str] = SQLITE_KEYWORDS) -> dict[str, int]:
    return dict.fromkeys(sorted(keywords), 0)


def update_counts(
    counts: dict[str, int], sql: str, keywords: frozenset[str] = SQLITE_KEYWORDS
) -> None:
    for keyword in keywords_in_query(sql, keywords):
        counts[keyword] += 1


def read_query_log(path: Path) -> Iterable[str]:
    with path.open(encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, start=1):
            if not line.strip():
                continue
            try:
                record = json.loads(line)
            except json.JSONDecodeError as err:
                raise ValueError(f"{path}:{line_number}: expected JSONL query record") from err
            query = record.get("query")
            if not isinstance(query, str):
                raise ValueError(f"{path}:{line_number}: query field must be a string")
            yield query


def write_reports(out_dir: Path, counts: dict[str, int], total_queries: int) -> None:
    out_dir.mkdir(parents=True, exist_ok=True)

    records = [
        {
            "keyword": keyword,
            "queries": count,
            "coverage": 0.0 if total_queries == 0 else count / total_queries,
        }
        for keyword, count in sorted(counts.items())
    ]

    summary = {
        "total_queries": total_queries,
        "total_keywords": len(counts),
        "covered_keywords": sum(1 for count in counts.values() if count > 0),
        "keyword_counts": records,
    }

    with (out_dir / "keyword_coverage.json").open("w", encoding="utf-8") as handle:
        json.dump(summary, handle, indent=2)
        handle.write("\n")

    with (out_dir / "keyword_coverage.csv").open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=["keyword", "queries", "coverage"])
        writer.writeheader()
        writer.writerows(records)


class KeywordCoverageRecorder:
    def __init__(self, out_dir: str | Path, report_every: int = 1000):
        self.out_dir = Path(out_dir)
        self.out_dir.mkdir(parents=True, exist_ok=True)
        self.query_log_path = self.out_dir / "generated_queries.jsonl"
        self.query_log = self.query_log_path.open("w", encoding="utf-8", buffering=1)
        self.counts = empty_counts()
        self.total_queries = 0
        self.report_every = report_every

    def record(self, sql: str) -> None:
        self.total_queries += 1
        update_counts(self.counts, sql)
        self.query_log.write(json.dumps({"id": self.total_queries, "query": sql}) + "\n")
        if self.report_every > 0 and self.total_queries % self.report_every == 0:
            write_reports(self.out_dir, self.counts, self.total_queries)

    def close(self) -> None:
        self.query_log.flush()
        self.query_log.close()
        write_reports(self.out_dir, self.counts, self.total_queries)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("query_log", type=Path)
    parser.add_argument("--out", type=Path, default=None)
    args = parser.parse_args()

    counts = empty_counts()
    total_queries = 0
    for query in read_query_log(args.query_log):
        total_queries += 1
        update_counts(counts, query)

    write_reports(args.out or args.query_log.parent, counts, total_queries)


if __name__ == "__main__":
    main()
