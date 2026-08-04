#!/usr/bin/env python3
"""
feature_bias.py — where does the judge actually DISAGREE with the user?

WHY THIS EXISTS
---------------
The original bias analysis was ad hoc and unrepeatable, and its output became
doctrine in three documents. Two of its five headline claims did not survive
re-derivation:

  It reported POINTWISE score deltas — "titles with a colon average +8.4 judge
  points" — and those were read as "the judge is biased toward colons".

  A pointwise delta is NOT a disagreement. The user may like colons too. In the
  HEAD-TO-HEAD frame (the frame he actually labelled in) he picks the
  colon-bearing title 63% of the time and the judge only 36% — the judge
  UNDER-values colons. A v2 rubric built from the pointwise list would have
  suppressed colons and length and moved the judge further from the user.

So this script reports the contrast frame, with confidence intervals, and
refuses to print a verdict where n is too small to support one.

Usage:
  python tools/feature_bias.py [judge-pairs.json] [judge-user-labels.json]
"""

import json
import math
import os
import sys
from collections import Counter

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Below this, print INSUFFICIENT rather than a number. The `$` (n=15) and
# parens (n=12) results were quoted as fact despite being this thin; the guard
# is here so that cannot happen silently again.
MIN_N = 20

FEATURES = {
    "digit":        lambda t: any(c.isdigit() for c in t),
    "dollar":       lambda t: "$" in t,
    "parens":       lambda t: "(" in t,
    "colon":        lambda t: ":" in t,
    "len>=50":      lambda t: len(t) >= 50,
    "question":     lambda t: t.strip().endswith("?"),
    "first_person": lambda t: _has_word(t, {"i", "my", "me", "we", "our"}),
    "second_person": lambda t: _has_word(t, {"you", "your", "you're"}),
    "starts_the":   lambda t: t.strip().lower().startswith("the "),
}


def _has_word(title, wordset):
    words = {w.strip(".,:;!?\"'()").lower() for w in title.split()}
    return bool(words & wordset)


def wilson(k, n, z=1.96):
    """Wilson score interval — correct at small n, unlike normal approximation."""
    if n == 0:
        return (float("nan"), float("nan"))
    p = k / n
    d = 1 + z * z / n
    centre = (p + z * z / (2 * n)) / d
    half = (z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n))) / d
    return (max(0.0, centre - half), min(1.0, centre + half))


def load(pairs_path, labels_path):
    pairs = {p["id"]: p for p in json.load(open(pairs_path, encoding="utf-8"))["pairs"]}
    labels = json.load(open(labels_path, encoding="utf-8"))
    return pairs, labels


def contrast_table(pairs, labels, band_min=None):
    """For each feature, restrict to pairs where EXACTLY ONE title has it."""
    rows = []
    for name, fn in FEATURES.items():
        user_picks = judge_picks = n = 0
        for l in labels:
            p = pairs.get(l["id"])
            if not p or l.get("choice") not in ("a", "b"):
                continue
            if band_min is not None and min(p["judgeScoreA"], p["judgeScoreB"]) < band_min:
                continue
            fa, fb = fn(p["titleA"]), fn(p["titleB"])
            if fa == fb:
                continue  # feature does not discriminate this pair
            n += 1
            feat_side = "a" if fa else "b"
            if l["choice"] == feat_side:
                user_picks += 1
            sa, sb = p["judgeScoreA"], p["judgeScoreB"]
            if (sa > sb and feat_side == "a") or (sb > sa and feat_side == "b"):
                judge_picks += 1
        rows.append((name, n, user_picks, judge_picks))
    return rows


def print_table(rows, title):
    print(f"\n{title}")
    print(f"  {'feature':<15} {'n':>4} {'user picks':>18} {'judge picks':>13} {'gap':>8}  verdict")
    print("  " + "-" * 78)
    for name, n, up, jp in sorted(rows, key=lambda r: -r[1]):
        if n == 0:
            continue
        if n < MIN_N:
            print(f"  {name:<15} {n:>4} {'':>18} {'':>13} {'':>8}  INSUFFICIENT (n<{MIN_N})")
            continue
        u, j = up / n, jp / n
        lo, hi = wilson(up, n)
        # gap = user - judge. POSITIVE means the USER picks the feature-bearing
        # title more often than the judge does, i.e. the judge UNDER-values it.
        # NEGATIVE means the judge picks it more, i.e. it OVER-rewards it.
        # (These were inverted on first write and the output caught it — the
        # whole point of printing the raw percentages next to the verdict.)
        gap = (u - j) * 100
        if gap > 15:
            verdict = "judge UNDER-values — DO NOT SUPPRESS in a rubric"
        elif gap < -15:
            verdict = "judge OVER-rewards — neutralise this one"
        else:
            verdict = "shared preference — leave alone"
        print(f"  {name:<15} {n:>4} {u:>7.0%} [{lo:.0%}-{hi:.0%}] {j:>12.0%} {gap:>+7.0f}pp  {verdict}")


