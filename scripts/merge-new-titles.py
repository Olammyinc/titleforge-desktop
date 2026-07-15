#!/usr/bin/env python3
"""
merge-new-titles.py
Merges new curated titles from curated-titles-output.json into seed-data.json
for BOTH repos (titleforge and titleforge-desktop).

Steps:
1. Load existing seed-data.json
2. Load new curated titles from curated-titles-output.json
3. For each category, dedup new titles against existing ones (case-insensitive)
4. Append deduped new titles to existing
5. Update stats block
6. Write to both repos
"""

import json, re, sys, os

EXISTING_SEED = '/home/olammy/projects/paul/titleforge-desktop/seed-data.json'
NEW_CURATED = '/home/olammy/projects/paul/titleforge-desktop/curated-titles-output.json'
SEED_PATHS = [
    '/home/olammy/projects/paul/titleforge-desktop/seed-data.json',
    '/home/olammy/projects/paul/titleforge/seed-data.json',
]

CATEGORIES = [
    'book', 'article', 'blog', 'movie', 'song', 'youtube',
    'podcast', 'newsletter', 'ebook', 'speech', 'album',
    'poem', 'street', 'character', 'product', 'childname'
]


def normalize_for_dedup(title):
    """Case-insensitive, punctuation-stripped normalization for dedup."""
    return re.sub(r'[^\w\s]', '', title.lower()).strip()


def main():
    # Load existing seed
    with open(EXISTING_SEED) as f:
        seed = json.load(f)

    # Load new curated titles
    with open(NEW_CURATED) as f:
        new_data = json.load(f)

    existing_curated = seed.get('curated_titles', {})
    new_curated = new_data.get('curated_titles', {})

    # Build dedup index for each category
    total_existing = 0
    dedup_idx = {}  # cat -> set of normalized titles
    for cat in CATEGORIES:
        existing = existing_curated.get(cat, [])
        total_existing += len(existing)
        dedup_idx[cat] = set()
        for t in existing:
            dedup_idx[cat].add(normalize_for_dedup(t['title']))

    print(f'Existing titles: {total_existing}')
    print(f'New titles available: {sum(len(v) for v in new_curated.values())}')

    # Merge: append new titles that aren't duplicates
    total_appended = 0
    total_skipped = 0
    for cat in CATEGORIES:
        new_titles = new_curated.get(cat, [])
        cat_appended = 0
        cat_skipped = 0

        for t in new_titles:
            norm = normalize_for_dedup(t['title'])
            if norm in dedup_idx[cat]:
                cat_skipped += 1
                continue
            dedup_idx[cat].add(norm)
            # Ensure we only keep keys that belong in seed-data.json
            clean = {
                'title': t['title'],
                'genre': t.get('genre', 'any'),
                'tone': t.get('tone', 'normal'),
                'appeal_score': t.get('appeal_score', 85),
                'notes': t.get('notes', '')
            }
            existing_curated[cat].append(clean)
            cat_appended += 1

        total_appended += cat_appended
        total_skipped += cat_skipped
        if cat_skipped > 0:
            print(f'  {cat}: +{cat_appended}, skipped {cat_skipped} duplicates')

    # Update seed's curated_titles
    seed['curated_titles'] = existing_curated

    # Update stats block
    total_templates = sum(len(v) for v in seed.get('templates', {}).values())
    total_word_pools = sum(len(v) for v in seed.get('word_pools', {}).values())
    total_curated = sum(len(v) for v in existing_curated.values())

    seed['stats'] = {
        'templates': total_templates,
        'wordPools': total_word_pools,
        'curatedTitles': total_curated,
        'curated_titles': total_curated
    }

    print(f'\nTotals: {total_templates} templates, {total_word_pools} word pool entries, {total_curated} curated titles')
    print(f'Appended: {total_appended}, Skipped duplicates: {total_skipped}')

    # Write to both repos
    json_str = json.dumps(seed, indent=2)
    for path in SEED_PATHS:
        with open(path, 'w') as f:
            f.write(json_str)
        size_kb = os.path.getsize(path) / 1024
        print(f'  Wrote {path} ({size_kb:.1f} KB)')

    print('\nMerge complete!')


if __name__ == '__main__':
    main()
