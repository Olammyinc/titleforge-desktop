#!/usr/bin/env python3
"""
gen_judge_pairs.py — Task 2a pair sampler for judge calibration.

Samples ~200 PAIRWISE comparisons from already-judged titles across
bench-usability.csv, bench-production.csv, bench-batch-constraints.csv and
rank-signal-check.csv. Pair titles WITHIN the same keyword. Deliberately mix
pairs the judge scored close together AND far apart — close ones are where
disagreement shows up.

Outputs judge-pairs.json + a self-contained judge-calibration.html tool.
The HTML embeds ONLY {id, keyword, category, titleA, titleB} — judge scores
are kept in the JSON for the analysis step, never shown to the labeller
(anchoring destroys the comparison).

Usage: python tools/gen_judge_pairs.py [--seed N] [--target 200]
"""

import argparse
import csv
import json
import os
import random
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SOURCES = [
    "bench-usability.csv",
    "bench-production.csv",
    "bench-batch-constraints.csv",
    "rank-signal-check.csv",
]
OUT_JSON = os.path.join(ROOT, "judge-pairs.json")
OUT_HTML = os.path.join(ROOT, "judge-calibration.html")


def load_titles():
    """Return {keyword: {lower_title: (title, category, judge_score)}} deduped."""
    by_keyword = {}
    for fname in SOURCES:
        path = os.path.join(ROOT, fname)
        if not os.path.exists(path):
            print(f"  (skip missing {fname})")
            continue
        with open(path, newline="", encoding="utf-8-sig") as fh:
            for row in csv.DictReader(fh):
                title = (row.get("title") or "").strip()
                if not title:
                    continue
                try:
                    score = int(row.get("judge_score") or 0)
                except (ValueError, TypeError):
                    continue
                if score <= 0:
                    continue
                kw = (row.get("keyword") or "").strip()
                cat = (row.get("category") or "").strip()
                by_keyword.setdefault(kw, {})[title.lower()] = (title, cat, score)
    return by_keyword


