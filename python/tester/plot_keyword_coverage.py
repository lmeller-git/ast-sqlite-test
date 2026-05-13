import argparse
import csv
import json
from pathlib import Path


def read_keyword_counts(path: Path) -> tuple[int | None, list[dict[str, float | int | str]]]:
    if path.suffix == ".json":
        with path.open(encoding="utf-8") as handle:
            data = json.load(handle)
        return data.get("total_queries"), data["keyword_counts"]

    if path.suffix == ".csv":
        with path.open(encoding="utf-8", newline="") as handle:
            rows = list(csv.DictReader(handle))
        records = [
            {
                "keyword": row["keyword"],
                "queries": int(row["queries"]),
                "coverage": float(row["coverage"]),
            }
            for row in rows
        ]
        return None, records

    raise ValueError(f"Unsupported input type: {path.suffix}. Expected .json or .csv")


def plot_top_keywords(input_path: Path, out_path: Path, top_n: int) -> None:
    total_queries, records = read_keyword_counts(input_path)
    top_records = top_keyword_records(records, top_n)
    write_svg_bar_chart(out_path, top_records, total_queries)


def top_keyword_records(
    records: list[dict[str, float | int | str]], top_n: int
) -> list[dict[str, float | int | str]]:
    return sorted(records, key=lambda record: int(record["queries"]), reverse=True)[:top_n]


def write_svg_bar_chart(
    out_path: Path, records: list[dict[str, float | int | str]], total_queries: int | None
) -> None:
    rows = records
    width = 1100
    row_height = 28
    top_margin = 70
    left_margin = 130
    right_margin = 170
    bottom_margin = 50
    height = top_margin + bottom_margin + row_height * len(rows)
    max_count = max((int(record["queries"]) for record in rows), default=1)
    chart_width = width - left_margin - right_margin
    title = f"Top {len(rows)} SQLite Keywords by Generated Query Coverage"
    if total_queries is not None:
        title += f" (n={total_queries:,})"

    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
        f'viewBox="0 0 {width} {height}">',
        "<style>",
        "text { font-family: Arial, sans-serif; fill: #24292f; }",
        ".title { font-size: 20px; font-weight: 700; }",
        ".label { font-size: 12px; }",
        ".value { font-size: 11px; }",
        ".axis { stroke: #8c959f; stroke-width: 1; }",
        ".grid { stroke: #d8dee4; stroke-width: 1; }",
        "</style>",
        f'<text x="{width / 2}" y="32" text-anchor="middle" class="title">'
        f"{escape_xml(title)}</text>",
        f'<line x1="{left_margin}" y1="{top_margin - 10}" x2="{left_margin}" '
        f'y2="{height - bottom_margin + 5}" class="axis" />',
    ]

    for tick in range(0, 6):
        x = left_margin + chart_width * tick / 5
        value = round(max_count * tick / 5)
        parts.append(
            f'<line x1="{x:.1f}" y1="{top_margin - 10}" x2="{x:.1f}" '
            f'y2="{height - bottom_margin + 5}" class="grid" />'
        )
        parts.append(
            f'<text x="{x:.1f}" y="{height - 20}" text-anchor="middle" '
            f'class="value">{value:,}</text>'
        )

    for i, record in enumerate(rows):
        keyword = str(record["keyword"])
        count = int(record["queries"])
        coverage = float(record["coverage"])
        y = top_margin + i * row_height
        bar_width = 0 if max_count == 0 else chart_width * count / max_count
        parts.append(
            f'<text x="{left_margin - 10}" y="{y + 17}" text-anchor="end" '
            f'class="label">{escape_xml(keyword)}</text>'
        )
        parts.append(
            f'<rect x="{left_margin}" y="{y + 4}" width="{bar_width:.1f}" height="18" '
            'fill="#287c8e" />'
        )
        parts.append(
            f'<text x="{left_margin + bar_width + 6:.1f}" y="{y + 17}" '
            f'class="value">{count:,} ({coverage:.1%})</text>'
        )

    parts.append("</svg>")
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text("\n".join(parts), encoding="utf-8")


def escape_xml(value: str) -> str:
    return (
        value.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace('"', "&quot;")
    )


def default_output_path(input_path: Path) -> Path:
    return input_path.parent / "top_30_keywords.svg"


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("input", type=Path, help="keyword_coverage.json or keyword_coverage.csv")
    parser.add_argument("--out", type=Path, default=None, help="Output SVG path")
    parser.add_argument("--top", type=int, default=30)
    args = parser.parse_args()

    plot_top_keywords(args.input, args.out or default_output_path(args.input), args.top)


if __name__ == "__main__":
    main()
