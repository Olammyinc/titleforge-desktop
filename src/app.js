/* ============================================
   TitleForge Desktop — Application Logic
   Uses Tauri invoke() for all data operations.
   Local SQLite — no Supabase, no Netlify, no auth.
   ============================================ */

// ---- Tauri API (lazy-initialized) ----
var _invoke = null;

// ---- Streamed-results accumulator (U1) ----
// The Rust backend emits `titleforge://title-generated` once per ACCEPTED title
// during `generate_titles`. We accumulate those into a live preview area while
// the batch runs, then let the canonical displayResults() replace it on completion.
var liveTitles = [];
var streamListener = null;
var streamedCount = 0;
var streamTotal = 0;

function invoke(cmd, args) {
  if (!_invoke) {
    // Dump all Tauri-related globals for diagnostics
    console.log('[invoke setup] __TAURI__:', typeof window.__TAURI__);
    console.log('[invoke setup] __TAURI_INTERNALS__:', typeof window.__TAURI_INTERNALS__);
    console.log('[invoke setup] __TAURI__ keys:', window.__TAURI__ ? Object.keys(window.__TAURI__) : 'N/A');

    // Tauri v2 — __TAURI_INTERNALS__ is the low-level IPC injected by the Rust webview
    if (window.__TAURI_INTERNALS__) {
      console.log('[invoke setup] __TAURI_INTERNALS__ keys:', Object.keys(window.__TAURI_INTERNALS__));
      dumpDebug('invoke setup: __TAURI_INTERNALS__ found, keys: ' + Object.keys(window.__TAURI_INTERNALS__).join(','));
      if (typeof window.__TAURI_INTERNALS__.invoke === 'function') {
        _invoke = function (c, a) { return window.__TAURI_INTERNALS__.invoke(c, a); };
        dumpDebug('invoke setup: using __TAURI_INTERNALS__.invoke(cmd, args)');
      }
    }

    // Tauri v2 — __TAURI__ with core.invoke
    if (!_invoke && window.__TAURI__ && window.__TAURI__.core && typeof window.__TAURI__.core.invoke === 'function') {
      _invoke = function (c, a) { return window.__TAURI__.core.invoke(c, a); };
      dumpDebug('invoke setup: using __TAURI__.core.invoke(cmd, args)');
    }

    // Tauri v1 — __TAURI__.invoke directly
    if (!_invoke && window.__TAURI__ && typeof window.__TAURI__.invoke === 'function') {
      _invoke = function (c, a) { return window.__TAURI__.invoke(c, a); };
      dumpDebug('invoke setup: using __TAURI__.invoke(cmd, args)');
    }

    // Dev mode fallback
    if (!_invoke) {
      console.warn('[TitleForge] No Tauri IPC bridge found — using dev mode mock.');
      dumpDebug('invoke setup: NO Tauri IPC found — falling back to DEV MODE MOCK');
      window.__TF_DEV_MODE = true;
      // Show a visible indicator in the app
      var devBanner = document.createElement('div');
      devBanner.style.cssText = 'position:fixed;top:0;left:0;right:0;z-index:99999;background:#dc2626;color:#fff;text-align:center;padding:8px 16px;font:13px sans-serif;';
      devBanner.textContent = '⚠ Dev Mode: Tauri IPC not found. Check console for details.';
      document.body.prepend(devBanner);
      var mockDb = { license_status: '', settings: {} };
      _invoke = function (cmd, args) {
        if (cmd === 'get_settings') return Promise.resolve(mockDb.settings);
        if (cmd === 'validate_license') { mockDb.settings.license_status = 'valid'; mockDb.settings.license_tier = 'pro'; return Promise.resolve({ valid: true, tier: 'pro' }); }
        if (cmd === 'get_categories') return Promise.resolve([]);
        if (cmd === 'get_history' || cmd === 'get_favorites' || cmd === 'get_projects') return Promise.resolve([]);
        if (cmd === 'get_usage_stats') return Promise.resolve({ totalGenerations: 0, todayGenerations: 0, totalFavorites: 0, isPro: true, tier: 'pro' });
        if (cmd === 'record_generation' || cmd === 'set_setting' || cmd === 'deactivate_license') return Promise.resolve();
        if (cmd === 'generate_titles') return Promise.resolve([{ title: 'Dev Mode: Sample Title', score: 85, categories: ['book'], breakdown: null, source: 'template', seo_score: 82, seo_breakdown: { platform: 'amazon', length_fit: { score: 90, weight: 20, value: '24 chars', detail: '24 chars — acceptable for amazon' }, keyword_presence: { score: 100, weight: 20, value: 'front-loaded', detail: 'keyword leads the title' }, keyword_density: { score: 100, weight: 10, value: '20%', detail: 'density in sweet spot' }, search_pattern: { score: 60, weight: 15, value: '1 match(es)', detail: 'matched: guide to' }, question_format: { score: 0, weight: 5, value: 'no', detail: 'not a question' }, number_year: { score: 0, weight: 10, value: 'none', detail: 'no numbers present' }, reading_level: { score: 60, weight: 5, value: 'n/a', detail: 'too short' }, power_words: { score: 60, weight: 5, value: '1 power word(s)', detail: '1 power word' }, uniqueness: { score: 85, weight: 10, value: '82% novel', detail: 'low overlap — mostly novel' } } }]);
        if (cmd === 'get_app_info') return Promise.resolve({ version: '0.0.0-devmock', seeded: false, templateCount: 0, localLlmLoaded: false });
        if (cmd === 'get_model_status') return Promise.resolve({ qwenPresent: true, qwenSize: 986048768, downloadDone: 0, downloadTotal: 0, downloadFinished: null });
        if (cmd === 'start_model_download') return Promise.resolve();
        return Promise.reject(new Error('Tauri API not available in dev mode for: ' + cmd));
      };
    }
  }
  return _invoke(cmd, args);
}

// ---- CONFIG ----
const FREE_MAX_TITLES = 10;
const ALL_CATEGORIES = [
  { id: 'book',      label: 'Book titles',          free: true  },
  { id: 'article',   label: 'Article titles',        free: true  },
  { id: 'blog',      label: 'Blog post titles',      free: true  },
  { id: 'movie',     label: 'Movie / film titles',   free: true  },
  { id: 'song',      label: 'Song titles',           free: true  },
  { id: 'youtube',   label: 'YouTube video titles',  free: true  },
  { id: 'podcast',   label: 'Podcast episode titles', free: true },
  { id: 'newsletter',label: 'Newsletter titles',     free: true  },
  { id: 'ebook',     label: 'eBook titles',          free: true  },
  { id: 'speech',    label: 'Speech titles',         free: true  },
  { id: 'album',     label: 'Music album titles',    free: true  },
  { id: 'poem',      label: 'Poem titles',           free: true  },
  { id: 'street',    label: 'Street / place names',  free: true  },
  { id: 'character', label: 'Character names',       free: true  },
  { id: 'product',   label: 'Product names',         free: true  },
  { id: 'childname', label: "Children's names",      free: true  },
];

const STYLES = [
  { id: 'normal',      label: 'Normal',       free: true  },
  { id: 'shout',       label: 'Bold / Shout', free: true  },
  { id: 'whisper',     label: 'Subtle / Whisper', free: true  },
  { id: 'blessing',    label: 'Uplifting / Blessing', free: true  },
  { id: 'provocative', label: 'Provocative',  free: true  },
  { id: 'minimalist',  label: 'Minimalist',   free: true  },
  { id: 'storytelling',label: 'Storytelling', free: true  },
  { id: 'question',    label: 'Question',     free: true  },
  { id: 'playful',     label: 'Playful',      free: true  },
];

const BREAKDOWN_FIELDS = [
  { key: 'curiosityGap',     label: 'Curiosity gap',  tip: 'How much the title makes you want to know more.' },
  { key: 'emotionalTrigger', label: 'Emotion',         tip: 'The emotion the title evokes.' },
  { key: 'powerWords',       label: 'Power words',     isArray: true, tip: 'Words that carry emotional weight.' },
  { key: 'lengthAnalysis',   label: 'Length',          tip: 'Whether the length is optimal for its medium.' },
  { key: 'specificity',      label: 'Specificity',     tip: 'How concrete vs abstract the title is.' },
  { key: 'uniqueness',       label: 'Uniqueness',      tip: 'How distinctive the name is.' },
  { key: 'memorability',     label: 'Memorability',    tip: 'How easy to remember and pronounce.' },
  { key: 'meaningDepth',     label: 'Meaning depth',   tip: 'Depth of meaning or cultural significance.' },
  { key: 'pronunciationEase',label: 'Pronunciation',   tip: 'How easy the name is to say aloud.' },
  { key: 'originVibe',       label: 'Origin / vibe',   tip: 'Cultural origin or overall feel.' },
];

// ---- SEO SCORING HELPERS ----
const SEO_SIGNALS = [
  { key: 'length_fit',        label: 'Length fit' },
  { key: 'keyword_presence',  label: 'Keyword presence' },
  { key: 'keyword_density',   label: 'Keyword density' },
  { key: 'search_pattern',    label: 'Search patterns' },
  { key: 'question_format',   label: 'Question format' },
  { key: 'number_year',       label: 'Numbers & year' },
  { key: 'reading_level',     label: 'Reading level' },
  { key: 'power_words',       label: 'Power words' },
  { key: 'uniqueness',        label: 'Uniqueness' },
];

function seoTier(score) {
  if (score >= 80) return 'seo-high';
  if (score >= 50) return 'seo-mid';
  return 'seo-low';
}

function renderSeoBreakdownHtml(bd) {
  if (!bd) return '';
  var html = '';
  if (bd.platform) {
    html += '<div class="seo-platform">Target platform: <strong>' + escapeHtml(String(bd.platform)) + '</strong></div>';
  }
  SEO_SIGNALS.forEach(function (sig) {
    var s = bd[sig.key];
    if (!s) return;
    var cls = s.score >= 75 ? 'high' : (s.score >= 50 ? 'medium' : 'low');
    html += '<div class="seo-signal-row">';
    html += '<span class="seo-signal-label">' + sig.label + '</span>';
    html += '<span class="seo-signal-value">' + (s.value ? escapeHtml(String(s.value)) : '—') + '</span>';
    html += '<span class="seo-signal-score ' + cls + '">' + s.score + '</span>';
    html += '</div>';
    if (s.detail) { html += '<div class="seo-signal-detail">' + escapeHtml(String(s.detail)) + '</div>'; }
  });
  return html;
}

function toggleSeoPanel(hostDiv, bd) {
  var body = hostDiv.querySelector('.result-body');
  if (!body) return;
  var existing = body.querySelector('.seo-panel');
  if (existing) { existing.classList.toggle('show'); return; }
  if (!bd) return;
  var panel = document.createElement('div');
  panel.className = 'seo-panel show';
  panel.innerHTML = renderSeoBreakdownHtml(bd);
  body.appendChild(panel);
}

// ---- STATE ----
var isPro = false;
var isLoggedIn = true;
var currentTier = 'core';
var isGuest = false;
var selectedStyle = 'normal';
var selectedGender = 'any';
var dailyUsage = 0;
var activeEngine = 'auto';
var aiProvider = '';
var aiApiKey = '';

// Dashboard state
var dashHistory = [];
var dashFavorites = [];
var dashProjects = [];
var dashCurrentTab = 'overview';
var dashSearchQuery = '';
var dashFilterCategory = '';
var dashFilterSort = 'newest';
var genCountThisSession = 0;

// ---- HELPERS ----
function escapeHtml(str) {
  if (!str) return '';
  return String(str)
    .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;').replace(/'/g, '&#039;');
}

// Maps the backend's TitleResult.source value to a short, user-facing label.
// Lets people (and us, when debugging) tell which generator actually
// produced a given result instead of guessing from its shape.
function engineSourceLabel(source) {
  switch (source) {
    case 'local-llm': return 'Offline engine';
    case 'ai': return 'AI · cloud';
    case 'egcg-a': case 'egcg-b': case 'egcg-c': return 'Offline engine';
    case 'template': return 'Offline engine (basic)';
    default: return source;
  }
}

function createTipBtn(tipText) {
  var btn = document.createElement('span');
  btn.className = 'tip-btn';
  btn.textContent = '?';
  btn.setAttribute('role', 'button');
  btn.setAttribute('tabindex', '0');
  btn.addEventListener('click', function (e) {
    e.stopPropagation();
    var existing = document.querySelector('.tip-popup');
    if (existing) { existing.remove(); return; }
    var popup = document.createElement('div');
    popup.className = 'tip-popup';
    popup.textContent = tipText;
    var rect = btn.getBoundingClientRect();
    popup.style.left = Math.min(rect.left, window.innerWidth - 220) + 'px';
    popup.style.top = (rect.bottom + 6) + 'px';
    document.body.appendChild(popup);
    setTimeout(function () {
      document.addEventListener('click', function closeTip(ev) {
        var p = document.querySelector('.tip-popup');
        if (p && !ev.target.closest('.tip-btn')) { p.remove(); }
        document.removeEventListener('click', closeTip);
      });
    }, 10);
  });
  return btn;
}

