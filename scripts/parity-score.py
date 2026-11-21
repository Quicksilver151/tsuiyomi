#!/usr/bin/env python3
"""Recompute parity completion in docs/roadmap.md from docs/parity/*.md.

Score = (✅ + 0.5 × 🟡) / (✅ + 🟡 + ❌). The intentional-deviation marker (➖)
is excluded from the denominator.
"""

from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DOCS = ROOT / "docs"
ROADMAP = DOCS / "roadmap.md"
START = "<!-- parity-summary:start -->"
END = "<!-- parity-summary:end -->"

CATEGORIES = [
    ("On-disk compatibility", "parity/01-on-disk.md"),
    ("Library", "parity/02-library.md"),
    ("Browse, sources, and global search", "parity/03-browse.md"),
    ("Extensions and host lifecycle", "parity/04-extensions.md"),
    ("Shared story / chapter screen", "parity/05-story.md"),
    ("Reader", "parity/06-reader.md"),
    ("History and Stats", "parity/07-history.md"),
    ("Migrate", "parity/08-migrate.md"),
    ("Settings, security, and data", "parity/09-settings-data.md"),
    ("Documented TUI deviations", "parity/10-tui-deviations.md"),
]


def score(counts: dict[str, int]) -> float:
    denominator = counts["✅"] + counts["🟡"] + counts["❌"]
    if denominator == 0:
        return 0.0
    return (counts["✅"] + 0.5 * counts["🟡"]) / denominator * 100.0


def statuses(path: Path) -> dict[str, int]:
    counts = {"✅": 0, "🟡": 0, "❌": 0, "➖": 0}
    for line in path.read_text().splitlines():
        if not line.startswith("|"):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        for cell in cells:
            if cell in counts:
                counts[cell] += 1
                break
    return counts


def summary_table(results: list[tuple[str, str, dict[str, int]]]) -> list[str]:
    total = {key: sum(counts[key] for _, _, counts in results) for key in ("✅", "🟡", "❌", "➖")}
    lines = [
        "## Parity completion",
        "",
        "| Category | ✅ | 🟡 | ❌ | ➖ | Score |",
        "|---|---:|---:|---:|---:|---:|",
    ]
    for name, link, counts in results:
        lines.append(
            f"| [{name}]({link}) | {counts['✅']} | {counts['🟡']} | {counts['❌']} | "
            f"{counts['➖']} | {score(counts):.1f}% |"
        )
    lines.append(
        f"| **All categories** | **{total['✅']}** | **{total['🟡']}** | "
        f"**{total['❌']}** | **{total['➖']}** | **{score(total):.1f}%** |"
    )
    lines += [
        "",
        "Score = `(✅ + 0.5×🟡) / (✅ + 🟡 + ❌)`. `➖` intentional TUI deviations are",
        "excluded. Recompute with `python3 scripts/parity-score.py` after editing a",
        "category file.",
    ]
    return lines


def main() -> None:
    results = []
    for name, relative in CATEGORIES:
        path = DOCS / relative
        if not path.exists():
            raise SystemExit(f"missing parity file: {path}")
        results.append((name, relative, statuses(path)))

    text = ROADMAP.read_text()
    start = text.index(START) + len(START)
    end = text.index(END)
    block = "\n\n" + "\n".join(summary_table(results)) + "\n\n"
    ROADMAP.write_text(text[:start] + block + text[end:])

    total = {key: sum(counts[key] for _, _, counts in results) for key in ("✅", "🟡", "❌", "➖")}
    print(f"Updated docs/roadmap.md: {score(total):.1f}% complete")
    for name, _, counts in results:
        print(f"  {score(counts):5.1f}%  {name}")


if __name__ == "__main__":
    main()
