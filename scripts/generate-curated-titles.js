#!/usr/bin/env node
/**
 * generate-curated-titles.js
 * Regenerates ~800 curated titles across 16 categories via DeepSeek V4 Flash.
 * Outputs JSON that can be merged into seed-data.json.
 *
 * Usage: DEEPSEEK_API_KEY=sk-xxx node scripts/generate-curated-titles.js
 * Output: writes to curated-titles-output.json
 *
 * Cost estimate: ~$0.50-$1.00 (50 titles x 16 categories = 800 titles)
 */

const https = require('https');
const fs = require('fs');

const API_KEY = process.env.DEEPSEEK_API_KEY;
if (!API_KEY) {
  console.error('ERROR: DEEPSEEK_API_KEY environment variable not set');
  process.exit(1);
}

const CATEGORIES = [
  'book', 'article', 'blog', 'movie', 'song', 'youtube',
  'podcast', 'newsletter', 'ebook', 'speech', 'album',
  'poem', 'street', 'character', 'product', 'childname'
];

const TITLES_PER_CATEGORY = 50;

function callDeepSeek(prompt) {
  return new Promise((resolve, reject) => {
    const data = JSON.stringify({
      model: 'deepseek-v4-flash',
      messages: [
        {
          role: 'system',
          content: 'You are a title generation database curator. Generate high-quality, real-world titles that sound like they belong to actual published works. Return ONLY a JSON array of title objects. Each title must feel authentic and original. Vary the styles — some poetic, some direct, some provocative, some minimalist. No clichés or AI-sounding filler.'
        },
        {
          role: 'user',
          content: prompt
        }
      ],
      temperature: 0.85,
      max_tokens: 4096,
      response_format: { type: 'json_object' }
    });

    const options = {
      hostname: 'api.deepseek.com',
      path: '/v1/chat/completions',
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${API_KEY}`,
        'Content-Length': Buffer.byteLength(data)
      }
    };

    const req = https.request(options, (res) => {
      let body = '';
      res.on('data', (chunk) => body += chunk);
      res.on('end', () => {
        try {
          const parsed = JSON.parse(body);
          const content = parsed.choices?.[0]?.message?.content;
          if (!content) {
            reject(new Error('No content in response: ' + body));
            return;
          }
          const cleaned = content.replace(/```json\n?/g, '').replace(/```\n?/g, '').trim();
          const result = JSON.parse(cleaned);
          resolve(result);
        } catch (e) {
          reject(new Error('Failed to parse response: ' + e.message + '\nBody: ' + body));
        }
      });
    });

    req.on('error', reject);
    req.write(data);
    req.end();
  });
}

function buildPrompt(category, count) {
  const prompts = {
    book: `Generate ${count} real, published-style book titles (fiction and non-fiction) across various genres. Include a mix of literary, commercial, memoir, and genre fiction titles. Make them feel authentic — like books you'd find on a real shelf.`,
    article: `Generate ${count} article and essay titles suitable for magazines, journals, and online publications. Mix of long-form journalism, opinion pieces, research essays, and feature articles.`,
    blog: `Generate ${count} blog post titles that are clickable but not clickbaity. Mix of how-to, listicle, opinion, and personal story formats. Should feel like real blog content.`,
    movie: `Generate ${count} movie/film titles across genres (drama, comedy, thriller, sci-fi, horror, romance, documentary). Mix of one-word titles, phrase titles, and descriptive titles.`,
    song: `Generate ${count} song titles across genres (pop, rock, hip-hop, country, electronic, indie, R&B). Mix of single-word, phrase, and poetic titles.`,
    youtube: `Generate ${count} YouTube video titles that are compelling without being clickbait. Mix of tutorials, reviews, vlogs, documentaries, and entertainment formats.`,
    podcast: `Generate ${count} podcast episode titles. Mix of interview shows, narrative podcasts, educational series, and conversational formats.`,
    newsletter: `Generate ${count} newsletter/subject line titles. Should be intriguing but professional. Mix of weekly digest, curated lists, personal updates, and industry analysis.`,
    ebook: `Generate ${count} ebook/digital book titles across self-help, business, technology, health, and personal development genres. Should feel like books you'd find on Amazon Kindle.`,
    speech: `Generate ${count} speech/presentation titles suitable for TED talks, conferences, keynote addresses, and commencement speeches. Should be inspiring and memorable.`,
    album: `Generate ${count} music album titles across genres. Mix of conceptual titles, single-word statements, and poetic phrases. Should feel like real album names from various eras.`,
    poem: `Generate ${count} poem titles. Mix of traditional, modern, and experimental poetry styles. Should be evocative and lyrical.`,
    street: `Generate ${count} street/place names that sound like real-world locations in English-speaking countries. Mix of residential streets, main roads, lanes, drives, and avenues. Include a variety of name origins.`,
    character: `Generate ${count} character names suitable for fiction. Mix of first names, full names, and character archetype names. Include a variety of cultural backgrounds and time periods.`,
    product: `Generate ${count} product/brand names suitable for real consumer products. Mix of tech products, household brands, fashion labels, and innovative product names. Should be memorable and brandable.`,
    childname: `Generate ${count} children's/baby names with brief meaning notes. Mix of traditional, modern, unique, and culturally diverse names. Each should be a real, usable name.`
  };

  return prompts[category] || `Generate ${count} titles for the category "${category}". Make them varied and authentic.`;
}

async function main() {
  const result = { curated_titles: {} };
  const errors = [];

  for (const cat of CATEGORIES) {
    console.error(`Generating ${TITLES_PER_CATEGORY} titles for: ${cat}...`);
    try {
      const prompt = buildPrompt(cat, TITLES_PER_CATEGORY);
      const response = await callDeepSeek(prompt);

      // Extract titles from various response formats
      let titles = [];
      if (Array.isArray(response)) {
        titles = response;
      } else if (response.titles && Array.isArray(response.titles)) {
        titles = response.titles;
      } else if (response[cat] && Array.isArray(response[cat])) {
        titles = response[cat];
      } else if (response.curated_titles && response.curated_titles[cat]) {
        titles = response.curated_titles[cat];
      } else {
        // Try to find any array in the response
        for (const key of Object.keys(response)) {
          if (Array.isArray(response[key]) && response[key].length > 0) {
            titles = response[key];
            break;
          }
        }
      }

      if (titles.length === 0) {
        errors.push(`${cat}: No titles returned`);
        console.error(`  WARNING: No titles returned for ${cat}`);
        continue;
      }

      // Normalize title objects to the format db.rs expects
      const normalized = titles.map((t, i) => {
        const titleStr = typeof t === 'string' ? t : (t.title || t.name || '');
        return {
          title: titleStr.trim(),
          genre: 'any',
          tone: 'normal',
          appeal_score: Math.floor(Math.random() * 30) + 70, // 70-100
          notes: ''
        };
      }).filter(t => t.title.length > 3);

      result.curated_titles[cat] = normalized;
      console.error(`  Got ${normalized.length} titles`);
    } catch (e) {
      errors.push(`${cat}: ${e.message}`);
      console.error(`  ERROR: ${e.message}`);
      result.curated_titles[cat] = [];
    }
  }

  // Write output
  const outputPath = '/home/olammy/projects/paul/titleforge-desktop/curated-titles-output.json';
  fs.writeFileSync(outputPath, JSON.stringify(result, null, 2));
  console.error(`\nDone! Output written to: ${outputPath}`);

  if (errors.length > 0) {
    console.error(`\nErrors (${errors.length}):`);
    errors.forEach(e => console.error(`  - ${e}`));
  }

  // Summary
  let total = 0;
  for (const cat of CATEGORIES) {
    const count = (result.curated_titles[cat] || []).length;
    console.error(`  ${cat}: ${count}`);
    total += count;
  }
  console.error(`\nTotal curated titles: ${total}`);
  console.error(`JSON file: ${outputPath}`);
}

main().catch(e => {
  console.error('Fatal error:', e.message);
  process.exit(1);
});