function csvEscape(str) {
  if (!str) return '';
  str = String(str);
  if (str.indexOf(',') !== -1 || str.indexOf('"') !== -1 || str.indexOf('\n') !== -1) {
    return '"' + str.replace(/"/g, '""') + '"';
  }
  return str;
}

function downloadFile(content, filename, mimeType) {
  var blob = new Blob([content], { type: mimeType });
  var url = URL.createObjectURL(blob);
  var a = document.createElement('a');
  a.href = url; a.download = filename;
  document.body.appendChild(a); a.click();
  document.body.removeChild(a); URL.revokeObjectURL(url);
}

// ---- SIDEBAR NAVIGATION ----
function setupSidebarNav() {
  var items = document.querySelectorAll('.sidebar-item');
  var views = {
    generator: document.getElementById('viewGenerator'),
    dashboard: document.getElementById('viewDashboard'),
    settings: document.getElementById('viewSettings'),
  };
  var title = document.getElementById('pageTitle');

  items.forEach(function (item) {
    item.addEventListener('click', function (e) {
      e.preventDefault();
      var view = item.getAttribute('data-view');
      items.forEach(function (i) { i.classList.remove('active'); });
      item.classList.add('active');
      Object.keys(views).forEach(function (k) {
        if (views[k]) views[k].classList.remove('active');
      });
      if (views[view]) views[view].classList.add('active');
      if (title) {
        var titleMap = { dashboard: 'Overview', generator: 'Generator', settings: 'Settings' };
        title.textContent = titleMap[view] || (view.charAt(0).toUpperCase() + view.slice(1));
      }
      if (view === 'dashboard') {
        renderDashboard();
      }
      if (view === 'settings') {
        renderSettingsContent();
      }
    });
  });
}

function switchToGenerator() {
  var gen = document.querySelector('.sidebar-item[data-view="generator"]');
  if (gen) gen.click();
}

function switchToDashboard() {
  var dash = document.querySelector('.sidebar-item[data-view="dashboard"]');
  if (dash) dash.click();
}

// ---- LICENSE ACTIVATION ----
document.addEventListener('DOMContentLoaded', function () {
  _flushDebugLog(); // flush any diagnostic messages queued before DOM ready
  var activationScreen = document.getElementById('activationScreen');
  var mainApp = document.getElementById('mainApp');

  // Wire buy links
  document.getElementById('activationBuyLink').addEventListener('click', function (e) {
    e.preventDefault();
    openBuyLink();
  });
  document.getElementById('activationBuyLink2').addEventListener('click', function (e) {
    e.preventDefault();
    openBuyLink();
  });

  // Wire activation button
  document.getElementById('activationBtn').addEventListener('click', handleActivation);
  document.getElementById('activationKey').addEventListener('keydown', function (e) { if (e.key === 'Enter') handleActivation(); });
  document.getElementById('activationEmail').addEventListener('keydown', function (e) { if (e.key === 'Enter') handleActivation(); });

  // Check license — always validate with server, don't trust local SQLite alone
  invoke('get_settings').then(function (settings) {
    var savedKey = settings.license_key || '';
    var savedEmail = settings.license_email || '';
    var cachedStatus = settings.license_status || '';

    if (savedKey && savedEmail) {
      // We have saved credentials — validate with server (Rust handles 24h cache fallback)
      invoke('validate_license', { key: savedKey, email: savedEmail }).then(function (result) {
        if (result && result.valid) {
          activationScreen.style.display = 'none';
          mainApp.style.display = 'flex';
          initApp();
        }
        // else: show activation screen (below)
      }).catch(function () {
        // Server unreachable — use cached status if within 24h
        // (Rust validate_license already handles this, but if IPC failed entirely)
        if (cachedStatus === 'valid') {
          activationScreen.style.display = 'none';
          mainApp.style.display = 'flex';
          initApp();
        }
      });
    } else if (cachedStatus === 'valid') {
      // Legacy: cached status without saved credentials — still show the app
      // (less secure but maintains backward compatibility)
      activationScreen.style.display = 'none';
      mainApp.style.display = 'flex';
      initApp();
    }
  }).catch(function (err) { console.error('get_settings on init failed:', err); });
});

function openBuyLink() {
  var url = 'https://titleforge-tool.netlify.app/desktop';
  // Fallback until domain is live: use GitHub releases page
  // Try Tauri shell open — check __TAURI_INTERNALS__ first (earliest available), then __TAURI__
  var ipc = (window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke)
    || (window.__TAURI__ && window.__TAURI__.invoke);
  if (ipc) {
    ipc('plugin:shell|open', { path: url });
  } else {
    window.open(url, '_blank');
  }
}

function openExternalUrl(url) {
  var ipc = (window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke)
    || (window.__TAURI__ && window.__TAURI__.invoke);
  if (ipc) {
    ipc('plugin:shell|open', { path: url });
  } else {
    window.open(url, '_blank');
  }
}

function handleActivation() {
  var key = document.getElementById('activationKey').value.trim();
  var email = document.getElementById('activationEmail').value.trim();
  var errEl = document.getElementById('activationError');
  var btn = document.getElementById('activationBtn');

  if (!key || !email) {
    errEl.textContent = 'Please enter both your license key and email.';
    errEl.style.display = 'block';
    return;
  }
  errEl.style.display = 'none';
  btn.textContent = 'Activating...';
  btn.disabled = true;

  invoke('validate_license', { key: key, email: email }).then(function (result) {
    if (result.valid) {
      // Save key and email so startup can re-validate with server
      Promise.all([
        invoke('set_setting', { key: 'license_key', value: key }),
        invoke('set_setting', { key: 'license_email', value: email }),
      ]).catch(function () {});
      _bgLicenseKey = key;
      _bgLicenseEmail = email;
      document.getElementById('activationScreen').style.display = 'none';
      document.getElementById('mainApp').style.display = 'flex';
      initApp();
    } else {
      errEl.textContent = 'Invalid license key or email. Check your dashboard or try again.';
      errEl.style.display = 'block';
      btn.textContent = 'Activate';
      btn.disabled = false;
    }
  }).catch(function (err) {
    errEl.textContent = 'Could not validate license: ' + err;
    errEl.style.display = 'block';
    btn.textContent = 'Activate';
    btn.disabled = false;
  });
}

function initApp() {
  setupSidebarNav();
  renderCategories();
  setupStyleButtons();
  setupGenderButtons();
  setupFineTune();
  setupTranslateToggle();
  setupSlider();
  setupEngineToggle();
  setupGenerateButton();
  setupModelDownloadButton();
  setupEnginePrompt();
  setupDashboardTabs();
  setupDashboardSearch();
  setupExportButtons();
  setupProjects();
  populateDashFilters();
  updateUsageDisplay();
  loadDashboardData();

  // Load AI settings
  invoke('get_settings').then(function (settings) {
    aiProvider = settings.ai_provider || '';
    aiApiKey = settings.ai_api_key || '';
    if (aiProvider && aiApiKey) {
      var el = document.getElementById('engineStatus');
      if (el) el.textContent = aiProvider.charAt(0).toUpperCase() + aiProvider.slice(1) + ' key ready';
    }
    // Show first-launch prompt if no API key configured
    promptApiKeySetup();
  }).catch(function (err) { console.error('get_settings for AI provider failed:', err); });

  // Auto-update check on launch (with small delay so UI renders first)
  setupUpdaterEvents();
  setTimeout(setupUpdaterAutoCheck, 800);

  // Start background license verification + update checking (every 30 min)
  startBackgroundTasks();
}

// ---- BACKGROUND TASKS ----
var _bgLicenseKey = '';
var _bgLicenseEmail = '';

function startBackgroundTasks() {
  invoke('get_settings').then(function (settings) {
    _bgLicenseKey = settings.license_key || '';
    _bgLicenseEmail = settings.license_email || '';
  }).catch(function () {});

  setInterval(function () {
    if (!_bgLicenseKey || !_bgLicenseEmail) return;
    // Quick connectivity check before trying background operations
    invoke('background_verify', { key: _bgLicenseKey, email: _bgLicenseEmail })
      .catch(function () {}); // Silent — never show errors to user
    checkForUpdate(true); // Silent update check — stores result, never auto-downloads
  }, 30 * 60 * 1000); // Every 30 minutes
}

// Show API key setup prompt if no key configured AND the license tier can
// actually use BYO AI (Pro/Studio). Core has no AI access, so the upsell is
// noise for Core users.
function promptApiKeySetup() {
  if (aiProvider && aiApiKey) return;              // Already have a key
  if (currentTier === 'core') return;              // Core cannot use BYO AI
  setTimeout(function () {
    var existing = document.getElementById('apiKeyNotice');
    if (existing) return;
    var notice = document.createElement('div');
    notice.id = 'apiKeyNotice';
    notice.style.cssText = 'background:linear-gradient(135deg, #E8782B, #FF9147);color:#fff;padding:12px 16px;border-radius:8px;margin-bottom:16px;font:14px var(--font-body);line-height:1.5;';
    notice.innerHTML = '<strong>Want better titles?</strong> Add an API key in Settings for AI-powered generation. <a href="#" id="apiKeyNoticeLink" style="color:#fff;font-weight:700;text-decoration:underline;">Go to Settings →</a>';
    var settingsView = document.getElementById('viewSettings');
    if (settingsView && !settingsView.querySelector('#apiKeyNotice')) {
      settingsView.insertBefore(notice, settingsView.firstChild);
    }
    var link = document.getElementById('apiKeyNoticeLink');
    if (link) {
      link.addEventListener('click', function (e) {
        e.preventDefault();
        var settingsItem = document.querySelector('.sidebar-item[data-view="settings"]');
        if (settingsItem) settingsItem.click();
      });
    }
  }, 1000);
}

// ---- CATEGORIES ----
function renderCategories() {
  var grid = document.getElementById('categoryGrid');
  if (!grid) return;
  grid.innerHTML = '';
  ALL_CATEGORIES.forEach(function (cat) {
    var div = document.createElement('div');
    div.className = 'checkbox-item';
    var checkbox = document.createElement('input');
    checkbox.type = 'checkbox';
    checkbox.id = 'cat-' + cat.id;
    checkbox.value = cat.id;
    var label = document.createElement('label');
    label.htmlFor = 'cat-' + cat.id;
    label.style.cursor = 'pointer';
    label.textContent = cat.label;
    div.appendChild(checkbox);
    div.appendChild(label);
    div.addEventListener('click', function (e) {
      if (e.target !== checkbox && !checkbox.disabled) checkbox.checked = !checkbox.checked;
      if (checkbox.checked) div.classList.add('checked');
      else div.classList.remove('checked');
      if (cat.id === 'childname' || cat.id === 'character') {
        var anyGendered = document.querySelector('#cat-childname:checked, #cat-character:checked');
        var genderGroup = document.getElementById('genderGroup');
        if (genderGroup) genderGroup.style.display = anyGendered ? 'block' : 'none';
      }
    });
    grid.appendChild(div);
  });
}

// ---- STYLE BUTTONS ----
function setupStyleButtons() {
  var container = document.getElementById('styleRow');
  if (!container) return;
  container.querySelectorAll('.style-btn').forEach(function (btn) {
    var newBtn = btn.cloneNode(true);
    btn.parentNode.replaceChild(newBtn, btn);
    var styleId = newBtn.getAttribute('data-style');
    newBtn.addEventListener('click', function () {
      container.querySelectorAll('.style-btn').forEach(function (b) { b.classList.remove('active'); });
      newBtn.classList.add('active');
      selectedStyle = styleId;
    });
  });
}

// ---- GENDER BUTTONS ----
function setupGenderButtons() {
  document.querySelectorAll('.gender-btn').forEach(function (btn) {
    btn.addEventListener('click', function () {
      document.querySelectorAll('.gender-btn').forEach(function (b) { b.classList.remove('active'); });
      btn.classList.add('active');
      selectedGender = btn.getAttribute('data-gender') || 'any';
    });
  });
}

// ---- FINE-TUNE ----
function setupFineTune() {
  var toggle = document.getElementById('finetuneToggle');
  var panel = document.getElementById('finetunePanel');
  if (!toggle || !panel) return;
  toggle.addEventListener('click', function () {
    var isOpen = panel.style.display !== 'none';
    panel.style.display = isOpen ? 'none' : 'block';
    toggle.setAttribute('aria-expanded', String(!isOpen));
    var arrow = toggle.querySelector('.finetune-arrow');
    if (arrow) arrow.textContent = isOpen ? '\u25B8' : '\u25BE';
  });
}

function collectFineTune() {
  var ft = {};
  var map = {
    audience: 'ftAudience', emotion: 'ftEmotion', length: 'ftLength',
    angle: 'ftAngle', mustInclude: 'ftMustInclude', avoid: 'ftAvoid', beatTitle: 'ftBeat'
  };
  Object.keys(map).forEach(function (key) {
    var el = document.getElementById(map[key]);
    if (el && el.value && el.value.trim()) ft[key] = el.value.trim();
  });
  return Object.keys(ft).length ? ft : null;
}

// ---- TRANSLATE / SUBTITLE / CROSS-MEDIUM TOGGLES ----
function setupTranslateToggle() {
  var toggle = document.getElementById('translateToggle');
  var langs = document.getElementById('translateLangs');
  if (toggle && langs) {
    toggle.addEventListener('change', function () {
      langs.style.display = toggle.checked ? 'block' : 'none';
      maybeWarnAIFeature();
    });
  }
  var st = document.getElementById('subtitlesToggle');
  if (st) st.addEventListener('change', maybeWarnAIFeature);
  var cm = document.getElementById('crossMediumToggle');
  if (cm) cm.addEventListener('change', maybeWarnAIFeature);
}

// Subtitles / Translation / Cross-medium are AI-mode features (the offline
// engine only produces flat titles). If the user enables one while not in AI
// mode, tell them clearly so it's not silently ignored.
function maybeWarnAIFeature() {
  var wantAI = document.getElementById('translateToggle').checked
    || document.getElementById('subtitlesToggle').checked
    || document.getElementById('crossMediumToggle').checked;
  if (!wantAI) return;
  var usesAI = activeEngine === 'ai' && aiProvider && aiApiKey;
  if (usesAI) return; // fine — AI mode will handle it
  var offline = activeEngine === 'database' || !aiApiKey;
  if (offline) {
    showToast('Subtitles, translation & cross-medium need AI mode (add an API key in Settings).');
  }
}

// ---- SLIDER ----
function setupSlider() {
  var slider = document.getElementById('quantity');
  if (!slider) return;
  // Offline (database) engine caps were lowered 2026-07-31 (user decision):
  // measured ~7-12s/title made 100 ≈ 22 min and 500 ≈ 110 min unshippable.
  // BYOK AI mode has NO backend cap — users pay their own API bill, so give
  // them a generous slider ceiling there (Studio 1000, Pro 500).
  var aiUncapped = activeEngine === 'ai' && aiApiKey;
  if (aiUncapped) {
    if (currentTier === 'studio') slider.max = 1000;
    else if (currentTier === 'pro') slider.max = 500;
    else slider.max = 25; // Core has no AI access (backend rejects)
  } else {
    if (currentTier === 'studio') slider.max = 200;
    else if (currentTier === 'pro') slider.max = 50;
    else slider.max = 25;
  }
  updateQuantityLabel();
  slider.addEventListener('input', function () {
    updateSliderTrack(slider);
    updateQuantityLabel();
  });
}

function updateSliderTrack(slider) {
  var val = parseInt(slider.value);
  var max = parseInt(slider.max);
  var pct = (val / max) * 100;
  slider.style.background = 'linear-gradient(to right, var(--forge) 0%, var(--forge) ' + pct + '%, #E8E3D9 ' + pct + '%, #E8E3D9 100%)';
}

function updateQuantityLabel() {
  var slider = document.getElementById('quantity');
  var display = document.getElementById('qtyDisplay');
  if (!slider || !display) return;
  display.textContent = parseInt(slider.value);
}

function setupGenerateButton() {
  var btn = document.getElementById('generateBtn');
  if (!btn) return;
  btn.addEventListener('click', handleGenerate);
}

// ---- USAGE DISPLAY ----
function updateUsageDisplay() {
  var usageBar = document.getElementById('usageBar');
  var usageText = document.getElementById('usageText');
  if (!usageBar || !usageText) return;
  usageBar.style.display = 'block';
  usageBar.style.background = '#e8f5e9';
  usageBar.style.borderColor = '#c8e6c9';
  usageBar.style.color = '#2e7d32';
  var tierLabel = currentTier.charAt(0).toUpperCase() + currentTier.slice(1);
  usageText.innerHTML = tierLabel + ' — ' + dailyUsage + ' generations today';
}

// ============================================
// GENERATE TITLES
// ============================================

function setupEngineToggle() {
  var autoBtn = document.getElementById('engineAutoBtn');
  var dbBtn = document.getElementById('engineDbBtn');
  var aiBtn = document.getElementById('engineAiBtn');
  var status = document.getElementById('engineStatus');
  if (!dbBtn || !aiBtn) return;

  if (autoBtn) {
    autoBtn.addEventListener('click', function () {
      activeEngine = 'auto';
      autoBtn.classList.add('active');
      dbBtn.classList.remove('active');
      aiBtn.classList.remove('active');
      if (status) status.textContent = aiProvider ? 'Auto — AI first, falls back to the offline engine' : 'No API key — using the offline engine';
      setupSlider();
    });
  }

  dbBtn.addEventListener('click', function () {
    activeEngine = 'database';
    if (autoBtn) autoBtn.classList.remove('active');
    dbBtn.classList.add('active');
    aiBtn.classList.remove('active');
    if (status) status.textContent = 'Offline engine — always available';
    setupSlider();
  });

  aiBtn.addEventListener('click', function () {
    if (!aiProvider || !aiApiKey) {
      if (status) status.textContent = 'No API key saved. Go to Settings → AI Integration.';
      return;
    }
    activeEngine = 'ai';
    if (autoBtn) autoBtn.classList.remove('active');
    dbBtn.classList.remove('active');
    aiBtn.classList.add('active');
    if (status) status.textContent = aiProvider.charAt(0).toUpperCase() + aiProvider.slice(1) + ' — using your key';
    setupSlider();
  });
}

function handleGenerate() {
  var keyword = document.getElementById('keyword').value.trim();
  if (!keyword) { showError('Please enter a keyword or existing title.'); return; }

  var checkedCategories = [];
  document.querySelectorAll('#categoryGrid input:checked').forEach(function (cb) { checkedCategories.push(cb.value); });
  if (checkedCategories.length === 0) { showError('Please select at least one category.'); return; }

  var genre = document.getElementById('genre').value;
  var quantity = parseInt(document.getElementById('quantity').value);

  var wantCrossMedium = document.getElementById('crossMediumToggle').checked;
  var wantSubtitles = document.getElementById('subtitlesToggle').checked;
  var wantTranslation = document.getElementById('translateToggle').checked;
  var translateLang = wantTranslation ? document.getElementById('translateLang').value : null;
  var gender = selectedGender || 'any';
  var finetune = collectFineTune();

  document.getElementById('loading').style.display = 'block';
  document.getElementById('results').innerHTML = '';
  document.getElementById('error').style.display = 'none';
  document.getElementById('generateBtn').disabled = true;
  startLoadingCopy(keyword); // rotate engaging messages while the forge works

  // Reset the U1 stream accumulator for this batch.
  liveTitles = [];
  streamedCount = 0;
  streamTotal = quantity;

  // One-shot listener for the Rust stream event `titleforge://title-generated`.
  // Fires once per ACCEPTED title during `generate_titles`. We render titles as
  // they land and show live "Forging N of M..." progress. A single listener is
  // registered per generation and always removed on completion/error (finally).
  if (window.__TAURI__ && window.__TAURI__.event) {
    window.__TAURI__.event.listen('titleforge://title-generated', function (ev) {
      var payload = ev.payload || {};
      // Guard against malformed/duplicate payloads.
      if (typeof payload.title !== 'string' || !payload.title) return;
      streamedCount++;
      liveTitles.push(payload.title);
      updateLiveProgress();
      appendLiveTitle(payload.title);
    }).then(function (unlisten) {
      // `unlisten` is the cleanup function returned by Tauri's listen().
      streamListener = unlisten;
    }).catch(function (e) {
      dumpDebug('Stream listener setup failed: ' + e);
    });
  }

  var genPromise;

  if (activeEngine === 'ai' && aiProvider && aiApiKey) {
    // Pure AI mode
    dumpDebug('handleGenerate: using AI engine (' + aiProvider + '), keyword=' + keyword + ', cats=' + checkedCategories.join(',') + ', qty=' + quantity);
    genPromise = invoke('generate_with_ai', {
      keyword: keyword,
      categories: checkedCategories,
      style: selectedStyle,
      genre: genre,
      quantity: quantity,
      cross_medium: wantCrossMedium,
      include_subtitles: wantSubtitles,
      include_translation: wantTranslation,
      translate_lang: translateLang,
      gender: gender,
      finetune: finetune,
      provider: aiProvider,
      api_key: aiApiKey,
    });
  } else if (activeEngine === 'auto' && aiProvider && aiApiKey) {
    // Auto mode: try AI first, fall back to database
    dumpDebug('handleGenerate: auto mode, trying AI first (' + aiProvider + '), keyword=' + keyword + ', cats=' + checkedCategories.join(',') + ', qty=' + quantity);
    genPromise = invoke('generate_with_ai', {
      keyword: keyword,
      categories: checkedCategories,
      style: selectedStyle,
      genre: genre,
      quantity: quantity,
      cross_medium: wantCrossMedium,
      include_subtitles: wantSubtitles,
      include_translation: wantTranslation,
      translate_lang: translateLang,
      gender: gender,
      finetune: finetune,
      provider: aiProvider,
      api_key: aiApiKey,
    }).catch(function (aiErr) {
      // AI failed — fall back to database
      dumpDebug('AI failed in auto mode, falling back to database: ' + (aiErr.message || aiErr));
      var statusEl = document.getElementById('engineStatus');
      if (statusEl) statusEl.textContent = 'AI unavailable — using the offline engine';
      return invoke('generate_titles', {
        keyword: keyword,
        categories: checkedCategories,
        style: selectedStyle,
        genre: genre,
        quantity: quantity,
        finetune: finetune,
      });
    });
  } else {
    // Database mode (or auto without API key)
    dumpDebug('handleGenerate: using DB engine, keyword=' + keyword + ', cats=' + checkedCategories.join(',') + ', qty=' + quantity);
    genPromise = invoke('generate_titles', {
      keyword: keyword,
      categories: checkedCategories,
      style: selectedStyle,
      genre: genre,
      quantity: quantity,
      finetune: finetune,
    });
  }

  genPromise.then(function (titles) {
    dumpDebug('generate: SUCCESS, titles count=' + (titles ? titles.length : 'null'));
    displayResults(titles, keyword);
    var count = (titles && titles.length) || 0;
    if (count === 0) {
      showToast('No titles generated. Try a different keyword, category, or AI mode.');
    } else if (count < quantity) {
      showToast(count + (count === 1 ? ' title forged' : ' titles forged') + ' of ' + quantity + ' requested — duplicates and low-quality candidates were filtered out.');
    } else {
      showToast(count + (count === 1 ? ' title forged —' : ' titles forged —') + ' ready to publish.');
    }
    if (count > 0) {
      dailyUsage++;
      invoke('record_generation', {
        keyword: keyword,
        categories: checkedCategories,
        genre: genre,
        style: selectedStyle,
        titles: titles,
      }).catch(function (err) { console.error('record_generation failed:', err); });
      updateUsageDisplay();
      saveToHistoryLocal(keyword, checkedCategories, genre, selectedStyle, titles);
      genCountThisSession++;
    }
  }).catch(function (err) {
    var errMsg = typeof err === 'string' ? err : (err.message || 'Something went wrong. Please try again.');
    dumpDebug('generate: FAILED — ' + errMsg + ' (err type: ' + (typeof err) + ', keys: ' + (err && typeof err === 'object' ? Object.keys(err).join(',') : 'N/A') + ')');
    showError(errMsg);
  }).finally(function () {
    // U1 cleanup: always remove the stream listener so repeated generations
    // never stack duplicate listeners. Persistence stays gated on completion
    // (displayResults + record_generation + history run in .then), so a
    // cancelled/failed run never writes partial history.
    if (streamListener) {
      try { streamListener(); } catch (e) { /* ignore */ }
      streamListener = null;
    }
    document.getElementById('loading').style.display = 'none';
    document.getElementById('generateBtn').disabled = false;
    stopLoadingCopy();
  });
}

// --- U1 live-streaming helpers ---
// Update the "Forging N of M..." counter shown next to the loading area.
function updateLiveProgress() {
  if (streamTotal <= 0) return;
  var counter = document.getElementById('liveProgress');
  if (!counter) {
    // Lazily create the counter inside the loading div so it sits next to the
    // spinner. Created here (not in index.html) to keep the change app.js-only.
    var loading = document.getElementById('loading');
    if (!loading) return;
    counter = document.createElement('p');
    counter.id = 'liveProgress';
    counter.style.textAlign = 'center';
    counter.style.color = 'var(--forge, #E8782B)';
    counter.style.marginTop = '6px';
    counter.style.fontWeight = '600';
    loading.appendChild(counter);
  }
  counter.textContent = 'Forging ' + streamedCount + ' of ' + streamTotal + '...';
}

// Append a live title row to the preview area. Uses a lightweight card with a
// pending-appeal placeholder — the canonical grid from displayResults() replaces
// this on completion.
function appendLiveTitle(title) {
  var results = document.getElementById('results');
  if (!results) return;
  var div = document.createElement('div');
  div.className = 'result-item';

  var leftCol = document.createElement('div');
  leftCol.className = 'result-left';
  var scoreNum = document.createElement('div');
  scoreNum.className = 'result-score-num';
  scoreNum.textContent = '\u2026'; // pending score placeholder
  leftCol.appendChild(scoreNum);
  div.appendChild(leftCol);

  var body = document.createElement('div');
  body.className = 'result-body';
  var titleEl = document.createElement('div');
  titleEl.className = 'result-title';
  titleEl.textContent = title;
  body.appendChild(titleEl);
  div.appendChild(body);

  results.appendChild(div);
}

// Rotate engaging, on-brand messages while titles are being generated.
// Copybank from the copywriter (Editorial Industrial voice). Keeps long
// batches feeling alive — the forge is working, not stuck.
var _loadingRotateTimer = null;
function startLoadingCopy(kw) {
  var sub = document.getElementById('loadingSub');
  var rotate = [
    'Pounding your keyword into shape.',
    'Ink\'s hot. The press never misses.',
    'Tempering each line until it rings true.',
    kw ? 'For “' + kw + '” — crafted over the fire.' : 'Crafted over the fire.',
    'Good titles take a hammer or two.',
    'Sparks fly. So do the good ideas.',
    'Every strike sharpens something.',
    'Scoring a title worth putting your name on.',
    'One pass at a time, like a master smith.',
    kw ? 'For “' + kw + '” — filing the rough edges.' : 'Filing the rough edges.',
    'Chiseling away everything that isn\'t a hook.',
  ];
  var i = 0;
  if (_loadingRotateTimer) clearInterval(_loadingRotateTimer);
  _loadingRotateTimer = setInterval(function () {
    if (!sub) return;
    sub.textContent = rotate[i % rotate.length];
    i++;
  }, 3200);
}
function stopLoadingCopy() {
  if (_loadingRotateTimer) { clearInterval(_loadingRotateTimer); _loadingRotateTimer = null; }
}

function saveToHistoryLocal(keyword, categories, genre, style, titles) {
  var entry = {
    id: Date.now(),
    keyword: keyword,
    categories: categories.join(','),
    genre: genre,
    style: style,
    titles: JSON.stringify(titles),
    created_at: new Date().toISOString(),
  };
  dashHistory.unshift(entry);
}

// ============================================
// DISPLAY RESULTS
// ============================================

function displayResults(titles, currentKeyword) {
  try {
    var container = document.getElementById('results');
    if (!container) {
      dumpDebug('displayResults: #results element NOT FOUND in DOM');
      return;
    }
    container.innerHTML = '';

    // Diagnostic: log what we received
    dumpDebug('displayResults received: type=' + (typeof titles) + ', isArray=' + Array.isArray(titles) + ', len=' + (titles ? titles.length : 'null') + ', keyword=' + currentKeyword);

    if (!titles || titles.length === 0) {
      container.innerHTML = '<p style="text-align:center;color:var(--text-secondary);padding:20px;">No titles generated. Try a different keyword or category.</p>';
      return;
    }

    // ── Revealed-preference batch id (brief §4 Task 2b + handoff 5a) ──
    // One id per generated batch. Passed to toggle_favorite so favorites made
    // from THIS batch are grouped, and the titles they passed over are known.
    // Purely local — logged to SQLite, never sent anywhere.
    var batchId = Date.now().toString(36) + '-' + Math.random().toString(36).slice(2, 8);

    // ── Display-order randomization (handoff 5a) ──
    // Position bias dominates click data — people pick from the top. For ~50%
    // of batches we SHUFFLE the display order before rendering, so favourites
    // from those batches are near-experimental rather than correlational.
    // batchTitles is built from the ACTUAL displayed order (post-shuffle), so
    // the rank the backend records is the rank the user genuinely saw.
    var displayRandomized = Math.random() < 0.5 && titles.length >= 2;
    var displayOrder = titles.slice();
    if (displayRandomized) {
      for (var i = displayOrder.length - 1; i > 0; i--) {
        var j = Math.floor(Math.random() * (i + 1));
        var tmp = displayOrder[i];
        displayOrder[i] = displayOrder[j];
        displayOrder[j] = tmp;
      }
    }
    var batchTitles = displayOrder.map(function (t) { return t.title; }).filter(Boolean);
    var seoLegend = document.getElementById('seoLegend');
    if (seoLegend) seoLegend.style.display = (titles && titles.length) ? 'flex' : 'none';

  displayOrder.forEach(function (item, idx) {
    var div = document.createElement('div');
    div.className = 'result-item';

    if (item.score !== undefined && item.score !== null) {
      var leftCol = document.createElement('div');
      leftCol.className = 'result-left';
      var scoreNum = document.createElement('div');
      scoreNum.className = 'result-score-num';
      scoreNum.textContent = item.score;
      var bar = document.createElement('div');
      bar.className = 'result-score-bar';
      var fill = document.createElement('div');
      fill.className = 'result-score-fill';
      var color = '#c62828';
      if (item.score >= 75) color = '#4caf50';
      else if (item.score >= 50) color = '#e8a040';
      else if (item.score >= 25) color = '#ff9800';
      fill.style.background = color;
      fill.style.width = item.score + '%';
      bar.appendChild(fill);
      var scoreLabel = document.createElement('div');
      scoreLabel.className = 'result-score-label';
      scoreLabel.textContent = 'appeal';
      leftCol.appendChild(scoreNum);
      leftCol.appendChild(bar);
      leftCol.appendChild(scoreLabel);
      if (item.seo_score !== undefined && item.seo_score !== null) {
        var seoPill = document.createElement('div');
        seoPill.className = 'seo-badge ' + seoTier(item.seo_score);
        seoPill.textContent = 'SEO ' + item.seo_score;
        seoPill.title = 'SEO score — click for breakdown';
        seoPill.style.cursor = 'pointer';
        (function (pill, host, bdRef) {
          pill.addEventListener('click', function () { toggleSeoPanel(host, bdRef); });
        })(seoPill, div, item.seo_breakdown);
        leftCol.appendChild(seoPill);
      }
      div.appendChild(leftCol);
    }

    var body = document.createElement('div');
    body.className = 'result-body';

    var titleEl = document.createElement('div');
    titleEl.className = 'result-title';
    titleEl.textContent = item.title;
    body.appendChild(titleEl);

    if (item.categories && item.categories.length > 0) {
      var tagsDiv = document.createElement('div');
      tagsDiv.className = 'result-tags';
      var tagsLabel = document.createElement('span');
      tagsLabel.className = 'tags-label';
      tagsLabel.textContent = 'Best for: ';
      tagsDiv.appendChild(tagsLabel);
      item.categories.forEach(function (cat) {
        var tag = document.createElement('span');
        tag.className = 'result-tag';
        tag.textContent = cat;
        tagsDiv.appendChild(tag);
      });
      if (item.source) {
        var engineTag = document.createElement('span');
        engineTag.className = 'result-engine-badge';
        engineTag.textContent = engineSourceLabel(item.source);
        engineTag.title = 'Which generator produced this title (source: "' + item.source + '")';
        tagsDiv.appendChild(engineTag);
      }
      body.appendChild(tagsDiv);
    }

    if (item.title) {
      var starBtn = document.createElement('button');
      var isFav = isFavorited(item.title);
      starBtn.className = 'result-star' + (isFav ? ' starred' : '');
      starBtn.title = 'Save to favorites';
      starBtn.innerHTML = isFav ? '\u2605' : '\u2606';
      (function (titleText, btn) {
        btn.addEventListener('click', function () {
          toggleFavorite(titleText, currentKeyword, item.score || 0, (item.categories || [''])[0], btn, batchTitles, batchId, displayRandomized);
        });
      })(item.title, starBtn);
      body.appendChild(starBtn);

      var projBtn = document.createElement('button');
      projBtn.className = 'proj-add-btn';
      projBtn.title = 'Add to project';
      projBtn.textContent = '\uD83D\uDCC1';
      projBtn.addEventListener('click', function (e) {
        e.stopPropagation();
        var existing = document.querySelector('.proj-dropdown.active');
        if (existing) {
          if (existing._title === item.title) { existing.remove(); return; }
          existing.remove();
        }
        showProjectPicker(item.title, currentKeyword, item.score || 0, projBtn);
      });
      body.appendChild(projBtn);
    }

    // Breakdown toggle
    if (item.breakdown) {
      var bdBtn = document.createElement('button');
      bdBtn.className = 'breakdown-toggle';
      bdBtn.textContent = 'Why this works';
      bdBtn.addEventListener('click', function () {
        var existing = div.querySelector('.breakdown-panel');
        if (existing) { existing.classList.toggle('show'); return; }
        var panel = document.createElement('div');
        panel.className = 'breakdown-panel show';
        var bd = item.breakdown;
        var fields = [
          { key: 'curiosityGap', label: 'Curiosity gap', val: bd.curiosityGap || 'Medium' },
          { key: 'emotionalTrigger', label: 'Emotion', val: bd.emotionalTrigger || 'neutral' },
          { key: 'powerWords', label: 'Power words', val: Array.isArray(bd.powerWords) ? bd.powerWords.join(', ') : (bd.powerWords || '—') },
          { key: 'lengthAnalysis', label: 'Length', val: bd.lengthAnalysis || '—' },
          { key: 'specificity', label: 'Specificity', val: bd.specificity || 'Abstract' },
        ];
        var html = '';
        fields.forEach(function (f) {
          var cls = 'medium';
          if (f.val === 'High' || f.val === 'Concrete') cls = 'high';
          else if (f.val === 'Low' || f.val === 'Abstract') cls = 'low';
          html += '<div class="breakdown-row"><span class="breakdown-label">' + f.label + '</span><span class="breakdown-value ' + cls + '">' + escapeHtml(String(f.val)) + '</span></div>';
        });
        panel.innerHTML = html;
        div.appendChild(panel);
      });
      body.appendChild(bdBtn);
    }

    if (item.seo_score !== undefined && item.seo_score !== null && item.seo_breakdown) {
      var seoBtn = document.createElement('button');
      seoBtn.className = 'breakdown-toggle seo-toggle';
      seoBtn.textContent = 'SEO details';
      (function (btn, host, bdRef) {
        btn.addEventListener('click', function () { toggleSeoPanel(host, bdRef); });
      })(seoBtn, div, item.seo_breakdown);
      body.appendChild(seoBtn);
    }

    div.appendChild(body);
    container.appendChild(div);
  });
  } catch (renderErr) {
    dumpDebug('displayResults CRASHED: ' + (renderErr.message || String(renderErr)));
    // Show the error in the results area so the user can see it
    var fallback = document.getElementById('results');
    if (fallback) {
      fallback.innerHTML = '<div class="error-msg" style="display:block;border:2px solid #dc2626;background:rgba(220,38,38,0.08);padding:16px;border-radius:8px;color:#b91c1c;font-size:14px;"><strong>\u26A0\uFE0F Display Error:</strong> ' + escapeHtml(renderErr.message || String(renderErr)) + '<br><small style="font-size:11px;color:#666;">This is an internal rendering error. The titles were received but could not be displayed. Check the debug log below for details.</small></div>';
    }
    showError('Display failed: ' + (renderErr.message || String(renderErr)));
  }
}

// ============================================
// ERRORS
// ============================================

function showError(msg) {
  // 1. Write to #error element
  var el = document.getElementById('error');
  if (el) {
    el.textContent = msg;
    el.style.display = 'block';
  }
  // 2. FALLBACK: always write to #results so error is visible even if #error fails
  var resultsEl = document.getElementById('results');
  if (resultsEl) {
    var existing = resultsEl.innerHTML || '';
    resultsEl.innerHTML = existing + '<div class="error-msg" style="display:block;border:2px solid #dc2626;background:rgba(220,38,38,0.08);padding:16px;border-radius:8px;margin-top:8px;color:#b91c1c;font-size:14px;font-weight:600;">\u26A0\uFE0F ' + escapeHtml(String(msg)) + '</div>';
  }
  // 3. Also dump to debug log
  dumpDebug('ERROR: ' + String(msg));
}

/**
 * Write a diagnostic message to the on-screen debug log.
 * The #debugLog element must exist in the HTML.
 * Messages sent before DOM ready are queued and flushed later.
 */
var _debugQueue = [];
function dumpDebug(msg) {
  _debugQueue.push({ time: new Date(), msg: msg });
  _flushDebugLog();
}
function _flushDebugLog() {
  var logEl = document.getElementById('debugLog');
  if (!logEl || _debugQueue.length === 0) return;
  while (_debugQueue.length > 0) {
    var item = _debugQueue.shift();
    var time = item.time.toLocaleTimeString();
    var entry = document.createElement('div');
    entry.style.cssText = 'font-family:monospace;font-size:11px;padding:2px 0;border-bottom:1px solid rgba(0,0,0,0.05);color:#333;';
    entry.textContent = '[' + time + '] ' + item.msg;
    logEl.appendChild(entry);
  }
  logEl.style.display = 'block';
  logEl.scrollTop = logEl.scrollHeight;
}

/**
 * Attach global error traps that dump to the debug log.
 */
(function setupGlobalErrorTraps() {
  window.addEventListener('error', function (e) {
    dumpDebug('GLOBAL ERROR: ' + (e.message || String(e)) + ' @ ' + (e.filename || '?') + ':' + (e.lineno || '?'));
  });
  window.addEventListener('unhandledrejection', function (e) {
    var reason = e.reason;
    dumpDebug('UNHANDLED REJECTION: ' + (reason && reason.message ? reason.message : String(reason)));
  });
})();

// ============================================
// DASHBOARD
// ============================================

function loadDashboardData() {
  Promise.all([
    invoke('get_history').catch(function () { return []; }),
    invoke('get_favorites').catch(function () { return []; }),
    invoke('get_projects').catch(function () { return []; }),
    invoke('get_usage_stats').catch(function () { return { totalGenerations: 0, todayGenerations: 0, totalFavorites: 0 }; }),
  ]).then(function (results) {
    dashHistory = results[0] || [];
    dashFavorites = results[1] || [];
    dashProjects = results[2] || [];
    dashHistory.forEach(function (entry) {
      if (typeof entry.titles === 'string') {
        try { entry.titles = JSON.parse(entry.titles); } catch (e) { entry.titles = []; }
      }
    });
    dashProjects.forEach(function (proj) {
      if (typeof proj.titles === 'string') {
        try { proj.titles = JSON.parse(proj.titles); } catch (e) { proj.titles = []; }
      }
    });
    var stats = results[3];
    dailyUsage = stats.todayGenerations || 0;
    currentTier = stats.tier || 'core';
    isPro = stats.isPro !== false;
    updateUsageDisplay();
    setupSlider(); // re-apply slider max based on real tier
    renderDashboard();
  }).catch(function (err) {
    console.error('Dashboard load error:', err);
  });
}

function renderDashboard() {
  renderStatsBar();
  renderOverviewTab();
  renderHistoryTab();
  renderFavoritesTab();
  renderProjectsTab();
  renderExportTab();
}

function renderStatsBar() {
  var container = document.getElementById('dashStats');
  if (!container) return;
  var totalTitles = 0;
  dashHistory.forEach(function (entry) {
    var titles = Array.isArray(entry.titles) ? entry.titles : [];
    totalTitles += titles.length;
  });
  container.innerHTML =
    '<div class="stat-card stat-card--titles"><span class="stat-number">' + totalTitles + '</span><span class="stat-label">Titles generated</span></div>' +
    '<div class="stat-card stat-card--favs"><span class="stat-number">' + dashFavorites.length + '</span><span class="stat-label">Favorites</span></div>' +
    '<div class="stat-card stat-card--projects"><span class="stat-number">' + dashProjects.length + '</span><span class="stat-label">Projects</span></div>';
}

// ---- OVERVIEW TAB ----
function renderOverviewTab() {
  var container = document.getElementById('dashOverviewList');
  if (!container) return;
  var html = '';
  // Tier identity pill — "what plan am I on" in context, not a 4th stat tile.
  html += '<div class="overview-tier"><span class="overview-tier-pill" title="' + currentTier.charAt(0).toUpperCase() + currentTier.slice(1) + ' plan">' + currentTier.toUpperCase() + ' \u00B7 Desktop</span>' +
         '<span class="overview-tier-note">' + (currentTier === 'core' ? 'Install the TitleForge Engine for offline titles' : (currentTier === 'pro' ? 'Bring your own AI key for larger batches' : 'All features unlocked')) + '</span></div>';
  html += '<div class="overview-card">';
  html += '<h3 class="overview-card-title">Your usage today</h3>';
  html += '<div class="usage-row"><span>' + dailyUsage + ' generation' + (dailyUsage !== 1 ? 's' : '') + '</span><span>Unlimited</span></div>';
  html += '</div>';
  var recentHistory = dashHistory.slice(0, 3);
  if (recentHistory.length > 0) {
    html += '<h3 class="overview-section-title">Recent activity</h3>';
    recentHistory.forEach(function (entry) {
      var date = new Date(entry.created_at).toLocaleDateString();
      var titles = Array.isArray(entry.titles) ? entry.titles : [];
      html += '<div class="overview-item">';
      html += '<div class="overview-item-icon">\u2726</div>';
      html += '<div class="overview-item-body"><strong>' + escapeHtml(entry.keyword) + '</strong><span class="overview-item-meta">' + titles.length + ' title' + (titles.length !== 1 ? 's' : '') + ' \u00B7 ' + date + '</span></div>';
      html += '</div>';
    });
    html += '<a href="#" onclick="switchDashTab(\'history\');return false;" class="overview-view-all">View all history \u2192</a>';
  } else {
    html += '<div class="overview-empty">';
    html += '<div class="overview-empty-icon">\uD83C\uDFAF</div>';
    html += '<h3>No titles generated yet</h3>';
    html += '<p>Go to the generator and create your first batch of titles.</p>';
    html += '<a href="#" onclick="switchToGenerator();return false;" class="btn btn-primary" style="display:inline-block;margin-top:12px;">Generate Your First Titles \u2192</a>';
    html += '</div>';
  }
  html += '<h3 class="overview-section-title" style="margin-top:24px;">Quick actions</h3>';
  html += '<div class="overview-actions">';
  html += '<a href="#" onclick="switchToGenerator();return false;" class="overview-action-btn"><span class="overview-action-icon">\u26A1</span> Generate Titles</a>';
  if (dashFavorites.length > 0) {
    html += '<a href="#" onclick="switchDashTab(\'favorites\');return false;" class="overview-action-btn"><span class="overview-action-icon">\u2605</span> Browse Favorites</a>';
  }
  if (dashProjects.length > 0) {
    html += '<a href="#" onclick="switchDashTab(\'projects\');return false;" class="overview-action-btn"><span class="overview-action-icon">\uD83D\uDCC1</span> Open Projects</a>';
  }
  html += '</div>';
  container.innerHTML = html;
}

// ---- HISTORY TAB ----
function getFilteredHistory() {
  var filtered = dashHistory.slice();
  if (dashSearchQuery) {
    var q = dashSearchQuery.toLowerCase();
    filtered = filtered.filter(function (entry) {
      if (entry.keyword && entry.keyword.toLowerCase().indexOf(q) !== -1) return true;
      var titles = Array.isArray(entry.titles) ? entry.titles : [];
      return titles.some(function (t) {
        var titleText = typeof t === 'string' ? t : t.title;
        return titleText && titleText.toLowerCase().indexOf(q) !== -1;
      });
    });
  }
  if (dashFilterCategory) {
    filtered = filtered.filter(function (entry) {
      var cats = entry.categories ? entry.categories.split(',') : [];
      return cats.indexOf(dashFilterCategory) !== -1;
    });
  }
  if (dashFilterSort === 'oldest') {
    filtered.sort(function (a, b) { return new Date(a.created_at) - new Date(b.created_at); });
  } else if (dashFilterSort === 'alpha') {
    filtered.sort(function (a, b) { return (a.keyword || '').localeCompare(b.keyword || ''); });
  } else {
    filtered.sort(function (a, b) { return new Date(b.created_at) - new Date(a.created_at); });
  }
  return filtered;
}

function renderHistoryTab() {
  var container = document.getElementById('dashHistoryList');
  if (!container) return;
  var filtered = getFilteredHistory();
  if (filtered.length === 0) {
    container.innerHTML = '<div class="dash-empty"><div class="dash-empty-icon">\uD83C\uDFAF</div><p class="dash-empty-text">' + (dashSearchQuery ? 'No results match your search.' : 'No titles generated yet.') + '</p>' + (dashSearchQuery ? '' : '<a href="#" onclick="switchToGenerator();return false;" class="btn btn-primary" style="display:inline-block;margin-top:12px;">Generate Your First Titles \u2192</a>') + '</div>';
    return;
  }
  container.innerHTML = '';
  filtered.forEach(function (entry) {
    var card = document.createElement('div');
    card.className = 'history-card';
    var date = new Date(entry.created_at).toLocaleString();
    var titles = Array.isArray(entry.titles) ? entry.titles : [];
    var header = document.createElement('div');
    header.className = 'history-header';
    header.innerHTML = '<span class="history-keyword">"' + escapeHtml(entry.keyword) + '"</span><span class="history-date">' + date + '</span>';
    card.appendChild(header);
    var meta = document.createElement('div');
    meta.className = 'history-meta';
    var cats = entry.categories ? entry.categories.split(',') : [];
    meta.innerHTML = '<span class="history-tag">' + escapeHtml(cats.join(', ')) + '</span>' + '<span class="history-tag">' + escapeHtml(entry.genre || 'any genre') + '</span>' + '<span class="history-tag">' + escapeHtml(entry.style || 'normal') + '</span>';
    card.appendChild(meta);
    var titlesList = document.createElement('div');
    titlesList.className = 'history-titles';
    titles.slice(0, 10).forEach(function (t) {
      var titleText = typeof t === 'string' ? t : t.title;
      var score = typeof t === 'object' ? t.score : null;
      var itemDiv = document.createElement('div');
      itemDiv.className = 'history-title-item';
      var textSpan = document.createElement('span');
      textSpan.style.flex = '1';
      textSpan.textContent = titleText;
      itemDiv.appendChild(textSpan);
      if (score !== null && score !== undefined) {
        var scoreBadge = document.createElement('span');
        scoreBadge.className = 'dash-score-badge';
        var scoreColor = '#c62828';
        if (score >= 75) scoreColor = '#4caf50';
        else if (score >= 50) scoreColor = '#e8a040';
        else if (score >= 25) scoreColor = '#ff9800';
        scoreBadge.style.background = scoreColor;
        scoreBadge.textContent = score;
        itemDiv.appendChild(scoreBadge);
      }
      if (typeof t === 'object' && t.seo_score !== undefined && t.seo_score !== null) {
        var seoMini = document.createElement('span');
        seoMini.className = 'dash-seo-badge ' + seoTier(t.seo_score);
        seoMini.textContent = 'SEO ' + t.seo_score;
        seoMini.title = 'SEO score';
        itemDiv.appendChild(seoMini);
      }
      var hStar = document.createElement('button');
      var isFav = isFavorited(titleText);
      hStar.className = 'dash-star' + (isFav ? ' starred' : '');
      hStar.innerHTML = isFav ? '\u2605' : '\u2606';
      (function (titleText, entry, t, cats, hStar) {
        hStar.addEventListener('click', function () {
          var histBatch = titles.map(function (x) { return typeof x === 'string' ? x : x.title; }).filter(Boolean);
          toggleFavorite(titleText, entry.keyword, (typeof t === 'object' ? t.score : 0) || 0, cats[0] || '', hStar, histBatch, 'hist-' + entry.created_at);
        });
      })(titleText, entry, t, cats, hStar);
      itemDiv.appendChild(hStar);
      var hProj = document.createElement('button');
      hProj.className = 'dash-proj-btn';
      hProj.textContent = '\uD83D\uDCC1';
      hProj.addEventListener('click', function (e) {
        e.stopPropagation();
        var existing = document.querySelector('.proj-dropdown.active');
        if (existing) { existing.remove(); }
        showProjectPicker(titleText, entry.keyword, (typeof t === 'object' ? t.score : 0) || 0, hProj);
      });
      itemDiv.appendChild(hProj);
      titlesList.appendChild(itemDiv);
    });
    if (titles.length > 10) {
      var more = document.createElement('div');
      more.className = 'history-more';
      more.textContent = '+ ' + (titles.length - 10) + ' more';
      titlesList.appendChild(more);
    }
    card.appendChild(titlesList);
    container.appendChild(card);
  });
}

// ---- FAVORITES ----
function isFavorited(titleText) {
  return dashFavorites.some(function (f) { return f.title === titleText; });
}

function toggleFavorite(titleText, sourceKeyword, score, category, starBtn, batchTitles, batchId, displayRandomized) {
  invoke('toggle_favorite', {
    title: titleText,
    keyword: sourceKeyword || '',
    score: score || 0,
    category: category || '',
    batchTitles: batchTitles || null,
    batchId: batchId || null,
    displayRandomized: !!displayRandomized,
  }).then(function (nowFavorited) {
    if (nowFavorited) {
      dashFavorites.unshift({ title: titleText, keyword: sourceKeyword || '', score: score || 0, category: category || '', created_at: new Date().toISOString() });
      if (starBtn) { starBtn.classList.add('starred'); starBtn.innerHTML = '\u2605'; }
    } else {
      dashFavorites = dashFavorites.filter(function (f) { return f.title !== titleText; });
      if (starBtn) { starBtn.classList.remove('starred'); starBtn.innerHTML = '\u2606'; }
    }
    if (dashCurrentTab === 'favorites') renderFavoritesTab();
  }).catch(function (err) { console.error('Toggle favorite error:', err); });
}

function renderFavoritesTab() {
  var container = document.getElementById('dashFavoritesList');
  if (!container) return;
  if (dashFavorites.length === 0) {
    container.innerHTML = '<div class="dash-empty"><div class="dash-empty-icon">\u2605</div><p class="dash-empty-text">Build your collection.</p><p style="font-size:13px;color:var(--text-secondary);margin-bottom:16px;">Star any title from your history to save it here.</p><a href="#" onclick="switchDashTab(\'history\');return false;" class="btn btn-outline" style="display:inline-block;">Browse Generated Titles \u2192</a></div>';
    return;
  }
  container.innerHTML = '';
  dashFavorites.forEach(function (fav) {
    var card = document.createElement('div');
    card.className = 'history-card';
    var date = new Date(fav.created_at || Date.now()).toLocaleString();
    var header = document.createElement('div');
    header.className = 'history-header';
    header.innerHTML = '<span class="history-keyword">"' + escapeHtml(fav.title) + '"</span><span class="history-date">' + date + '</span>';
    card.appendChild(header);
    if (fav.keyword) {
      var meta = document.createElement('div');
      meta.className = 'history-meta';
      meta.innerHTML = '<span class="history-tag">From: "' + escapeHtml(fav.keyword) + '"</span>';
      card.appendChild(meta);
    }
    container.appendChild(card);
  });
}

// ---- PROJECTS ----
function renderProjectsTab() {
  var container = document.getElementById('dashProjectsList');
  if (!container) return;
  if (dashProjects.length === 0) {
    container.innerHTML = '<div class="dash-empty"><div class="dash-empty-icon">\uD83D\uDCC1</div><p class="dash-empty-text">Organize your work.</p><p style="font-size:13px;color:var(--text-secondary);margin-bottom:16px;">Group your best titles into projects for easy access.</p><a href="#" onclick="switchToGenerator();return false;" class="btn btn-primary" style="display:inline-block;">Generate Titles to Organize \u2192</a></div>';
    return;
  }
  container.innerHTML = '';
  dashProjects.forEach(function (proj) {
    var card = document.createElement('div');
    card.className = 'history-card';
    var projTitles = Array.isArray(proj.titles) ? proj.titles : [];
    var count = projTitles.length;
    var header = document.createElement('div');
    header.className = 'history-header';
    var delBtn = document.createElement('button');
    delBtn.className = 'project-delete-btn';
    delBtn.textContent = '\u2715';
    delBtn.title = 'Delete project';
    delBtn.addEventListener('click', function () { deleteProject(proj.id); });
    header.appendChild(delBtn);
    var nameSpan = document.createElement('span');
    nameSpan.className = 'history-keyword';
    nameSpan.textContent = proj.name;
    header.appendChild(nameSpan);
    var countSpan = document.createElement('span');
    countSpan.className = 'history-date';
    countSpan.textContent = count + ' title' + (count === 1 ? '' : 's');
    header.appendChild(countSpan);
    card.appendChild(header);
    if (count > 0) {
      var titlesList = document.createElement('div');
      titlesList.className = 'history-titles';
      projTitles.slice(0, 5).forEach(function (t) {
        var item = document.createElement('div');
        item.className = 'history-title-item proj-title-row';
        var titleText = typeof t === 'string' ? t : (t.title || '');
        var scoreText = (typeof t === 'object' && t.score) ? ' <span class="history-score">' + t.score + '</span>' : '';
        item.innerHTML = escapeHtml(titleText) + scoreText;
        if (typeof t === 'object') {
          var noteToggle = document.createElement('span');
          noteToggle.className = 'proj-note-toggle';
          noteToggle.textContent = t.notes ? ' \uD83D\uDCAC' : ' \u270F\uFE0F';
          noteToggle.style.cssText = 'cursor:pointer;font-size:12px;margin-left:8px;';
          noteToggle.addEventListener('click', function (e) {
            e.stopPropagation();
            var existingNote = item.querySelector('.proj-note-editor');
            if (existingNote) { existingNote.remove(); return; }
            var editor = document.createElement('div');
            editor.className = 'proj-note-editor';
            var textarea = document.createElement('textarea');
            textarea.className = 'proj-note-input';
            textarea.placeholder = 'Add a note about this title...';
            textarea.value = t.notes || '';
            textarea.rows = 2;
            var saveBtn = document.createElement('button');
            saveBtn.className = 'btn btn-small btn-primary';
            saveBtn.textContent = 'Save';
            saveBtn.style.cssText = 'margin-top:4px;padding:4px 12px;';
            saveBtn.addEventListener('click', function () {
              t.notes = textarea.value;
              invoke('update_title_notes', { projectId: proj.id, title: titleText, notes: textarea.value }).catch(function (err) { console.error('update_title_notes failed:', err); });
              noteToggle.textContent = textarea.value ? ' \uD83D\uDCAC' : ' \u270F\uFE0F';
              editor.remove();
            });
            editor.appendChild(textarea);
            editor.appendChild(saveBtn);
            item.appendChild(editor);
          });
          item.appendChild(noteToggle);
        }
        titlesList.appendChild(item);
      });
      if (count > 5) {
        var more = document.createElement('div');
        more.className = 'history-more';
        more.textContent = '+ ' + (count - 5) + ' more';
        titlesList.appendChild(more);
      }
      card.appendChild(titlesList);
    }
    container.appendChild(card);
  });
}

// Project CRUD
function setupProjects() {
  var createBtn = document.getElementById('createProjectBtn');
  var nameInput = document.getElementById('newProjectName');
  if (!createBtn || !nameInput) return;
  createBtn.addEventListener('click', function () {
    var name = nameInput.value.trim();
    if (!name) { nameInput.focus(); return; }
    createBtn.textContent = 'Creating...';
    createBtn.disabled = true;
    invoke('create_project', { name: name })
      .then(function (proj) {
        proj.titles = [];
        dashProjects.unshift(proj);
        nameInput.value = '';
        renderProjectsTab();
      })
      .catch(function (err) { alert('Could not create project: ' + (err.message || err)); })
      .finally(function () { createBtn.textContent = 'Create Project'; createBtn.disabled = false; });
  });
  nameInput.addEventListener('keydown', function (e) { if (e.key === 'Enter') { createBtn.click(); } });
}

function deleteProject(projId) {
  if (!confirm('Delete this project? Titles assigned to it will be removed.')) return;
  invoke('delete_project', { projectId: projId })
    .then(function () {
      dashProjects = dashProjects.filter(function (p) { return p.id !== projId; });
      renderProjectsTab();
      renderStatsBar();
    })
    .catch(function (err) { console.error('Delete project error:', err); });
}

function addTitleToProject(titleText, sourceKeyword, score, projId) {
  invoke('add_to_project', { projectId: projId, title: titleText, keyword: sourceKeyword || '', score: score || 0 })
    .then(function () {
      invoke('get_projects').then(function (projects) {
        projects.forEach(function (p) { if (typeof p.titles === 'string') { try { p.titles = JSON.parse(p.titles); } catch (e) { p.titles = []; } } });
        dashProjects = projects;
        if (dashCurrentTab === 'projects') renderProjectsTab();
      });
    })
    .catch(function (err) { console.error('Add to project error:', err); });
}

function showProjectPicker(titleText, sourceKeyword, score, anchorBtn) {
  if (dashProjects.length === 0) { alert('No projects yet. Create one on the Dashboard first.'); return; }
  var existing = document.querySelector('.proj-dropdown');
  if (existing) existing.remove();
  var dropdown = document.createElement('div');
  dropdown.className = 'proj-dropdown active';
  dropdown._title = titleText;
  var label = document.createElement('div');
  label.className = 'proj-dropdown-label';
  label.textContent = 'Add to project:';
  dropdown.appendChild(label);
  dashProjects.forEach(function (proj) {
    var item = document.createElement('div');
    item.className = 'proj-dropdown-item';
    item.textContent = proj.name;
    item.addEventListener('click', function () {
      addTitleToProject(titleText, sourceKeyword, score, proj.id);
      dropdown.textContent = '\u2713 Added!';
      dropdown.style.color = '#16a34a';
      dropdown.style.padding = '12px';
      dropdown.style.fontWeight = '600';
      setTimeout(function () { dropdown.remove(); }, 1200);
    });
    dropdown.appendChild(item);
  });
  var rect = anchorBtn.getBoundingClientRect();
  dropdown.style.position = 'fixed';
  dropdown.style.top = (rect.bottom + 4) + 'px';
  dropdown.style.left = Math.max(4, Math.min(rect.left, window.innerWidth - 200)) + 'px';
  document.body.appendChild(dropdown);
  setTimeout(function () {
    document.addEventListener('click', function closeDrop(ev) {
      var d = document.querySelector('.proj-dropdown');
      if (d && !ev.target.closest('.proj-add-btn') && !ev.target.closest('.dash-proj-btn')) { d.remove(); }
      document.removeEventListener('click', closeDrop);
    });
  }, 10);
}

// ---- EXPORT ----
function renderExportTab() {
  var preview = document.getElementById('exportPreview');
  if (!preview) return;
  var items = [];
  dashHistory.forEach(function (entry) {
    var titles = Array.isArray(entry.titles) ? entry.titles : [];
    titles.forEach(function (t) {
      var titleText = typeof t === 'string' ? t : t.title;
      var score = typeof t === 'object' ? t.score : '';
      if (titleText) {
        items.push({ title: titleText, score: score, keyword: entry.keyword || '', category: (entry.categories || '').replace(/,/g, '; '), genre: entry.genre || '', style: entry.style || '', date: entry.created_at || '' });
      }
    });
  });
  if (items.length === 0) {
    preview.innerHTML = '<div class="dash-empty"><div class="dash-empty-icon">\u2B07</div><p class="dash-empty-text">Nothing to export yet.</p><a href="#" onclick="switchToGenerator();return false;" class="btn btn-primary" style="display:inline-block;">Generate Titles \u2192</a></div>';
    return;
  }
  var html = '<div class="export-count-bar">' + items.length + ' titles — <span id="exportSelectedCount">0</span> selected</div>';
  html += '<div class="export-list">';
  items.forEach(function (item, i) {
    var scoreColor = '#c62828';
    if (item.score >= 75) scoreColor = '#4caf50';
    else if (item.score >= 50) scoreColor = '#e8a040';
    else if (item.score >= 25) scoreColor = '#ff9800';
    html += '<label class="export-item" data-index="' + i + '"><input type="checkbox" class="export-checkbox" data-index="' + i + '" /><span class="export-score" style="color:' + scoreColor + '">' + (item.score || '-') + '</span><span class="export-title">' + escapeHtml(item.title) + '</span><span class="export-meta">' + escapeHtml(item.keyword) + '</span></label>';
  });
  html += '</div>';
  preview.innerHTML = html;
  preview.querySelectorAll('.export-checkbox').forEach(function (cb) { cb.addEventListener('change', function () { var c = preview.querySelectorAll('.export-checkbox:checked').length; var el = document.getElementById('exportSelectedCount'); if (el) el.textContent = c; }); });
  var el = document.getElementById('exportSelectedCount');
  if (el) el.textContent = '0';
}

function getSelectedExportItems() {
  var preview = document.getElementById('exportPreview');
  if (!preview) return [];
  var allItems = [];
  dashHistory.forEach(function (entry) {
    var titles = Array.isArray(entry.titles) ? entry.titles : [];
    titles.forEach(function (t) {
      var titleText = typeof t === 'string' ? t : t.title;
      var score = typeof t === 'object' ? t.score : '';
      if (titleText) allItems.push({ title: titleText, score: score, keyword: entry.keyword || '', category: (entry.categories || '').replace(/,/g, '; '), genre: entry.genre || '', style: entry.style || '', date: entry.created_at || '' });
    });
  });
  var items = [];
  preview.querySelectorAll('.export-checkbox:checked').forEach(function (cb) {
    var label = cb.closest('.export-item');
    if (!label) return;
    var idx = parseInt(label.getAttribute('data-index'));
    if (allItems[idx]) items.push(allItems[idx]);
  });
  return items;
}

function setupExportButtons() {
  var exportSel = document.getElementById('exportSelectedCsv');
  if (exportSel) {
    exportSel.addEventListener('click', function () {
      var items = getSelectedExportItems();
      if (items.length === 0) { alert('Select at least one title to export.'); return; }
      var rows = [['Title', 'Score', 'Keyword', 'Category', 'Genre', 'Style', 'Date']];
      items.forEach(function (item) { rows.push([csvEscape(item.title), item.score, csvEscape(item.keyword), csvEscape(item.category), csvEscape(item.genre), csvEscape(item.style), csvEscape(item.date)]); });
      downloadFile(rows.map(function (r) { return r.join(','); }).join('\n'), 'titleforge-export.csv', 'text/csv');
    });
  }
  var copySel = document.getElementById('exportSelectedCopy');
  if (copySel) {
    copySel.addEventListener('click', function () {
      var items = getSelectedExportItems();
      if (items.length === 0) { alert('Select at least one title to copy.'); return; }
      var text = items.map(function (item) { return item.title + (item.score ? ' (' + item.score + ')' : ''); }).join('\n');
      if (navigator.clipboard) {
        navigator.clipboard.writeText(text).then(function () { copySel.textContent = 'Copied!'; setTimeout(function () { copySel.textContent = 'Copy Selected'; }, 2000); });
      } else {
        var ta = document.createElement('textarea');
        ta.value = text;
        document.body.appendChild(ta); ta.select();
        document.execCommand('copy');
        document.body.removeChild(ta);
        copySel.textContent = 'Copied!';
        setTimeout(function () { copySel.textContent = 'Copy Selected'; }, 2000);
      }
    });
  }
  var selectAllBtn = document.getElementById('exportSelectAllBtn');
  if (selectAllBtn) {
    selectAllBtn.addEventListener('click', function () {
      var preview = document.getElementById('exportPreview');
      if (!preview) return;
      preview.querySelectorAll('.export-checkbox').forEach(function (cb) { cb.checked = true; });
      var el = document.getElementById('exportSelectedCount');
      if (el) el.textContent = preview.querySelectorAll('.export-checkbox').length;
    });
  }
  var deselectAllBtn = document.getElementById('exportDeselectAllBtn');
  if (deselectAllBtn) {
    deselectAllBtn.addEventListener('click', function () {
      var preview = document.getElementById('exportPreview');
      if (!preview) return;
      preview.querySelectorAll('.export-checkbox').forEach(function (cb) { cb.checked = false; });
      var el = document.getElementById('exportSelectedCount');
      if (el) el.textContent = '0';
    });
  }
}

// ---- SETTINGS ----
function renderSettingsContent() {
  var el = document.getElementById('settingsUsage');
  if (el) el.textContent = dailyUsage;

  // Update plan label from current tier
  var planEl = document.getElementById('settingsPlan');
  if (planEl) {
    var tierLabel = currentTier.charAt(0).toUpperCase() + currentTier.slice(1);
    planEl.textContent = 'Desktop ' + tierLabel;
  }

  // Update version from app_info
  invoke('get_app_info').then(function (info) {
    var verEl = document.getElementById('settingsVersion');
    if (verEl && info.version) verEl.textContent = info.version;
    var updateVerEl = document.getElementById('settingsUpdateVersion');
    if (updateVerEl && info.version) updateVerEl.textContent = 'v' + info.version;
    // Propagate version to sidebar and activation screen
    var sidebarVer = document.getElementById('sidebarVersion');
    if (sidebarVer && info.version) sidebarVer.textContent = 'v' + info.version;
    var actVer = document.getElementById('activationVersion');
    if (actVer && info.version) actVer.textContent = 'v' + info.version;
    // Update tier badge in sidebar
    var tierBadge = document.getElementById('sidebarTierBadge');
    if (tierBadge) tierBadge.textContent = currentTier.toUpperCase();

    // Engine row reflects download + load state (shared helper also called
    // live when the download/poll updates — single source of truth).
    updateEnginePlanRow(!!info.enginePresent, !!info.localLlmLoaded);
  }).catch(function (err) { console.error('get_app_info failed:', err); });

  invoke('get_settings').then(function (settings) {
    if (settings.ai_provider) {
      var p = document.getElementById('aiProvider');
      if (p) p.value = settings.ai_provider;
    }
    if (settings.ai_api_key) {
      var ki = document.getElementById('aiApiKey');
      if (ki) ki.placeholder = 'API key saved (enter new key to change)';
    }
  }).catch(function (err) { console.error('get_settings for AI config failed:', err); });

  refreshModelStatus();
  refreshUpdateStatusOnly(); // populate the Updates Status field without auto-installing

  var saveBtn = document.getElementById('saveApiKeyBtn');
  if (saveBtn) {
    var newBtn = saveBtn.cloneNode(true);
    saveBtn.parentNode.replaceChild(newBtn, saveBtn);
    newBtn.addEventListener('click', function () {
      var provider = document.getElementById('aiProvider').value;
      var apiKey = document.getElementById('aiApiKey').value.trim();
      var statusEl = document.getElementById('aiKeyStatus');
      if (!provider || !apiKey) {
        if (statusEl) { statusEl.textContent = 'Please select a provider and enter an API key.'; statusEl.style.color = '#b91c1c'; statusEl.style.display = 'block'; }
        return;
      }
      newBtn.disabled = true;
      newBtn.textContent = 'Saving...';
      Promise.all([
        invoke('set_setting', { key: 'ai_provider', value: provider }),
        invoke('set_setting', { key: 'ai_api_key', value: apiKey }),
      ]).then(function () {
        if (statusEl) { statusEl.textContent = 'API key saved successfully.'; statusEl.style.color = '#16a34a'; statusEl.style.display = 'block'; }
        document.getElementById('aiApiKey').value = '';
        document.getElementById('aiApiKey').placeholder = 'API key saved (enter new key to change)';
      }).catch(function (err) {
        if (statusEl) { statusEl.textContent = 'Error: ' + (err.message || err); statusEl.style.color = '#b91c1c'; statusEl.style.display = 'block'; }
      }).finally(function () { newBtn.disabled = false; newBtn.textContent = 'Save API Key'; });
    });
  }

  // Wire up updater controls (Check for Updates button + auto-update toggle)
  setupUpdaterControls();
}

// ---- LOCAL AI MODEL (first-launch download) ----
var _modelPollTimer = null;

// Update the "Plan & Version" engine row live, so it reflects the engine
// state immediately (not only when the Settings panel re-renders).
//   present=true   → Qwen file downloaded (shows Installed)
//   loaded=true    → model loaded in memory (shows Active)
function updateEnginePlanRow(present, loaded) {
  var llmEl = document.getElementById('settingsLlmStatus');
  if (!llmEl) return;
  if (loaded) {
    llmEl.textContent = 'Active';
    llmEl.style.color = '#16a34a';
  } else if (present) {
    llmEl.textContent = 'Installed (generates on first title)';
    llmEl.style.color = '#16a34a';
  } else {
    llmEl.textContent = 'Off (see the TitleForge Engine card below)';
    llmEl.style.color = '';
  }
}

function refreshModelStatus() {
  // Guard on the IPC bridge, not window.__TAURI__. Tauri v2 exposes
  // __TAURI_INTERNALS__ (low-level IPC) but only populates window.__TAURI__
  // when withGlobalTauri is enabled — which it is NOT here. Checking
  // __TAURI__ made this return early and left the engine status stuck on
  // "checking…" (and the Task 4 first-run prompt dead).
  if (!window.__TAURI_INTERNALS__) { return; }
  invoke('get_model_status').then(function (s) {
    var label = document.getElementById('modelStatusLabel');
    var btn = document.getElementById('downloadModelBtn');
    var wrap = document.getElementById('modelProgressWrap');
    var bar = document.getElementById('modelProgressBar');
    var ptext = document.getElementById('modelProgressText');
    var msg = document.getElementById('modelStatusMsg');

    if (s.qwenPresent) {
      if (label) label.textContent = 'Engine status: ✓ Ready (offline titles available)';
      if (label) label.style.color = '#16a34a';
      if (btn) btn.style.display = 'none';
      if (wrap) wrap.style.display = 'none';
      if (msg) msg.textContent = '';
      stopModelPolling();
      hideEnginePrompt(); // engine is ready — the first-run banner's job is done
      updateEnginePlanRow(true, false); // reflect ready state in Plan & Version immediately
      return;
    }

    // Downloading: finished is false during the whole download, OR bytes have
    // already been received (defensive — covers any null/undefined quirk).
    var isDownloading = s.downloadFinished === false
      || (s.downloadDone != null && s.downloadDone > 0 && s.qwenPresent === false);
    if (isDownloading) {
      if (label) label.textContent = 'Engine status: downloading…';
      if (btn) btn.style.display = 'none';
      if (wrap) wrap.style.display = 'block';
      if (bar && s.downloadTotal > 0) {
        var pct = Math.min(100, Math.round((s.downloadDone / s.downloadTotal) * 100));
        bar.style.width = pct + '%';
      }
      if (ptext) ptext.textContent = formatBytes(s.downloadDone) + ' / ' + formatBytes(s.downloadTotal);
      startModelPolling();
      return;
    }

    // Not present, not downloading — offer download
    if (label) label.textContent = 'Engine status: not installed';
    if (label) label.style.color = '';
    if (btn) btn.style.display = 'block';
    if (wrap) wrap.style.display = 'none';
    if (msg && s.downloadFinished === true) {
      msg.textContent = 'Download finished — click Download again if it still shows not installed.';
      msg.style.color = '#b91c1c';
    }
    stopModelPolling();
  }).catch(function (err) {
    console.error('get_model_status failed:', err);
  });
}

function startModelPolling() {
  if (_modelPollTimer) return;
  var wasDownloading = true; // entering the loop means a download started
  _modelPollTimer = setInterval(function () {
    invoke('get_model_status').then(function (s) {
      if (s.qwenPresent) {
        refreshModelStatus();
        stopModelPolling();
        // Download completed successfully while polling.
        if (wasDownloading) {
          showToast('🎉 TitleForge Engine installed — generate offline anytime!');
          wasDownloading = false;
        }
        return;
      }
      // Update Settings progress (if visible)
      var bar = document.getElementById('modelProgressBar');
      var ptext = document.getElementById('modelProgressText');
      if (bar && s.downloadTotal > 0) {
        bar.style.width = Math.min(100, Math.round((s.downloadDone / s.downloadTotal) * 100)) + '%';
      }
      if (ptext) ptext.textContent = formatBytes(s.downloadDone) + ' / ' + formatBytes(s.downloadTotal);
      // Update the first-run banner progress too (if showing)
      var pb = document.getElementById('enginePromptProgressBar');
      var pt = document.getElementById('enginePromptProgressText');
      if (pb && s.downloadTotal > 0) {
        pb.style.width = Math.min(100, Math.round((s.downloadDone / s.downloadTotal) * 100)) + '%';
      }
      if (pt) pt.textContent = formatBytes(s.downloadDone) + ' / ' + formatBytes(s.downloadTotal);
      // Completed (success or fail): reflection stops.
      if (s.downloadFinished !== false && s.downloadFinished !== null) {
        refreshModelStatus();
        stopModelPolling();
      }
    }).catch(function () { stopModelPolling(); });
  }, 1000);
}

function stopModelPolling() {
  if (_modelPollTimer) { clearInterval(_modelPollTimer); _modelPollTimer = null; }
}

function formatBytes(n) {
  if (!n) return '0 MB';
  var mb = n / 1048576;
  if (mb > 1024) return (mb / 1024).toFixed(1) + ' GB';
  return mb.toFixed(0) + ' MB';
}

// Set BOTH the Settings card and the first-run banner into the "downloading"
// state so progress + button labels stay in lockstep regardless of which one
// started the download. `active` true = downloading, false = idle.
function setDownloadUISync(active) {
  var sBtn = document.getElementById('downloadModelBtn');
  var sWrap = document.getElementById('modelProgressWrap');
  var bBtn = document.getElementById('enginePromptDownloadBtn');
  var bWrap = document.getElementById('enginePromptProgressWrap');
  if (active) {
    if (sBtn) { sBtn.disabled = true; sBtn.textContent = 'Downloading…'; }
    if (sWrap) sWrap.style.display = 'block';
    if (bBtn) { bBtn.disabled = true; bBtn.textContent = 'Downloading…'; }
    if (bWrap) bWrap.style.display = 'block';
  } else {
    if (sBtn) { sBtn.disabled = false; sBtn.textContent = 'Download TitleForge Engine (~940 MB)'; }
    if (bBtn) { bBtn.disabled = false; bBtn.textContent = 'Download Engine'; }
  }
}

function setupModelDownloadButton() {
  var btn = document.getElementById('downloadModelBtn');
  if (!btn || !window.__TAURI_INTERNALS__) return;
  btn.addEventListener('click', function () {
    var msg = document.getElementById('modelStatusMsg');
    // Drive BOTH the Settings card and the banner into downloading state.
    setDownloadUISync(true);
    if (msg) msg.textContent = 'Downloading the offline engine (~940 MB). You can close this panel; it continues in the background.';
    invoke('start_model_download').then(function () {
      startModelPolling(); // drives progress in both Settings + banner together
    }).catch(function (err) {
      setDownloadUISync(false);
      if (msg) { msg.textContent = 'Download error: ' + (err.message || err); msg.style.color = '#b91c1c'; }
    });
  });
}

// ---- FIRST-RUN ENGINE PROMPT (Task 4: make the download prompt active) ----
// The engine download lives in Settings. A new user who never opens Settings
// never gets the offline engine — the one thing the product is sold on. This
// banner surfaces it in the main flow on first run, once, and remembers
// dismissal (engine_prompt_dismissed setting) so it doesn't nag.
function hideEnginePrompt() {
  var banner = document.getElementById('enginePromptBanner');
  if (banner && banner.style.display !== 'none') {
    banner.style.display = 'none';
    // Remember dismissal so it doesn't nag after a re-launch.
    if (window.__TAURI_INTERNALS__) {
      invoke('set_setting', { key: 'engine_prompt_dismissed', value: 'true' }).catch(function () {});
    }
  }
}

// Fire a small transient success toast (disappears after a few seconds).
function showToast(msg, type) {
  var t = document.getElementById('appToast');
  if (!t) {
    t = document.createElement('div');
    t.id = 'appToast';
    t.style.cssText = 'position:fixed;left:50%;bottom:32px;transform:translateX(-50%);z-index:99999;padding:14px 22px;border-radius:10px;font:14px var(--font-body);box-shadow:0 8px 30px rgba(0,0,0,0.25);opacity:0;transition:opacity 0.3s, bottom 0.3s;pointer-events:none;max-width:80vw;text-align:center;';
    document.body.appendChild(t);
  }
  t.textContent = msg;
  t.style.background = type === 'error' ? '#b91c1c' : '#16a34a';
  t.style.color = '#fff';
  t.style.opacity = '1';
  t.style.bottom = '32px';
  clearTimeout(t._timer);
  t._timer = setTimeout(function () {
    t.style.opacity = '0';
    t.style.bottom = '20px';
  }, 4000);
}

function setupEnginePrompt() {
  var banner = document.getElementById('enginePromptBanner');
  var dlBtn = document.getElementById('enginePromptDownloadBtn');
  var dismiss = document.getElementById('enginePromptDismiss');
  if (!banner || !window.__TAURI_INTERNALS__) return;

  var showBanner = function () {
    invoke('get_model_status').then(function (s) {
      if (!s.qwenPresent) {
        banner.style.display = 'flex';
      }
    }).catch(function () {});
  };

  // Check the dismissed flag; only show if never dismissed.
  invoke('get_settings').then(function (settings) {
    if (settings.engine_prompt_dismissed !== 'true') { showBanner(); }
  }).catch(function () { showBanner(); });

  if (dlBtn) {
    dlBtn.addEventListener('click', function () {
      // Drive BOTH the Settings card and the banner into downloading state.
      setDownloadUISync(true);
      invoke('start_model_download').then(function () {
        startModelPolling(); // progress driven in both spots together
      }).catch(function (err) {
        setDownloadUISync(false);
        dumpDebug('engine prompt download error: ' + (err.message || err));
      });
    });
  }

  if (dismiss) {
    dismiss.addEventListener('click', function () {
      banner.style.display = 'none';
      invoke('set_setting', { key: 'engine_prompt_dismissed', value: 'true' }).catch(function () {});
    });
  }
}

// ---- UPDATER ----
// State machine: idle → checking → update-available → downloading → downloaded → restart
// Uses separate Tauri IPC: check → download (returns bytesRid) → install (on explicit click)
// Auto-check may download in the background, but never installs or restarts.
var _pendingUpdate = null; // { version, rid, notes, pub_date }
var _pendingBytesRid = null; // rid returned by plugin:updater|download (bytes to install)

function setupUpdaterAutoCheck() {
  invoke('get_settings').then(function (settings) {
    if (settings.auto_update === 'true') {
       checkForUpdate(true); // silent: checks/downloads, never installs
    }
  }).catch(function (err) { console.error('get_settings for auto-update check failed:', err); });
}

// Register updater event listeners once at startup
function setupUpdaterEvents() {
  if (window.__TAURI__ && window.__TAURI__.event) {
    window.__TAURI__.event.listen('tauri://update-status', function (ev) {
      var statusEl = document.getElementById('settingsUpdateStatus');
      var checkBtn = document.getElementById('checkUpdateBtn');
      var payload = ev.payload || {};
      dumpDebug('Update status: ' + (payload.status || 'unknown') + (payload.error ? ': ' + payload.error : ''));
      if (payload.status === 'ERROR' && statusEl) {
        statusEl.textContent = 'Update failed: ' + (payload.error || 'unknown error');
        statusEl.style.color = '#b91c1c';
        _resetCheckButton(checkBtn);
      } else if (payload.status === 'DONE' && statusEl) {
        statusEl.textContent = 'Update downloaded. Restart to install.';
        statusEl.style.color = '#d97706';
        _setRestartButton(checkBtn);
      }
    }).catch(function (e) { dumpDebug('Update event listener setup failed: ' + e); });
  }
}

function setupUpdaterControls() {
  // Auto-update toggle (idempotent via _wired flag)
  var autoToggle = document.getElementById('autoUpdateToggle');
  if (autoToggle && !autoToggle._wired) {
    autoToggle._wired = true;
    invoke('get_settings').then(function (settings) {
      autoToggle.checked = settings.auto_update === 'true';
    }).catch(function (err) { console.error('get_settings for toggle load failed:', err); });
    autoToggle.addEventListener('change', function () {
      invoke('set_setting', { key: 'auto_update', value: autoToggle.checked ? 'true' : 'false' }).catch(function (err) { console.error('set_setting auto_update failed:', err); });
    });
  }

  var checkBtn = document.getElementById('checkUpdateBtn');
  if (checkBtn && !checkBtn._wired) {
    checkBtn._wired = true;
    checkBtn.addEventListener('click', function () {
      // Route click based on current button state
      if (checkBtn.dataset.updateState === 'download-available') {
        downloadPendingUpdate();
      } else if (checkBtn.dataset.updateState === 'restart-ready') {
        installAndRestart();
      } else {
        checkForUpdate(false);
      }
    });
  }
}

// Button state helpers — keeps the button and status label in sync
function _setCheckButton(checkBtn) {
  if (!checkBtn) return;
  checkBtn.dataset.updateState = '';
  checkBtn.textContent = 'Check for Updates';
  checkBtn.disabled = false;
}

function _setDownloadButton(checkBtn, version) {
  if (!checkBtn) return;
  checkBtn.dataset.updateState = 'download-available';
  checkBtn.textContent = 'Download v' + version;
  checkBtn.disabled = false;
}

function _setRestartButton(checkBtn) {
  if (!checkBtn) return;
  checkBtn.dataset.updateState = 'restart-ready';
  checkBtn.textContent = 'Restart to Install';
  checkBtn.disabled = false;
}

function _resetCheckButton(checkBtn) {
  _pendingBytesRid = null;
  _setCheckButton(checkBtn);
}

// Minimal Tauri v2 Channel bridge for updater download events. The bundled
// app uses the low-level invoke wrapper rather than the npm API package.
function createUpdaterChannel(onMessage) {
  if (!window.__TAURI_INTERNALS__ || typeof window.__TAURI_INTERNALS__.transformCallback !== 'function') return null;
  var callbackId = window.__TAURI_INTERNALS__.transformCallback(function (event) {
    if (typeof onMessage === 'function') onMessage(event);
  });
  return { toJSON: function () { return '__CHANNEL__:' + callbackId; } };
}

// STEP 1 — Check for an update. Stores the result (including rid) but never
// auto-downloads. Silent mode is used by the auto-check-on-launch path;
// manual mode shows status in the Settings panel.
function checkForUpdate(silent) {
  var statusEl = document.getElementById('settingsUpdateStatus');
  var checkBtn = document.getElementById('checkUpdateBtn');

  if (!silent && checkBtn) {
    checkBtn.disabled = true;
    checkBtn.textContent = 'Checking…';
  }
  if (!silent && statusEl) {
    statusEl.textContent = 'Checking for updates…';
    statusEl.style.color = 'var(--text-secondary)';
  }

  invoke('plugin:updater|check').then(function (result) {
    dumpDebug('Update check result: ' + JSON.stringify(result));
    if (result && result.version) {
      // Store the full update object (rid is required for download)
      _pendingUpdate = result;
      _pendingBytesRid = null; // clear any stale download from a previous version
      if (!silent && statusEl) {
        statusEl.textContent = 'Update v' + result.version + ' available.';
        statusEl.style.color = '#d97706';
      }
      if (!silent) {
        _setDownloadButton(checkBtn, result.version);
      } else {
        // Auto-update checks may download in the background, but installation
        // and restart always require an explicit user click.
        downloadPendingUpdate();
      }
    } else {
      _pendingUpdate = null;
      _pendingBytesRid = null;
      var verEl = document.getElementById('settingsUpdateVersion');
      var currentVer = (verEl && verEl.textContent) || '1.0.0';
      if (!silent && statusEl) {
        statusEl.textContent = 'You\'re up to date! (' + currentVer + ')';
        statusEl.style.color = '#16a34a';
      }
      if (!silent) {
        _setCheckButton(checkBtn);
      }
    }
  }).catch(function (err) {
    var msg = typeof err === 'string' ? err : (err.message || 'Network error');
    dumpDebug('Update check failed: ' + msg);
    _pendingUpdate = null;
    _pendingBytesRid = null;
    // Try the JS API as fallback if invoke path fails
    if (window.__TAURI__ && window.__TAURI__.updater) {
      return _checkViaJSFallback(silent, statusEl, checkBtn);
    }
    if (!silent && statusEl) {
      statusEl.textContent = 'Could not check for updates: ' + msg;
      statusEl.style.color = '#b91c1c';
    }
    if (!silent) {
      _setCheckButton(checkBtn);
    }
  });
}

// STEP 2 — Download the stored update via plugin:updater|download.
// Returns a bytesRid that is stored for the explicit install step.
// On success, transitions to restart-ready (button says "Restart to Install").
function downloadPendingUpdate() {
  var statusEl = document.getElementById('settingsUpdateStatus');
  var checkBtn = document.getElementById('checkUpdateBtn');

  if (!_pendingUpdate || !_pendingUpdate.rid) {
    if (statusEl) {
      statusEl.textContent = 'No update ready to download. Check for updates first.';
      statusEl.style.color = '#b91c1c';
    }
    return;
  }

  // Already downloaded — skip re-download, just show restart button
  if (_pendingBytesRid) {
    dumpDebug('Update already downloaded, showing restart button');
    if (statusEl) {
      statusEl.textContent = 'Update downloaded. Restart to install.';
      statusEl.style.color = '#d97706';
    }
    _setRestartButton(checkBtn);
    return;
  }

  var version = _pendingUpdate.version;
  if (checkBtn) {
    checkBtn.disabled = true;
    checkBtn.dataset.updateState = 'downloading';
    checkBtn.textContent = 'Downloading…';
  }
  if (statusEl) {
    statusEl.textContent = 'Downloading update v' + version + '…';
    statusEl.style.color = 'var(--text-secondary)';
  }

  dumpDebug('Starting download for update v' + version + ' (rid: ' + _pendingUpdate.rid + ')');

  var downloadChannel = createUpdaterChannel(function (event) {
    var payload = event && (event.message || event);
    dumpDebug('Update download event: ' + JSON.stringify(payload));
  });
  var downloadArgs = { rid: _pendingUpdate.rid };
  if (downloadChannel) downloadArgs.onEvent = downloadChannel;
  invoke('plugin:updater|download', downloadArgs).then(function (bytesRid) {
    dumpDebug('Update v' + version + ' downloaded (bytesRid: ' + bytesRid + ')');
    _pendingBytesRid = bytesRid;
    if (statusEl) {
      statusEl.textContent = 'Update downloaded. Restart to install.';
      statusEl.style.color = '#d97706';
    }
    _setRestartButton(checkBtn);
  }).catch(function (dlErr) {
    var msg = typeof dlErr === 'string' ? dlErr : (dlErr.message || 'Download failed');
    dumpDebug('Update download failed: ' + msg);
    _pendingBytesRid = null;
    // Fallback to JS API if invoke path fails
    if (window.__TAURI__ && window.__TAURI__.updater) {
      return _downloadViaJSFallback(statusEl, checkBtn, version);
    }
    if (statusEl) {
      statusEl.textContent = 'Download failed: ' + msg;
      statusEl.style.color = '#b91c1c';
    }
    _setDownloadButton(checkBtn, version);
  });
}

// STEP 3 — Install the downloaded update and restart the app.
// Only called on explicit "Restart to Install" click; never auto-invoked.
function installAndRestart() {
  var statusEl = document.getElementById('settingsUpdateStatus');
  var checkBtn = document.getElementById('checkUpdateBtn');

  if (!_pendingUpdate || !_pendingUpdate.rid || !_pendingBytesRid) {
    dumpDebug('installAndRestart called without a ready update — resetting');
    if (statusEl) {
      statusEl.textContent = 'No update downloaded. Check for updates first.';
      statusEl.style.color = '#b91c1c';
    }
    _resetCheckButton(checkBtn);
    return;
  }

  dumpDebug('Installing update via plugin:updater|install');
  if (checkBtn) {
    checkBtn.disabled = true;
    checkBtn.dataset.updateState = 'installing';
    checkBtn.textContent = 'Installing…';
  }
  if (statusEl) {
    statusEl.textContent = 'Installing update…';
    statusEl.style.color = 'var(--text-secondary)';
  }

  invoke('plugin:updater|install', { updateRid: _pendingUpdate.rid, bytesRid: _pendingBytesRid }).then(function () {
    dumpDebug('Install succeeded — restarting app');
    // Close the app; the updater applies the staged update on exit. The user
    // explicitly confirmed installation by clicking this button.
    window.close();
  }).catch(function (err) {
    var msg = typeof err === 'string' ? err : (err.message || 'Install failed');
    dumpDebug('Install failed: ' + msg);
    _pendingBytesRid = null;
    if (statusEl) {
      statusEl.textContent = 'Install failed: ' + msg + '. Try downloading again.';
      statusEl.style.color = '#b91c1c';
    }
    // Allow retry — go back to download-available if we still have the update rid
    if (_pendingUpdate && _pendingUpdate.rid) {
      _setDownloadButton(checkBtn, _pendingUpdate.version);
    } else {
      _resetCheckButton(checkBtn);
    }
  });
}

// Legacy alias kept so any stale event handler doesn't throw
function restartApp() {
  installAndRestart();
}

// ---- JS API fallbacks (used only when invoke-path fails entirely) ----
function _checkViaJSFallback(silent, statusEl, checkBtn) {
  dumpDebug('Falling back to updater JS API for check');
  return window.__TAURI__.updater.check().then(function (update) {
    if (update && update.version) {
      _pendingUpdate = { version: update.version, rid: update.rid || null, notes: update.notes, pub_date: update.pub_date };
      _pendingBytesRid = null;
      if (!silent && statusEl) {
        statusEl.textContent = 'Update v' + update.version + ' available.';
        statusEl.style.color = '#d97706';
      }
      if (!silent) {
        _setDownloadButton(checkBtn, update.version);
      }
      // If the JS API returned the update object directly, it may support
      // .downloadAndInstall() which doesn't need a separate rid
      if (update.downloadAndInstall) {
        _pendingUpdate._jsObj = update;
      }
    } else {
      _pendingUpdate = null;
      _pendingBytesRid = null;
      var verEl = document.getElementById('settingsUpdateVersion');
      var currentVer = (verEl && verEl.textContent) || '1.0.0';
      if (!silent && statusEl) {
        statusEl.textContent = 'You\'re up to date! (' + currentVer + ')';
        statusEl.style.color = '#16a34a';
      }
      if (!silent) {
        _setCheckButton(checkBtn);
      }
    }
  }).catch(function (err) {
    var msg = typeof err === 'string' ? err : (err.message || 'Network error');
    dumpDebug('Update check (JS fallback) failed: ' + msg);
    _pendingUpdate = null;
    _pendingBytesRid = null;
    if (!silent && statusEl) {
      statusEl.textContent = 'Could not check for updates: ' + msg;
      statusEl.style.color = '#b91c1c';
    }
    if (!silent) {
      _setCheckButton(checkBtn);
    }
  });
}

function _downloadViaJSFallback(statusEl, checkBtn, version) {
  dumpDebug('Falling back to updater JS API for download');
  if (_pendingUpdate && _pendingUpdate._jsObj && _pendingUpdate._jsObj.downloadAndInstall) {
    return _pendingUpdate._jsObj.downloadAndInstall().then(function () {
      _pendingBytesRid = '__js_fallback__';
      if (statusEl) {
        statusEl.textContent = 'Update downloaded. Restart to install.';
        statusEl.style.color = '#d97706';
      }
      _setRestartButton(checkBtn);
    }).catch(function (err) {
      var msg = typeof err === 'string' ? err : (err.message || 'Download failed');
      dumpDebug('Update download (JS fallback) failed: ' + msg);
      _pendingBytesRid = null;
      if (statusEl) {
        statusEl.textContent = 'Download failed: ' + msg;
        statusEl.style.color = '#b91c1c';
      }
      _setDownloadButton(checkBtn, version);
    });
  }
  // No JS object available — report error
  if (statusEl) {
    statusEl.textContent = 'Download not available. Please try again.';
    statusEl.style.color = '#b91c1c';
  }
  _setDownloadButton(checkBtn, version);
}

// Check for updates and populate the Settings "Status" field WITHOUT
// auto-downloading/installing (used when the Settings panel opens, so a user
// sees a real status instead of "—"; they still click the Check button to
// actually apply an update).
function refreshUpdateStatusOnly() {
  var statusEl = document.getElementById('settingsUpdateStatus');
  var checkBtn = document.getElementById('checkUpdateBtn');
  if (!statusEl) return;
  // Do not replace a downloaded update resource with a fresh check result;
  // the bytes rid is needed for the explicit install click.
  if (_pendingUpdate && _pendingBytesRid) {
    statusEl.textContent = 'Update downloaded. Restart to install.';
    statusEl.style.color = '#d97706';
    _setRestartButton(checkBtn);
    return;
  }
  invoke('plugin:updater|check').then(function (result) {
    if (result && result.version) {
      _pendingUpdate = result;
      _pendingBytesRid = null;
      statusEl.textContent = 'Update v' + result.version + ' available.';
      statusEl.style.color = '#d97706';
      _setDownloadButton(checkBtn, result.version);
    } else {
      _pendingUpdate = null;
      _pendingBytesRid = null;
      statusEl.textContent = 'You\'re up to date.';
      statusEl.style.color = '#16a34a';
      _setCheckButton(checkBtn);
    }
  }).catch(function () {
    _pendingUpdate = null;
    _pendingBytesRid = null;
    statusEl.textContent = 'Update check failed.';
    statusEl.style.color = '#b91c1c';
    _setCheckButton(checkBtn);
  });
}

// ---- TAB SWITCHING ----
function setupDashboardTabs() {
  var tabs = document.querySelectorAll('.dash-tab');
  tabs.forEach(function (tab) {
    tab.addEventListener('click', function () {
      tabs.forEach(function (t) { t.classList.remove('active'); });
      tab.classList.add('active');
      dashCurrentTab = tab.getAttribute('data-dashtab');
      var panels = ['overview', 'history', 'favorites', 'projects', 'export'];
      panels.forEach(function (p) {
        var panel = document.getElementById('dash' + p.charAt(0).toUpperCase() + p.slice(1));
        if (panel) panel.style.display = (p === dashCurrentTab) ? 'block' : 'none';
      });
      if (dashCurrentTab === 'export') renderExportTab();
    });
  });
}

function switchDashTab(tabName) {
  var tabs = document.querySelectorAll('.dash-tab');
  tabs.forEach(function (t) { t.classList.remove('active'); });
  tabs.forEach(function (t) { if (t.getAttribute('data-dashtab') === tabName) t.classList.add('active'); });
  dashCurrentTab = tabName;
  var panels = ['overview', 'history', 'favorites', 'projects', 'export'];
  panels.forEach(function (p) {
    var panel = document.getElementById('dash' + p.charAt(0).toUpperCase() + p.slice(1));
    if (panel) panel.style.display = (p === dashCurrentTab) ? 'block' : 'none';
  });
  if (tabName === 'export') renderExportTab();
}

// ---- SEARCH / FILTER ----
function setupDashboardSearch() {
  var search = document.getElementById('dashSearch');
  if (search) { search.addEventListener('input', function () { dashSearchQuery = search.value; renderHistoryTab(); }); }
  var filterCat = document.getElementById('dashFilterCat');
  if (filterCat) { filterCat.addEventListener('change', function () { dashFilterCategory = filterCat.value; renderHistoryTab(); }); }
  var filterSort = document.getElementById('dashFilterSort');
  if (filterSort) { filterSort.addEventListener('change', function () { dashFilterSort = filterSort.value; renderHistoryTab(); }); }
}

function populateDashFilters() {
  var filterCat = document.getElementById('dashFilterCat');
  if (!filterCat) return;
  var current = filterCat.value;
  filterCat.innerHTML = '<option value="">All categories</option>';
  ALL_CATEGORIES.forEach(function (cat) {
    var opt = document.createElement('option');
    opt.value = cat.id;
    opt.textContent = cat.label;
    filterCat.appendChild(opt);
  });
  filterCat.value = current;
}

// ---- EXPOSE GLOBALS ----
window.switchToGenerator = switchToGenerator;
window.switchToDashboard = switchToDashboard;
window.switchDashTab = switchDashTab;
window.deleteProject = deleteProject;

// ---- DEBUG LOG TOGGLE ----
(function() {
  var btn = document.getElementById('debugToggleBtn');
  if (btn) {
    btn.addEventListener('click', function() {
      var log = document.getElementById('debugLog');
      if (log) {
        var isVisible = log.style.display !== 'none';
        log.style.display = isVisible ? 'none' : 'block';
        btn.textContent = isVisible ? 'Show Debug Log' : 'Hide Debug Log';
      }
    });
  }
})();
