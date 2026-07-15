#!/usr/bin/env python3
"""
generate-curated-titles.py — v2 (July 2026)
Generates ~1,920 curated titles across 16 categories with real tone/genre metadata.

Batches per category:
  Phase 2: 40 titles at tone='normal', genre='any' (base pool top-up)
  Phase 1: 10 titles per non-normal tone × 8 tones (tone-tagged batches)
  TOTAL per category: 120 titles → 1,920 overall

Phase 3 (genre-tagged batches) skipped for now — lower priority.

Outputs additively to curated-titles-output.json (writes after each category).
"""

import os
import sys
import json
import urllib.request
import random
import time

API_KEY = os.environ.get('DEEPSEEK_API_KEY')
if not API_KEY:
    print('ERROR: DEEPSEEK_API_KEY environment variable not set', file=sys.stderr)
    sys.exit(1)

CATEGORIES = [
    'book', 'article', 'blog', 'movie', 'song', 'youtube',
    'podcast', 'newsletter', 'ebook', 'speech', 'album',
    'poem', 'street', 'character', 'product', 'childname'
]

# 8 non-normal tones (normal is handled separately in Phase 2)
TONES = ['shout', 'whisper', 'blessing', 'provocative', 'minimalist', 'storytelling', 'question', 'playful']

# Per-category base prompts (Phase 2 uses these, Phase 1 appends tone guidance)
CATEGORY_PROMPTS = {
    'book': 'Generate {count} real, published-style book titles (fiction and non-fiction) across various genres. Include a mix of literary, commercial, memoir, and genre fiction titles. Make them feel authentic — like books you would find on a real shelf.',
    'article': 'Generate {count} article and essay titles suitable for magazines, journals, and online publications. Mix of long-form journalism, opinion pieces, research essays, and feature articles.',
    'blog': 'Generate {count} blog post titles that are clickable but not clickbaity. Mix of how-to, listicle, opinion, and personal story formats.',
    'movie': 'Generate {count} movie/film titles across genres (drama, comedy, thriller, sci-fi, horror, romance, documentary). Mix of one-word titles, phrase titles, and descriptive titles.',
    'song': 'Generate {count} song titles across genres (pop, rock, hip-hop, country, electronic, indie, R&B).',
    'youtube': 'Generate {count} YouTube video titles that are compelling without being clickbait. Mix of tutorials, reviews, vlogs, documentaries, and entertainment formats.',
    'podcast': 'Generate {count} podcast episode titles. Mix of interview shows, narrative podcasts, educational series, and conversational formats.',
    'newsletter': 'Generate {count} newsletter/subject line titles. Should be intriguing but professional.',
    'ebook': 'Generate {count} ebook/digital book titles across self-help, business, technology, health, and personal development genres.',
    'speech': 'Generate {count} speech/presentation titles suitable for TED talks, conferences, keynote addresses, and commencement speeches.',
    'album': 'Generate {count} music album titles across genres. Mix of conceptual titles, single-word statements, and poetic phrases.',
    'poem': 'Generate {count} poem titles. Mix of traditional, modern, and experimental poetry styles. Should be evocative and lyrical.',
    'street': 'Generate {count} street/place names that sound like real-world locations in English-speaking countries.',
    'character': 'Generate {count} character names suitable for fiction. Mix of first names, full names, and character archetype names.',
    'product': 'Generate {count} product/brand names suitable for real consumer products.',
    'childname': 'Generate {count} children name/baby name ideas with brief meaning. Mix of traditional, modern, unique, and culturally diverse names.'
}

