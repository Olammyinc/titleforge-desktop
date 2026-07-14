/* ============================================
   TitleForge Desktop — Application Logic
   Uses Tauri invoke() for all data operations.
   Local SQLite — no Supabase, no Netlify, no auth.
   ============================================ */

// ---- Tauri API (lazy-initialized) ----
var _invoke = null;
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
        if (cmd === 'get_usage_stats') return Promise.resolve({ totalGenerations: 0, todayGenerations: 0, totalFavorites: 0 });
        if (cmd === 'record_generation' || cmd === 'set_setting' || cmd === 'deactivate_license') return Promise.resolve();
        if (cmd === 'generate_titles') return Promise.resolve([{ title: 'Dev Mode: Sample Title', score: 85, categories: ['book'], breakdown: null }]);
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

// ---- STATE ----
var isPro = true;
var isLoggedIn = true;
var isGuest = false;
var selectedStyle = 'normal';
var selectedGender = 'any';
var dailyUsage = 0;
var activeEngine = 'database';
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
      if (title) title.textContent = view.charAt(0).toUpperCase() + view.slice(1);
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

  // Check license
  invoke('get_settings').then(function (settings) {
    if (settings.license_status === 'valid') {
      activationScreen.style.display = 'none';
      mainApp.style.display = 'flex';
      initApp();
    }
  }).catch(function () {});
});

function openBuyLink() {
  var url = 'https://titleforge-tool.netlify.app/dashboard';
  // Try Tauri shell open — check __TAURI_INTERNALS__ first (earliest available), then __TAURI__
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
  }).catch(function () {});

  // Auto-update check on launch (with small delay so UI renders first)
  setTimeout(setupUpdaterAutoCheck, 800);
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

// ---- TRANSLATE TOGGLE ----
function setupTranslateToggle() {
  var toggle = document.getElementById('translateToggle');
  var langs = document.getElementById('translateLangs');
  if (toggle && langs) {
    toggle.addEventListener('change', function () {
      langs.style.display = toggle.checked ? 'block' : 'none';
    });
  }
}

// ---- SLIDER ----
function setupSlider() {
  var slider = document.getElementById('quantity');
  if (!slider) return;
  slider.max = 100;
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
  usageText.innerHTML = 'Pro — ' + dailyUsage + ' generations today <span style="font-weight:400;">(unlimited)</span>';
}

// ============================================
// GENERATE TITLES
// ============================================

