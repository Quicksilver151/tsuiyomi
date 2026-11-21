#!/usr/bin/env python3
"""Recompute the completion summary in docs/parity.md.

Score = (✅ + 0.5 × 🟡) / (✅ + 🟡 + ❌). The intentional-deviation marker (➖)
is excluded from the denominator.
"""

from pathlib import Path
import re

ROOT = Path(__file__).resolve().parent.parent
PARITY = ROOT / "docs" / "parity.md"
MARKERS = {
    "start": "<!-- parity-summary:start -->",
    "end": "<!-- parity-summary:end -->",
}
SCORE_MARKER = "<!-- parity-score -->"


def score(counts: dict[str, int]) -> float:
    denominator = counts["✅"] + counts["🟡"] + counts["❌"]
    if denominator == 0:
        return 0.0
    return (counts["✅"] + 0.5 * counts["🟡"]) / denominator * 100.0


def status_of(cells: list[str]) -> str | None:
    for cell in cells:
        if cell in ("✅", "🟡", "❌", "➖"):
            return cell
    return None


def parse(lines: list[str]) -> dict[str, dict[str, int]]:
    sections: dict[str, dict[str, int]] = {}
    current: str | None = None
    for line in lines:
        if line.startswith("## "):
            current = line[3:].strip()
            sections.setdefault(current, {"✅": 0, "🟡": 0, "❌": 0, "➖": 0})
            continue
        if not line.startswith("|") or current is None:
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        status = status_of(cells)
        if status:
            sections[current][status] += 1
    return {name: counts for name, counts in sections.items() if sum(counts.values())}


def summary_table(sections: dict[str, dict[str, int]]) -> list[str]:
    total = {key: 0 for key in ("✅", "🟡", "❌", "➖")}
    lines = [
        "### Completion summary",
        "",
        "| Category | ✅ | 🟡 | ❌ | ➖ | Score |",
        "|---|---:|---:|---:|---:|---:|",
    ]
    for name, counts in sections.items():
        for key in total:
            total[key] += counts[key]
        lines.append(
            f"| {name} | {counts['✅']} | {counts['🟡']} | {counts['❌']} | "
            f"{counts['➖']} | {score(counts):.1f}% |"
        )
    lines.append(
        f"| **All feature tables** | **{total['✅']}** | **{total['🟡']}** | "
        f"**{total['❌']}** | **{total['➖']}** | **{score(total):.1f}%** |"
    )
    lines += [
        "",
        "Score = `(✅ + 0.5×🟡) / (✅ + 🟡 + ❌)`. `➖` marks intentional TUI",
        "deviations and is excluded. Recompute with "
        "`python3 scripts/parity-score.py` after changing status rows.",
        "",
    ]
    return lines


def update(lines: list[str], sections: dict[str, dict[str, int]]) -> list[str]:
    # Replace or insert the summary block after the Legend section.
    try:
        start = lines.index(MARKERS["start"])
        end = lines.index(MARKERS["end"], start)
        summary = [MARKERS["start"], ""] + summary_table(sections) + [MARKERS["end"]]
        lines = lines[:start] + summary + lines[end + 1 :]
    except ValueError:
        insert_at = next(
            (index + 1 for index, line in enumerate(lines) if line.startswith("- ✅")),
            len(lines),
        )
        while insert_at < len(lines) and lines[insert_at].strip():
            insert_at += 1
        summary = ["", MARKERS["start"], ""] + summary_table(sections) + [MARKERS["end"]]
        lines = lines[:insert_at] + summary + lines[insert_at:]

    # Add or replace one score line directly below every scored section heading.
    result: list[str] = []
    for line in lines:
        result.append(line)
        if line.startswith("## "):
            name = line[3:].strip()
            counts = sections.get(name)
            if counts:
                result.append(
                    f"{SCORE_MARKER} *Completion: **{score(counts):.1f}%** · "
                    f"{counts['✅']} ✅ · {counts['🟡']} 🟡 · {counts['❌']} ❌"
                    + (f" · {counts['➖']} ➖" if counts["➖"] else "")
                    + "*"
                )
            continue
        if line.startswith(SCORE_MARKER):
            # The freshly inserted score line is already in the output.
            continue

    # The loop above appends old score lines too; remove duplicates/stale copies.
    cleaned: list[str] = []
    for index, line in enumerate(result):
        if line.startswith(SCORE_MARKER):
            previous = cleaned[-1] if cleaned else ""
            if previous.startswith(SCORE_MARKER):
                continue
            if index > 0 and result[index - 1].startswith("## "):
                cleaned.append(line)
            continue
        cleaned.append(line)
    return cleaned


def main() -> None:
    lines = PARITY.read_text().splitlines()
    sections = parse(lines)
    updated = update(lines, sections)
    PARITY.write_text("\n".join(updated) + "\n")
    total = {key: sum(counts[key] for counts in sections.values()) for key in ("✅", "🟡", "❌", "➖")}
    print(f"Updated docs/parity.md: {score(total):.1f}% complete")
    for name, counts in sections.items():
        print(f"  {score(counts):5.1f}%  {name}")


if __name__ == "__main__":
    main()
