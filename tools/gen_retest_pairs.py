#!/usr/bin/env python3
"""Build a deterministic, order-swapped retest of labelled judge pairs."""

import argparse
import json
import os
import random
import re
import sys

from gen_judge_pairs import render_html


ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PAIRS_PATH = os.path.join(ROOT, "judge-pairs.json")
LABELS_PATH = os.path.join(ROOT, "judge-user-labels.json")
OUT_JSON = os.path.join(ROOT, "judge-retest-pairs.json")
OUT_HTML = os.path.join(ROOT, "judge-retest.html")


class InputError(ValueError):
    pass


def load_inputs():
    for path in (PAIRS_PATH, LABELS_PATH):
        if not os.path.isfile(path):
            raise InputError(f"missing required input: {path}")

    try:
        with open(PAIRS_PATH, encoding="utf-8") as fh:
            pair_data = json.load(fh)
    except (OSError, json.JSONDecodeError) as exc:
        raise InputError(f"cannot read malformed {PAIRS_PATH}: {exc}") from exc
    try:
        with open(LABELS_PATH, encoding="utf-8") as fh:
            labels = json.load(fh)
    except (OSError, json.JSONDecodeError) as exc:
        raise InputError(f"cannot read malformed {LABELS_PATH}: {exc}") from exc

    if not isinstance(pair_data, dict) or not isinstance(pair_data.get("pairs"), list):
        raise InputError(f"malformed {PAIRS_PATH}: expected an object with a pairs list")
    if not isinstance(labels, list):
        raise InputError(f"malformed {LABELS_PATH}: expected a list of labels")

    pairs_by_id = {}
    for pair in pair_data["pairs"]:
        if not isinstance(pair, dict):
            raise InputError(f"malformed {PAIRS_PATH}: every pair must be an object")
        required = ("id", "keyword", "titleA", "titleB")
        missing = [key for key in required if key not in pair or pair[key] in (None, "")]
        if missing:
            raise InputError(f"malformed {PAIRS_PATH}: pair {pair.get('id', '<unknown>')!r} is missing {missing}")
        if pair["id"] in pairs_by_id:
            raise InputError(f"malformed {PAIRS_PATH}: duplicate pair id {pair['id']!r}")
        pairs_by_id[pair["id"]] = pair

    labelled = {}
    for label in labels:
        if not isinstance(label, dict):
            raise InputError(f"malformed {LABELS_PATH}: every label must be an object")
        pair_id = label.get("id")
        choice = label.get("choice")
        if pair_id not in pairs_by_id:
            raise InputError(f"malformed {LABELS_PATH}: label references unknown pair id {pair_id!r}")
        if choice not in {"a", "b", "skip"}:
            raise InputError(f"malformed {LABELS_PATH}: pair {pair_id!r} has invalid choice {choice!r}")
        if pair_id in labelled:
            raise InputError(f"malformed {LABELS_PATH}: duplicate label for pair id {pair_id!r}")
        pair = pairs_by_id[pair_id]
        if label.get("titleA") != pair["titleA"] or label.get("titleB") != pair["titleB"]:
            raise InputError(f"malformed {LABELS_PATH}: titles do not match pair id {pair_id!r}")
        labelled[pair_id] = choice

    if not labelled:
        raise InputError(f"no labelled pairs found in {LABELS_PATH}")
    return pairs_by_id, labelled


def allocate_counts(available, target):
    if target < 3:
        raise InputError("--target must be at least 3 to sample choices a, b, and skip")
    if target > sum(available.values()):
        raise InputError(f"--target {target} exceeds {sum(available.values())} labelled pairs")
    if any(available.get(choice, 0) == 0 for choice in ("a", "b", "skip")):
        raise InputError("labelled input must contain at least one each of choice a, b, and skip")

    counts = {choice: 1 for choice in ("a", "b", "skip")}
    remaining = target - 3
    while remaining:
        choice = max(available, key=lambda item: (available[item] - counts[item], item))
        if counts[choice] >= available[choice]:
            break
        counts[choice] += 1
        remaining -= 1
    if remaining:
        raise InputError("cannot allocate the requested target across the three choice strata")
    return counts


def build_retest(pairs_by_id, labelled, target, seed):
    available = {choice: sum(value == choice for value in labelled.values()) for choice in ("a", "b", "skip")}
    counts = allocate_counts(available, target)
    rng = random.Random(seed)
    selected = []
    for choice, count in counts.items():
        candidates = [pair_id for pair_id, value in labelled.items() if value == choice]
        rng.shuffle(candidates)
        selected.extend((pairs_by_id[pair_id], choice) for pair_id in candidates[:count])
    rng.shuffle(selected)

    result = []
    for pair, choice in selected:
        result.append({
            "id": pair["id"],
            "keyword": pair["keyword"],
            "categoryA": pair.get("categoryA", pair.get("category")),
            "titleA": pair["titleB"],
            "categoryB": pair.get("categoryB", pair.get("category")),
            "titleB": pair["titleA"],
            "originalChoice": choice,
            "swapped": True,
        })
    return result, counts


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--seed", type=int, default=42)
    parser.add_argument("--target", type=int, default=35)
    args = parser.parse_args()
    try:
        pairs_by_id, labelled = load_inputs()
        retest, counts = build_retest(pairs_by_id, labelled, args.target, args.seed)
        html = render_html([
            (row["keyword"], row["categoryA"], row["titleA"], 0,
             row["categoryB"], row["titleB"], 0)
            for row in retest
        ])
        payload = json.dumps([
            {"id": row["id"], "keyword": row["keyword"],
             "category": row["categoryA"], "titleA": row["titleA"], "titleB": row["titleB"]}
            for row in retest
        ], ensure_ascii=False)
        html, replacements = re.subn(r"const PAIRS = .*?;\n", f"const PAIRS = {payload};\n", html, count=1)
        if replacements != 1:
            raise InputError("renderer output did not contain its PAIRS payload")
        html = html.replace("a.download = 'judge-user-labels.json'", "a.download = 'judge-retest-user-labels.json'")
        with open(OUT_JSON, "w", encoding="utf-8") as fh:
            json.dump({"seed": args.seed, "target": args.target, "pairs": retest}, fh, indent=2, ensure_ascii=False)
        with open(OUT_HTML, "w", encoding="utf-8") as fh:
            fh.write(html)
    except InputError as exc:
        parser.error(str(exc))
    print(f"Sampled {len(retest)} pairs (a={counts['a']}, b={counts['b']}, skip={counts['skip']}; seed {args.seed})")
    print(f"Wrote {OUT_JSON}")
    print(f"Wrote {OUT_HTML}")


if __name__ == "__main__":
    main()