function setupEngineToggle() {
  var dbBtn = document.getElementById('engineDbBtn');
  var aiBtn = document.getElementById('engineAiBtn');
  var status = document.getElementById('engineStatus');
  if (!dbBtn || !aiBtn) return;

  dbBtn.addEventListener('click', function () {
    activeEngine = 'database';
    dbBtn.classList.add('active');
    aiBtn.classList.remove('active');
    if (status) status.textContent = 'Local database — always available';
  });

  aiBtn.addEventListener('click', function () {
    if (!aiProvider || !aiApiKey) {
      if (status) status.textContent = 'No API key saved. Go to Settings.';
      return;
    }
    activeEngine = 'ai';
    aiBtn.classList.add('active');
    dbBtn.classList.remove('active');
    if (status) status.textContent = aiProvider.charAt(0).toUpperCase() + aiProvider.slice(1) + ' — using your key';
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

  var genPromise;

  if (activeEngine === 'ai' && aiProvider && aiApiKey) {
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
  } else {
    dumpDebug('handleGenerate: using DB engine, keyword=' + keyword + ', cats=' + checkedCategories.join(',') + ', qty=' + quantity);
    genPromise = invoke('generate_titles', {
      keyword: keyword,
      categories: checkedCategories,
      style: selectedStyle,
      genre: genre,
      quantity: quantity,
    });
  }

  genPromise.then(function (titles) {
    dumpDebug('generate: SUCCESS, titles count=' + (titles ? titles.length : 'null'));
    displayResults(titles, keyword);
    dailyUsage++;
    invoke('record_generation', {
      keyword: keyword,
      categories: checkedCategories,
      genre: genre,
      style: selectedStyle,
      titles: titles,
    }).catch(function () {});
    updateUsageDisplay();
    saveToHistoryLocal(keyword, checkedCategories, genre, selectedStyle, titles);
    genCountThisSession++;
  }).catch(function (err) {
    var errMsg = typeof err === 'string' ? err : (err.message || 'Something went wrong. Please try again.');
    dumpDebug('generate: FAILED — ' + errMsg + ' (err type: ' + (typeof err) + ', keys: ' + (err && typeof err === 'object' ? Object.keys(err).join(',') : 'N/A') + ')');
    showError(errMsg);
  }).finally(function () {
    document.getElementById('loading').style.display = 'none';
    document.getElementById('generateBtn').disabled = false;
  });
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

  titles.forEach(function (item, idx) {
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
          toggleFavorite(titleText, currentKeyword, item.score || 0, (item.categories || [''])[0], btn);
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
    updateUsageDisplay();
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
    '<div class="stat-card"><span class="stat-number">' + totalTitles + '</span><span class="stat-label">Titles generated</span></div>' +
    '<div class="stat-card"><span class="stat-number">' + dashFavorites.length + '</span><span class="stat-label">Favorites</span></div>' +
    '<div class="stat-card"><span class="stat-number">' + dashProjects.length + '</span><span class="stat-label">Projects</span></div>' +
    '<div class="stat-card"><span class="stat-badge" style="background:var(--forge);">PRO</span><span class="stat-label">Desktop Pro</span></div>';
}

// ---- OVERVIEW TAB ----
function renderOverviewTab() {
  var container = document.getElementById('dashOverviewList');
  if (!container) return;
  var html = '';
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
      var hStar = document.createElement('button');
      var isFav = isFavorited(titleText);
      hStar.className = 'dash-star' + (isFav ? ' starred' : '');
      hStar.innerHTML = isFav ? '\u2605' : '\u2606';
      hStar.addEventListener('click', function () { toggleFavorite(titleText, entry.keyword, (typeof t === 'object' ? t.score : 0) || 0, cats[0] || '', hStar); });
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

function toggleFavorite(titleText, sourceKeyword, score, category, starBtn) {
  invoke('toggle_favorite', {
    title: titleText,
    keyword: sourceKeyword || '',
    score: score || 0,
    category: category || '',
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
    container.innerHTML = '<div class="dash-empty"><div class="dash-empty-icon">\uD83D\uDCC1</div><p class="dash-empty-text">Organize your work.</p><p style="font-size:13px;color:var(--text-secondary);margin-bottom:16px;">Group your best titles into projects for easy access.</p></div>';
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
              invoke('update_title_notes', { projectId: proj.id, title: titleText, notes: textarea.value }).catch(function () {});
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

  // Update version from app_info
  invoke('get_app_info').then(function (info) {
    var verEl = document.getElementById('settingsVersion');
    if (verEl && info.version) verEl.textContent = info.version;
    var updateVerEl = document.getElementById('settingsUpdateVersion');
    if (updateVerEl && info.version) updateVerEl.textContent = 'v' + info.version;
  }).catch(function () {});

  invoke('get_settings').then(function (settings) {
    if (settings.ai_provider) {
      var p = document.getElementById('aiProvider');
      if (p) p.value = settings.ai_provider;
    }
    if (settings.ai_api_key) {
      var ki = document.getElementById('aiApiKey');
      if (ki) ki.placeholder = 'API key saved (enter new key to change)';
    }
  }).catch(function () {});

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

// ---- UPDATER ----
function setupUpdaterAutoCheck() {
  invoke('get_settings').then(function (settings) {
    if (settings.auto_update === 'true') {
      // Silently check for updates — native dialog appears if update found
      checkAndInstallUpdate(true);
    }
  }).catch(function () {});
}

function setupUpdaterControls() {
  // Auto-update toggle (idempotent via _wired flag)
  var autoToggle = document.getElementById('autoUpdateToggle');
  if (autoToggle && !autoToggle._wired) {
    autoToggle._wired = true;
    // Load current setting
    invoke('get_settings').then(function (settings) {
      autoToggle.checked = settings.auto_update === 'true';
    }).catch(function () {});
    // Persist changes immediately
    autoToggle.addEventListener('change', function () {
      invoke('set_setting', { key: 'auto_update', value: autoToggle.checked ? 'true' : 'false' }).catch(function () {});
    });
  }

  // "Check for Updates" button (idempotent)
  var checkBtn = document.getElementById('checkUpdateBtn');
  if (checkBtn && !checkBtn._wired) {
    checkBtn._wired = true;
    checkBtn.addEventListener('click', function () {
      checkAndInstallUpdate(false);
    });
  }
}

function checkAndInstallUpdate(silent) {
  var verEl = document.getElementById('settingsUpdateVersion');
  var statusEl = document.getElementById('settingsUpdateStatus');
  var checkBtn = document.getElementById('checkUpdateBtn');
  var currentVer = (verEl && verEl.textContent) || '0.2.8';

  if (!silent && checkBtn) {
    checkBtn.disabled = true;
    checkBtn.textContent = 'Checking...';
  }
  if (!silent && statusEl) {
    statusEl.textContent = 'Checking for updates...';
    statusEl.style.color = 'var(--text-secondary)';
  }

  invoke('plugin:updater|check').then(function (result) {
    if (result) {
      // Update available
      if (!silent && statusEl) {
        statusEl.textContent = 'Update found: v' + (result.version || '?') + ' — installing...';
        statusEl.style.color = '#16a34a';
      }
      // Trigger download & install — native dialog appears automatically (dialog: true)
      return invoke('plugin:updater|download_and_install', { update: result });
    }
    // No update available
    if (!silent && statusEl) {
      statusEl.textContent = 'You\'re up to date! ' + currentVer + ' is the latest version.';
      statusEl.style.color = '#16a34a';
    }
    return null;
  }).catch(function (err) {
    var msg = typeof err === 'string' ? err : (err.message || 'Network error');
    if (!silent && statusEl) {
      statusEl.textContent = 'Could not check for updates: ' + msg;
      statusEl.style.color = '#b91c1c';
    }
    // Silent mode — don't bother the user on launch
  }).finally(function () {
    if (!silent && checkBtn) {
      checkBtn.disabled = false;
      checkBtn.textContent = 'Check for Updates';
    }
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
    opt.value = cat.label;
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
