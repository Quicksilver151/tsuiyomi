#!/usr/bin/env python3
"""Recompute parity completion in docs/roadmap.md.

All feature tables live in the roadmap under `## Parity detail`. Score =
(✅ + 0.5 × 🟡) / (✅ + 🟡 + ❌); the intentional-deviation marker (➖) is excluded.
"""

from pathlib import Path
import re

ROOT = Path(__file__).resolve().parent.parent
ROADMAP = ROOT / "docs" / "roadmap.md"
START = "<!-- parity-summary:start -->"
END = "<!-- parity-summary:end -->"
DETAIL = "## Parity detail"
DETAIL_END = "## Verification snapshot"

SLUGS = {
    "On-disk compatibility": "on-disk-compatibility",
    "Library": "library",
    "Browse, sources, and global search": "browse-sources-and-global-search",
    "Extensions and host lifecycle": "extensions-and-host-lifecycle",
    "Shared story / chapter screen": "shared-story--chapter-screen",
    "Reader": "reader",
    "History and Stats": "history-and-stats",
    "Migrate": "migrate",
    "Settings, security, and data": "settings-security-and-data",
    "Documented TUI deviations": "documented-tui-deviations",
}


def score(counts: dict[str, int]) -> float:
    denominator = counts["✅"] + counts["🟡"] + counts["❌"]
    if denominator == 0:
        return 0.0
    return (counts["✅"] + 0.5 * counts["🟡"]) / denominator * 100.0


def parse(text: str) -> list[tuple[str, dict[str, int]]]:
    start = text.index(DETAIL) + len(DETAIL)
    end = text.index(DETAIL_END)
    sections: list[tuple[str, dict[str, int]]] = []
    counts: dict[str, int] | None = None
    for line in text[start:end].splitlines():
        if line.startswith("### "):
            name = line[4:].strip()
            counts = {"✅": 0, "🟡": 0, "❌": 0, "➖": 0}
            sections.append((name, counts))
            continue
        if counts is None or not line.startswith("|"):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        for cell in cells:
            if cell in counts:
                counts[cell] += 1
                break
    return sections


def summary_table(sections: list[tuple[str, dict[str, int]]]) -> list[str]:
    total = {key: sum(counts[key] for _, counts in sections) for key in ("✅", "🟡", "❌", "➖")}
    lines = [
        "## Parity completion",
        "",
        "| Category | ✅ | 🟡 | ❌ | ➖ | Score |",
        "|---|---:|---:|---:|---:|---:|",
    ]
    for name, counts in sections:
        anchor = SLUGS.get(name, re.sub(r"[^a-z0-9 -]", "", name.lower()).replace(" ", "-"))
        lines.append(
            f"| [{name}](#{anchor}) | {counts['✅']} | {counts['🟡']} | {counts['❌']} | "
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
        "table below.",
    ]
    return lines


def main() -> None:
    text = ROADMAP.read_text()
    sections = parse(text)
    if not sections:
        raise SystemExit("no parity detail sections found")

    start = text.index(START) + len(START)
    end = text.index(END)
    block = "\n\n" + "\n".join(summary_table(sections)) + "\n\n"
    ROADMAP.write_text(text[:start] + block + text[end:])

    total = {key: sum(counts[key] for _, counts in sections) for key in ("✅", "🟡", "❌", "➖")}
    print(f"Updated docs/roadmap.md: {score(total):.1f}% complete")
    for name, counts in sections:
        print(f"  {score(counts):5.1f}%  {name}")


if __name__ == "__main__":
    main()