def correlation_matrix(pairs, labels):
    """Features are not independent. Print overlap so nobody reads N effects
    out of a set that is mostly one effect."""
    titles = []
    for l in labels:
        p = pairs.get(l["id"])
        if not p:
            continue
        titles += [p["titleA"], p["titleB"]]
    titles = list(dict.fromkeys(titles))
    names = list(FEATURES)
    print(f"\n  Feature co-occurrence (Jaccard) over {len(titles)} unique titles:")
    sets = {nm: {t for t in titles if FEATURES[nm](t)} for nm in names}
    shown = set()
    for a in names:
        for b in names:
            if a >= b or (b, a) in shown:
                continue
            shown.add((a, b))
            ia, ib = sets[a], sets[b]
            if not ia or not ib:
                continue
            j = len(ia & ib) / len(ia | ib)
            if j >= 0.25:
                print(f"    {a:<14} ~ {b:<14} J={j:.2f}   <-- correlated, not independent")


def skip_analysis(pairs, labels):
    """The 77 skips are thrown away by the agreement metric. Two things fall
    out of them for free."""
    skipped = [l for l in labels if l.get("choice") not in ("a", "b")]
    decided = [l for l in labels if l.get("choice") in ("a", "b")]
    total = len(labels)
    print("\n" + "=" * 80)
    print("  SKIP ANALYSIS — the data the agreement metric discards")
    print("=" * 80)
    print(f"  skipped {len(skipped)}/{total} = {len(skipped)/total:.1%}")
    print(f"\n  PRODUCT CEILING: the user saw NO DIFFERENCE on {len(skipped)/total:.1%} of head-to-heads.")
    print("  A perfect ranker adds nothing on those. Discount any claim about")
    print("  what ranking is worth by this fraction.")

    def gaps(rows):
        out = []
        for l in rows:
            p = pairs.get(l["id"])
            if p:
                out.append(abs(p["judgeScoreA"] - p["judgeScoreB"]))
        return out

    gs, gd = gaps(skipped), gaps(decided)
    if gs and gd:
        ms = sum(gs) / len(gs)
        md = sum(gd) / len(gd)
        print("\n  SKIP CALIBRATION (a free judge test, costs no labelling time):")
        print(f"    mean |judge gap| on SKIPPED pairs : {ms:5.1f}  (n={len(gs)})")
        print(f"    mean |judge gap| on DECIDED pairs : {md:5.1f}  (n={len(gd)})")
        if ms < md:
            print("    PASS — the judge is quieter where the user saw a tie.")
        else:
            print("    FAIL — the judge is LOUDLY confident on pairs the user could not")
            print("    separate. That is mis-calibration independent of agreement rate.")


def main():
    pairs_path = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "judge-pairs.json")
    labels_path = sys.argv[2] if len(sys.argv) > 2 else os.path.join(ROOT, "judge-user-labels.json")
    for p in (pairs_path, labels_path):
        if not os.path.exists(p):
            print(f"ERROR: {p} not found.")
            sys.exit(1)

    pairs, labels = load(pairs_path, labels_path)
    usable = sum(1 for l in labels if l.get("choice") in ("a", "b"))

    print("=" * 80)
    print("  FEATURE BIAS — where the judge and the user actually DISAGREE")
    print("=" * 80)
    print(f"  pairs: {len(pairs)}   labels: {len(labels)}   usable: {usable}")
    print("\n  Reading guide: a POINTWISE score delta is not a disagreement.")
    print("  Only the contrast frame below answers 'do they disagree', because it")
    print("  is the frame the user labelled in.")

    print_table(contrast_table(pairs, labels), "ALL usable pairs")
    print_table(contrast_table(pairs, labels, band_min=70),
                "Restricted to pairs where BOTH titles score >=70 (the band a ranker works in)")
    correlation_matrix(pairs, labels)
    skip_analysis(pairs, labels)

    print("\n" + "=" * 80)
    print("  RUBRIC GUIDANCE derived from the above")
    print("=" * 80)
    print("  Write a rubric rule ONLY against features marked 'judge OVER-rewards'.")
    print("  Features marked 'judge UNDER-values' or 'shared preference' must be")
    print("  LEFT ALONE — writing a rule against them moves the judge AWAY from")
    print("  the user. That was the exact error in the original analysis.")
    print("  Features marked INSUFFICIENT are directional only; they must not")
    print("  become load-bearing until there are more labels.")


if __name__ == "__main__":
    main()