# Tone-specific stylistic guidance injected into the prompt
TONE_GUIDANCE = {
    'shout': 'STYLE REQUIREMENT: Write big, urgent, high-energy titles. Use strong verbs, absolutes, bold assertions — the kind of title that reads loud even without exclamation points. Every title should feel declarative and forceful.',
    'whisper': 'STYLE REQUIREMENT: Write quiet, intimate, understated titles. Focus on small moments, restraint, soft language — the opposite of a hook. Titles should feel like a secret shared between two people.',
    'blessing': 'STYLE REQUIREMENT: Write warm, affirming, benedictory, hopeful titles. Every title should read like a well-wish or gentle reassurance. Uplifting and kind, never preachy.',
    'provocative': 'STYLE REQUIREMENT: Write confrontational titles that challenge an assumption or call something out directly. Each title should make the reader want to argue back or agree hard. No safe, neutral phrasing.',
    'minimalist': 'STYLE REQUIREMENT: Write extremely short titles — exactly 2 to 5 words, no filler, nothing decorative. Every word must carry weight. Strip everything unnecessary.',
    'storytelling': 'STYLE REQUIREMENT: Write narrative titles that drop the reader mid-scene. Evoke a specific moment or character rather than a topic. Should feel like the first line of a story.',
    'question': 'STYLE REQUIREMENT: Every title must be literally phrased as a question. Curiosity-driven, not rhetorical filler. Questions that genuinely make someone want to know the answer.',
    'playful': 'STYLE REQUIREMENT: Write light, fun, pun-friendly, humor-forward titles. Wordplay welcome. Should make someone smile or chuckle. No heavy or serious tones.',
}


def build_prompt(category, count, tone='normal', genre='any'):
    """Build a generation prompt for a given category, tone, and genre."""
    base = CATEGORY_PROMPTS.get(category, f'Generate {count} titles for the category "{category}".')
    prompt = base.format(count=count)

    # Inject tone guidance for non-normal tones
    if tone != 'normal' and tone in TONE_GUIDANCE:
        prompt += '\n\n' + TONE_GUIDANCE[tone]

    # Inject genre guidance (for future Phase 3 use)
    if genre != 'any':
        prompt += f'\n\nFocus specifically on the "{genre}" genre/topic area.'

    return prompt


def call_deepseek(prompt):
    data = json.dumps({
        'model': 'deepseek-v4-flash',
        'messages': [
            {
                'role': 'system',
                'content': 'You are a title generation database curator. Generate high-quality, real-world titles that sound like they belong to actual published works. Return ONLY a JSON object with a "titles" key containing an array of title strings. Each title must feel authentic and original. No clichés or AI-sounding filler.'
            },
            {
                'role': 'user',
                'content': prompt
            }
        ],
        'temperature': 0.85,
        'max_tokens': 4096,
        'response_format': {'type': 'json_object'}
    }).encode('utf-8')

    req = urllib.request.Request(
        'https://api.deepseek.com/v1/chat/completions',
        data=data,
        headers={
            'Content-Type': 'application/json',
            'Authorization': f'Bearer {API_KEY}'
        },
        method='POST'
    )

    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            body = json.loads(resp.read().decode('utf-8'))
            content = body['choices'][0]['message']['content']
    except Exception as e:
        print(f'  API call failed: {e}', file=sys.stderr)
        raise

    # Clean and parse
    content = content.replace('```json\n', '').replace('```\n', '').replace('```', '').strip()
    parsed = json.loads(content)

    # Extract titles from various response shapes
    if 'titles' in parsed and isinstance(parsed['titles'], list):
        return parsed['titles']
    for val in parsed.values():
        if isinstance(val, list) and len(val) > 0:
            return val
    return []