def build_pairs(by_keyword, target, rng):
    """Sample within-keyword pairs, mixing near-gap and far-gap judge scores."""
    pairs = []  # (keyword, categoryA, titleA, scoreA, categoryB, titleB, scoreB)

    for kw, titles in by_keyword.items():
        items = list(titles.values())
        if len(items) < 2:
            continue

        # Candidate pairs, classified by judge-score gap.
        near, far = [], []
        for i in range(len(items)):
            for j in range(i + 1, len(items)):
                a, b = items[i], items[j]
                gap = abs(a[2] - b[2])
                if gap <= 15:
                    near.append((a, b))
                elif gap >= 20:
                    far.append((a, b))
                # middle band (16-19) skipped — intentionally bimodal

        # Aim for a 60/40 mix of near/far per keyword so the close calls where
        # disagreement shows up are well represented.
        n_near = min(len(near), max(2, len(items) // 2))
        n_far = min(len(far), max(1, n_near // 2))
        rng.shuffle(near)
        rng.shuffle(far)
        chosen = near[:n_near] + far[:n_far]

        # Randomise order inside each pair so "A" isn't always the higher-scored.
        for a, b in chosen:
            if rng.random() < 0.5:
                a, b = b, a
            pairs.append((kw, a[1], a[0], a[2], b[1], b[0], b[2]))

    rng.shuffle(pairs)

    # Cap to target with keyword spread preserved (shuffle already randomises).
    if len(pairs) > target:
        pairs = pairs[:target]

    return pairs


def render_html(pair_list):
    """Single-file HTML tool. Judge scores are NOT embedded — anchoring."""
    payload = [
        {"id": i, "keyword": p[0], "category": p[1], "titleA": p[2], "titleB": p[5]}
        for i, p in enumerate(pair_list)
    ]
    data_json = json.dumps(payload, ensure_ascii=False)

    return f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>TitleForge — Judge Calibration</title>
<style>
  :root {{
    --ink:#0B0A0A; --paper:#F9F7F2; --forge:#E8782B; --forge-glow:#FF9147;
    --cool:#5B7B8A; --success:#5C8A67; --border:#E5DFD5; --muted:#8A847A;
    --ease: cubic-bezier(0.16,1,0.3,1);
  }}
  * {{ box-sizing:border-box; margin:0; padding:0; }}
  body {{
    font-family:'Satoshi', system-ui, sans-serif; background:var(--paper);
    color:var(--ink); min-height:100vh; display:flex; flex-direction:column;
    -webkit-font-smoothing:antialiased;
  }}
  .header {{
    padding:28px 40px 20px; border-bottom:1px solid var(--border);
    display:flex; align-items:flex-end; justify-content:space-between;
  }}
  .header h1 {{ font-family:'Clash Display', sans-serif; font-size:24px; font-weight:600; }}
  .header p {{ color:var(--muted); font-size:13px; margin-top:4px; }}
  .progress-wrap {{ width:220px; }}
  .progress-label {{ font-size:12px; color:var(--muted); margin-bottom:6px; text-align:right; }}
  .progress-bar {{ height:6px; background:var(--border); border-radius:3px; overflow:hidden; }}
  .progress-fill {{ height:100%; background:var(--forge); width:0%; transition:width 200ms var(--ease); }}
  .main {{ flex:1; display:flex; flex-direction:column; align-items:center; justify-content:center; padding:40px 20px; gap:24px; }}
  .context {{ font-size:14px; color:var(--muted); }}
  .context strong {{ color:var(--ink); }}
  .pair-row {{ display:flex; gap:20px; width:100%; max-width:960px; align-items:stretch; }}
  .title-card {{
    flex:1; background:#fff; border:1px solid var(--border); border-radius:12px;
    padding:36px 28px; text-align:center; cursor:pointer; min-height:220px;
    display:flex; align-items:center; justify-content:center;
    font-family:'Clash Display', sans-serif; font-size:20px; line-height:1.4;
    transition:transform 150ms var(--ease), box-shadow 150ms var(--ease), border-color 150ms var(--ease);
  }}
  .title-card:hover {{ transform:translateY(-2px); box-shadow:0 6px 20px rgba(11,10,10,0.08); border-color:var(--forge); }}
  .title-card.selected {{ border-color:var(--forge); background:rgba(232,120,43,0.05); }}
  .card-label {{ font-size:11px; text-transform:uppercase; letter-spacing:1px; color:var(--muted); margin-bottom:14px; font-family:'Satoshi', sans-serif; }}
  .hint {{ font-size:12px; color:var(--muted); }}
  .hint kbd {{ background:#fff; border:1px solid var(--border); border-radius:4px; padding:1px 6px; font-family:'JetBrains Mono', monospace; font-size:11px; }}
  .done {{ text-align:center; }}
  .done h2 {{ font-family:'Clash Display', sans-serif; font-size:28px; margin-bottom:12px; }}
  .done p {{ color:var(--muted); max-width:520px; margin:0 auto 20px; line-height:1.6; }}
  .btn {{
    background:var(--forge); color:#fff; border:none; border-radius:8px;
    padding:12px 28px; font-size:15px; font-weight:600; cursor:pointer;
    font-family:'Satoshi', sans-serif; transition:filter 150ms var(--ease);
  }}
  .btn:hover {{ filter:brightness(1.08); }}
  .btn-ghost {{
    background:#fff; color:var(--ink); border:1px solid var(--border);
    padding:8px 16px; font-size:13px; border-radius:8px; cursor:pointer;
    font-family:'Satoshi', sans-serif;
  }}
  .btn-ghost:hover {{ border-color:var(--forge); color:var(--forge); }}
</style>
</head>
<body>
<div class="header">
  <div>
    <h1>TitleForge — Judge Calibration</h1>
    <p>Which title would a creator actually publish? Pick the better one.</p>
  </div>
  <div class="progress-wrap">
    <div class="progress-label"><span id="doneCount">0</span> / {len(pair_list)}</div>
    <div class="progress-bar"><div class="progress-fill" id="progressFill"></div></div>
  </div>
</div>

<div class="main" id="main"></div>

<script>
const PAIRS = {data_json};
const STORE_KEY = 'titleforge_judge_pairs_v1';
let answers = {{}};
try {{
  const saved = localStorage.getItem(STORE_KEY);
  if (saved) answers = JSON.parse(saved);
}} catch (e) {{}}

let idx = 0;
while (idx < PAIRS.length && answers[PAIRS[idx].id] !== undefined) idx++;

const main = document.getElementById('main');

function render() {{
  if (idx >= PAIRS.length) {{ renderDone(); return; }}
  const p = PAIRS[idx];
  document.getElementById('doneCount').textContent = Object.keys(answers).length;
  document.getElementById('progressFill').style.width =
    (Object.keys(answers).length / PAIRS.length * 100) + '%';

  main.innerHTML =
    '<div class="context">Keyword: <strong>' + esc(p.keyword) + '</strong> · Category: <strong>' + esc(p.category) + '</strong></div>' +
    '<div class="pair-row">' +
      '<div class="title-card" data-side="a" tabindex="0"><div><div class="card-label">Title A</div>' + esc(p.titleA) + '</div></div>' +
      '<div class="title-card" data-side="b" tabindex="0"><div><div class="card-label">Title B</div>' + esc(p.titleB) + '</div></div>' +
    '</div>' +
    '<div class="hint">Click a title, press <kbd>←</kbd>/<kbd>→</kbd>, or press <kbd>A</kbd>/<kbd>B</kbd>. Skip with <kbd>S</kbd>.</div>';

  document.querySelectorAll('.title-card').forEach(function (card) {{
    card.addEventListener('click', function () {{ choose(card.getAttribute('data-side')); }});
  }});
}}

function choose(side) {{
  const p = PAIRS[idx];
  answers[p.id] = side;
  try {{ localStorage.setItem(STORE_KEY, JSON.stringify(answers)); }} catch (e) {{}}
  idx++;
  while (idx < PAIRS.length && answers[PAIRS[idx].id] !== undefined) idx++;
  render();
}}

function skip() {{
  const p = PAIRS[idx];
  answers[p.id] = 'skip';
  try {{ localStorage.setItem(STORE_KEY, JSON.stringify(answers)); }} catch (e) {{}}
  idx++;
  while (idx < PAIRS.length && answers[PAIRS[idx].id] !== undefined) idx++;
  render();
}}

function renderDone() {{
  document.getElementById('doneCount').textContent = PAIRS.length;
  document.getElementById('progressFill').style.width = '100%';
  const n = Object.keys(answers).filter(k => answers[k] === 'a' || answers[k] === 'b').length;
  main.innerHTML =
    '<div class="done">' +
      '<h2>All pairs labelled</h2>' +
      '<p>' + n + ' of ' + PAIRS.length + ' answered. Download the results, then run the analysis step (calibrate_judge.py) to correlate your taste against the judge.</p>' +
      '<button class="btn" onclick="download()">Download Results (JSON)</button>' +
    '</div>';
}}

function download() {{
  const rows = PAIRS.filter(p => answers[p.id] !== undefined).map(p => ({{
    id: p.id, keyword: p.keyword, category: p.category, titleA: p.titleA, titleB: p.titleB, choice: answers[p.id]
  }}));
  const blob = new Blob([JSON.stringify(rows, null, 2)], {{ type: 'application/json' }});
  const a = document.createElement('a');
  a.href = URL.createObjectURL(blob);
  a.download = 'judge-user-labels.json';
  a.click();
  URL.revokeObjectURL(a.href);
}}

function esc(s) {{
  return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;');
}}

document.addEventListener('keydown', function (e) {{
  if (idx >= PAIRS.length) return;
  const k = e.key.toLowerCase();
  if (k === 'a' || k === 'arrowleft') {{ e.preventDefault(); choose('a'); }}
  else if (k === 'b' || k === 'arrowright') {{ e.preventDefault(); choose('b'); }}
  else if (k === 's') {{ e.preventDefault(); skip(); }}
}});

render();
</script>
</body>
</html>"""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--seed", type=int, default=42)
    ap.add_argument("--target", type=int, default=200)
    args = ap.parse_args()

    print("Loading judged titles from benchmark CSVs...")
    by_keyword = load_titles()
    total = sum(len(v) for v in by_keyword.values())
    pair_able = sum(1 for v in by_keyword.values() if len(v) >= 2)
    print(f"  {total} unique judged titles across {len(by_keyword)} keywords ({pair_able} pair-able)")

    rng = random.Random(args.seed)
    pairs = build_pairs(by_keyword, args.target, rng)
    print(f"  Sampled {len(pairs)} within-keyword pairs (seed {args.seed})")

    # Persist full data (incl. judge scores) for the analysis step.
    full = [
        {
            "id": i,
            "keyword": p[0],
            "categoryA": p[1], "titleA": p[2], "judgeScoreA": p[3],
            "categoryB": p[4], "titleB": p[5], "judgeScoreB": p[6],
        }
        for i, p in enumerate(pairs)
    ]
    with open(OUT_JSON, "w", encoding="utf-8") as fh:
        json.dump({"seed": args.seed, "target": args.target, "pairs": full}, fh, indent=2, ensure_ascii=False)

    with open(OUT_HTML, "w", encoding="utf-8") as fh:
        fh.write(render_html(pairs))

    # Sanity report: gap distribution.
    near = sum(1 for p in pairs if abs(p[3] - p[6]) <= 15)
    far = sum(1 for p in pairs if abs(p[3] - p[6]) >= 20)
    print(f"  Gap mix: close(<=15) {near} · far(>=20) {far} · mid {len(pairs)-near-far}")
    print(f"  Wrote {OUT_JSON}")
    print(f"  Wrote {OUT_HTML}")
    print("\nNext: open judge-calibration.html in a browser, label ~30 min, download")
    print("      judge-user-labels.json, then run: python tools/calibrate_judge.py")


if __name__ == "__main__":
    main()
