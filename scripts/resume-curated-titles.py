#!/usr/bin/env python3
"""
Resume script — picks up where the main generate script left off.
Only generates missing tone batches per category based on what's already
in curated-titles-output.json.
"""

import os, sys, json, urllib.request, random, time

API_KEY = os.environ.get('DEEPSEEK_API_KEY')
if not API_KEY:
    print('ERROR: DEEPSEEK_API_KEY not set', file=sys.stderr)
    sys.exit(1)

CATEGORIES = [
    'book', 'article', 'blog', 'movie', 'song', 'youtube',
    'podcast', 'newsletter', 'ebook', 'speech', 'album',
    'poem', 'street', 'character', 'product', 'childname'
]

TONES = ['shout', 'whisper', 'blessing', 'provocative', 'minimalist', 'storytelling', 'question', 'playful']

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
    'childname': 'Generate {count} children name/baby name ideas with brief meaning. Mix of traditional, modern, unique, and culturally diverse names.',
}

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

OUTPUT_PATH = '/home/olammy/projects/paul/titleforge-desktop/curated-titles-output.json'


def build_prompt(category, count, tone='normal'):
    base = CATEGORY_PROMPTS.get(category, f'Generate {count} titles for "{category}".')
    prompt = base.format(count=count)
    if tone != 'normal' and tone in TONE_GUIDANCE:
        prompt += '\n\n' + TONE_GUIDANCE[tone]
    return prompt


def call_deepseek(prompt):
    data = json.dumps({
        'model': 'deepseek-v4-flash',
        'messages': [
            {'role': 'system', 'content': 'You are a title generation database curator. Generate high-quality, real-world titles that sound like they belong to actual published works. Return ONLY a JSON object with a "titles" key containing an array of title strings. Each title must feel authentic and original. No clichés or AI-sounding filler.'},
            {'role': 'user', 'content': prompt}
        ],
        'temperature': 0.85,
        'max_tokens': 4096,
        'response_format': {'type': 'json_object'}
    }).encode('utf-8')

    req = urllib.request.Request(
        'https://api.deepseek.com/v1/chat/completions',
        data=data,
        headers={'Content-Type': 'application/json', 'Authorization': f'Bearer {API_KEY}'},
        method='POST'
    )
    with urllib.request.urlopen(req, timeout=60) as resp:
        body = json.loads(resp.read().decode('utf-8'))
        content = body['choices'][0]['message']['content']
    content = content.replace('```json\n', '').replace('```\n', '').replace('```', '').strip()
    parsed = json.loads(content)
    if 'titles' in parsed and isinstance(parsed['titles'], list):
        return parsed['titles']
    for val in parsed.values():
        if isinstance(val, list) and len(val) > 0:
            return val
    return []


def get_missing_tone_counts(result, cat):
    """Return dict of tone -> count missing from the expected 10 each.
    Also checks if 'normal' base top-up (40 extra) has been done by checking
    if there are >= 90 normal titles."""
    existing = result['curated_titles'].get(cat, [])
    tone_counts = {}
    for t in existing:
        tone = t.get('tone', 'normal')
        tone_counts[tone] = tone_counts.get(tone, 0) + 1

    missing = {}
    # Check normal: we want >= 90 normal titles (50 original + 40 new)
    normal_count = tone_counts.get('normal', 0)
    if normal_count < 90:
        need = 90 - normal_count
        missing[('normal', 'any', need)] = need

    # Check each non-normal tone: we want 10 each
    for tone in TONES:
        ct = tone_counts.get(tone, 0)
        if ct < 10:
            need = 10 - ct
            missing[(tone, 'any', need)] = need

    return missing


def main():
    random.seed(42)
    with open(OUTPUT_PATH) as f:
        result = json.load(f)

    total_generated = 0

    for cat in CATEGORIES:
        existing = result['curated_titles'].get(cat, [])
        missing = get_missing_tone_counts(result, cat)
        if not missing:
            print(f'{cat}: DONE ({len(existing)} titles, all tones covered)', file=sys.stderr)
            continue

        print(f'\n{cat}: {len(existing)} titles, need to generate:', file=sys.stderr)
        for (tone, genre, count), _ in missing.items():
            print(f'  - {tone}: {count} titles', file=sys.stderr)

        for (tone, genre, count), _ in missing.items():
            tag = f'{cat}/{tone}/{genre}'
            print(f'  [{tag}] Requesting {count} titles...', file=sys.stderr, flush=True)
            try:
                prompt = build_prompt(cat, count, tone)
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
                            'genre': genre,
                            'tone': tone,
                            'appeal_score': random.randint(70, 100),
                            'notes': ''
                        })

                result['curated_titles'][cat].extend(normalized)
                total_generated += len(normalized)
                print(f'  [{tag}] Got {len(normalized)} titles', file=sys.stderr, flush=True)

                # Save after each batch
                tmp = OUTPUT_PATH + '.tmp'
                with open(tmp, 'w') as f:
                    json.dump(result, f, indent=2)
                os.rename(tmp, OUTPUT_PATH)

                time.sleep(0.6)

            except Exception as e:
                print(f'  [{tag}] ERROR: {e}', file=sys.stderr, flush=True)
                # Still save what we have
                tmp = OUTPUT_PATH + '.tmp'
                with open(tmp, 'w') as f:
                    json.dump(result, f, indent=2)
                os.rename(tmp, OUTPUT_PATH)

        print(f'  {cat} now has {len(result["curated_titles"][cat])} titles', file=sys.stderr)

    print(f'\nResume complete. Generated {total_generated} new titles this run.', file=sys.stderr)

    # Final summary
    total = sum(len(result['curated_titles'].get(c, [])) for c in CATEGORIES)
    print(f'Grand total: {total} curated titles', file=sys.stderr)


if __name__ == '__main__':
    main()