def run_batch(category, count, tone='normal', genre='any'):
    """Run a single generation batch. Returns list of normalized title objects."""
    prompt = build_prompt(category, count, tone, genre)
    tag = f'{category}/{tone}/{genre}'
    print(f'  [{tag}] Requesting {count} titles...', file=sys.stderr, flush=True)

    try:
        titles_raw = call_deepseek(prompt)
    except Exception as e:
        print(f'  [{tag}] ERROR: {e}', file=sys.stderr, flush=True)
        return []

    normalized = []
    for t in titles_raw:
        title_str = ''
        if isinstance(t, str):
            title_str = t
        elif isinstance(t, dict):
            title_str = t.get('title', t.get('name', ''))
        title_str = title_str.strip()
        if len(title_str) > 3:
            normalized.append({
                'title': title_str,
                'genre': genre,       # STAMP ACTUAL REQUESTED VALUES (not hardcoded)
                'tone': tone,          # STAMP ACTUAL REQUESTED VALUES (not hardcoded)
                'appeal_score': random.randint(70, 100),
                'notes': ''
            })

    print(f'  [{tag}] Got {len(normalized)} titles', file=sys.stderr, flush=True)
    time.sleep(0.6)  # Rate-limit: ~600ms between API calls
    return normalized


def load_existing_output(output_path):
    """Load existing output file if it exists, otherwise return empty structure."""
    if os.path.exists(output_path):
        with open(output_path) as f:
            return json.load(f)
    return {'curated_titles': {}}


def save_output(output_path, result):
    """Write result to output file (atomic: write to temp first, then rename)."""
    tmp_path = output_path + '.tmp'
    with open(tmp_path, 'w') as f:
        json.dump(result, f, indent=2)
    os.rename(tmp_path, output_path)


def main():
    random.seed(42)
    output_path = '/home/olammy/projects/paul/titleforge-desktop/curated-titles-output.json'

    # Load existing output (additive — don't overwrite previous runs)
    result = load_existing_output(output_path)

    # Initialize category arrays that don't exist yet
    for cat in CATEGORIES:
        if cat not in result['curated_titles']:
            result['curated_titles'][cat] = []

    errors = []
    total_generated = 0

    for cat_idx, cat in enumerate(CATEGORIES):
        cat_start_total = len(result['curated_titles'][cat])
        print(f'\n{"="*60}', file=sys.stderr)
        print(f'Category [{cat_idx+1}/16]: {cat} (currently {cat_start_total} titles)', file=sys.stderr, flush=True)

        # ── Phase 2: Base pool top-up (40 titles, normal tone) ──
        print(f'  ── Phase 2: Base top-up (tone=normal, genre=any, count=40) ──', file=sys.stderr)
        batch = run_batch(cat, count=40, tone='normal', genre='any')
        result['curated_titles'][cat].extend(batch)
        total_generated += len(batch)

        # Write after Phase 2 for this category
        save_output(output_path, result)
        print(f'  [SAVED] {cat} now has {len(result["curated_titles"][cat])} titles', file=sys.stderr)

        # ── Phase 1: Tone-tagged batches (10 titles × 8 tones) ──
        for tone in TONES:
            print(f'  ── Phase 1: tone={tone}, count=10 ──', file=sys.stderr)
            batch = run_batch(cat, count=10, tone=tone, genre='any')
            result['curated_titles'][cat].extend(batch)
            total_generated += len(batch)

            # Write after each tone batch for safety
            save_output(output_path, result)

        cat_end_total = len(result['curated_titles'][cat])
        cat_new = cat_end_total - cat_start_total
        print(f'  [SAVED] {cat} now has {cat_end_total} titles (+{cat_new} this run)', file=sys.stderr)

    # Final save
    save_output(output_path, result)

    print(f'\n{"="*60}', file=sys.stderr)
    print(f'Complete! Output: {output_path}', file=sys.stderr)

    # Summary
    total = 0
    for cat in CATEGORIES:
        count = len(result['curated_titles'][cat])
        print(f'  {cat}: {count}', file=sys.stderr)
        total += count
    print(f'\nTotal curated titles: {total} (generated {total_generated} this run)', file=sys.stderr)

    if errors:
        print(f'\nErrors ({len(errors)}):', file=sys.stderr)
        for e in errors:
            print(f'  - {e}', file=sys.stderr)


if __name__ == '__main__':
    main()
