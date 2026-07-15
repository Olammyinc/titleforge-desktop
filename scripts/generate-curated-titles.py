#!/usr/bin/env python3
"""
generate-curated-titles.py
Regenerates ~800 curated titles across 16 categories via DeepSeek V4 Flash.
Writes to curated-titles-output.json for merging into seed-data.json.
"""

import os
import sys
import json
import urllib.request
import random

API_KEY = os.environ.get('DEEPSEEK_API_KEY')
if not API_KEY:
    print('ERROR: DEEPSEEK_API_KEY environment variable not set', file=sys.stderr)
    sys.exit(1)

CATEGORIES = [
    'book', 'article', 'blog', 'movie', 'song', 'youtube',
    'podcast', 'newsletter', 'ebook', 'speech', 'album',
    'poem', 'street', 'character', 'product', 'childname'
]

TITLES_PER_CATEGORY = 50

PROMPTS = {
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


def call_deepseek(prompt):
    data = json.dumps({
        'model': 'deepseek-v4-flash',
        'messages': [
            {
                'role': 'system',
                'content': 'You are a title generation database curator. Generate high-quality, real-world titles that sound like they belong to actual published works. Return ONLY a JSON object with a "titles" key containing an array of title strings. Each title must feel authentic and original. Vary the styles — some poetic, some direct, some provocative, some minimalist. No clichés or AI-sounding filler.'
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


def main():
    random.seed(42)
    result = {'curated_titles': {}}
    errors = []

    for cat in CATEGORIES:
        prompt = PROMPTS.get(cat, f'Generate {TITLES_PER_CATEGORY} titles for the category "{cat}".').format(count=TITLES_PER_CATEGORY)

        print(f'Generating {TITLES_PER_CATEGORY} titles for: {cat}...', file=sys.stderr, flush=True)

        try:
            titles_raw = call_deepseek(prompt)

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
                        'genre': 'any',
                        'tone': 'normal',
                        'appeal_score': random.randint(70, 100),
                        'notes': ''
                    })

            result['curated_titles'][cat] = normalized
            print(f'  Got {len(normalized)} titles', file=sys.stderr)

        except Exception as e:
            errors.append(f'{cat}: {e}')
            print(f'  ERROR: {e}', file=sys.stderr)
            result['curated_titles'][cat] = []

    # Write output
    output_path = '/home/olammy/projects/paul/titleforge-desktop/curated-titles-output.json'
    with open(output_path, 'w') as f:
        json.dump(result, f, indent=2)

    print(f'\nDone! Output: {output_path}', file=sys.stderr)

    # Summary
    total = 0
    for cat in CATEGORIES:
        count = len(result['curated_titles'].get(cat, []))
        print(f'  {cat}: {count}', file=sys.stderr)
        total += count

    print(f'\nTotal: {total}', file=sys.stderr)

    if errors:
        print(f'\nErrors ({len(errors)}):', file=sys.stderr)
        for e in errors:
            print(f'  - {e}', file=sys.stderr)


if __name__ == '__main__':
    main()
