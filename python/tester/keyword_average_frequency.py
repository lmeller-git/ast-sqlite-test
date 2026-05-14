import argparse
import csv
import json
from pathlib import Path

try:
    from tester.keyword_coverage import (
        SQLITE_KEYWORDS,
        read_query_dir,
        read_query_log,
        sql_words,
    )
except ModuleNotFoundError:
    from keyword_coverage import SQLITE_KEYWORDS, read_query_dir, read_query_log, sql_words


def empty_occurrences() -> dict[str, int]:
    return dict.fromkeys(sorted(SQLITE_KEYWORDS), 0)


def update_occurrences(counts: dict[str, int], sql: str) -> None:
    for word in sql_words(sql):
        if word in SQLITE_KEYWORDS:
            counts[word] += 1


def write_reports(out_dir: Path, occurrences: dict[str, int], total_queries: int) -> None:
    out_dir.mkdir(parents=True, exist_ok=True)

    records = [
        {
            "keyword": keyword,
            "occurrences": count,
            "average_frequency": 0.0 if total_queries == 0 else count / total_queries,
        }
        for keyword, count in sorted(occurrences.items())
    ]

    summary = {
        "total_queries": total_queries,
        "total_keywords": len(occurrences),
        "total_keyword_occurrences": sum(occurrences.values()),
        "keyword_frequencies": records,
    }

    with (out_dir / "keyword_average_frequency.json").open("w", encoding="utf-8") as handle:
        json.dump(summary, handle, indent=2)
        handle.write("\n")

    with (out_dir / "keyword_average_frequency.csv").open(
        "w", encoding="utf-8", newline=""
    ) as handle:
        writer = csv.DictWriter(
            handle, fieldnames=["keyword", "occurrences", "average_frequency"]
        )
        writer.writeheader()
        writer.writerows(records)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Calculate mean SQL keyword occurrences per query."
    )
    parser.add_argument(
        "input",
        type=Path,
        help="Directory of query_*.sql files or a generated_queries.jsonl file.",
    )
    parser.add_argument(
        "--out",
        type=Path,
        default=None,
        help="Output directory. Defaults to the input directory's parent.",
    )
    args = parser.parse_args()

    occurrences = empty_occurrences()
    total_queries = 0
    queries = read_query_dir(args.input) if args.input.is_dir() else read_query_log(args.input)

    for query in queries:
        total_queries += 1
        update_occurrences(occurrences, query)

    write_reports(args.out or args.input.parent, occurrences, total_queries)


if __name__ == "__main__":
    main()
