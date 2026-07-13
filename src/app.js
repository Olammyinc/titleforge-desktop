/* ============================================
   TitleForge — Application Logic v4
   DEFENSIVE: works even if Supabase doesn't load
   - Guest mode ALWAYS works (3 free, localStorage)
   - Categories ALWAYS render
   - Auth modal shows if Supabase is available
   ============================================ */

// ---- CONFIG ----
const FREE_DAILY_LIMIT = 5;
const GUEST_FREE_LIMIT = 3;
const FREE_MAX_TITLES = 10;
const FREE_CATEGORIES = ['book', 'article', 'blog', 'movie', 'song'];

const ALL_CATEGORIES = [
  { id: 'book',      label: 'Book titles',          free: true  },
  { id: 'article',   label: 'Article titles',        free: true  },
  { id: 'blog',      label: 'Blog post titles',      free: true  },
  { id: 'movie',     label: 'Movie / film titles',   free: true  },
  { id: 'song',      label: 'Song titles',           free: true  },
  { id: 'youtube',   label: 'YouTube video titles',  free: false },
  { id: 'podcast',   label: 'Podcast episode titles', free: false },
  { id: 'newsletter',label: 'Newsletter titles',     free: false },
  { id: 'ebook',     label: 'eBook titles',          free: false },
  { id: 'speech',    label: 'Speech titles',         free: false },
  { id: 'album',     label: 'Music album titles',    free: false },
  { id: 'poem',      label: 'Poem titles',           free: false },
  { id: 'street',    label: 'Street / place names',  free: false },
  { id: 'character', label: 'Character names',       free: false },
  { id: 'product',   label: 'Product names',         free: false },
  { id: 'childname', label: "Children's names",      free: false },
];

// ---- NAME CATEGORIES (use name-scoring rubric, not title rubric) ----
const NAME_CATEGORIES = ['street', 'character', 'product', 'childname'];

// ---- COMMUNICATION STYLES ----
const STYLES = [
  { id: 'normal',      label: 'Normal',       free: true  },
  { id: 'shout',       label: 'Bold / Shout', free: true  },
  { id: 'whisper',     label: 'Subtle / Whisper', free: true  },
  { id: 'blessing',    label: 'Uplifting / Blessing', free: true  },
  { id: 'provocative', label: 'Provocative',  free: false },
  { id: 'minimalist',  label: 'Minimalist',   free: false },
  { id: 'storytelling',label: 'Storytelling', free: false },
  { id: 'question',    label: 'Question',     free: false },
  { id: 'playful',     label: 'Playful',      free: false },
];

// ---- STATE ----
let isPro = false;
let isLoggedIn = false;
let isGuest = true;
let selectedStyle = 'normal';
let dailyUsage = 0;
let sbClient = null; // renamed to avoid collision with CDN global
let currentUser = null;
let authToken = null;
var stripeProLink = '';
var stripePortalLink = '';
var stripeSuccessUrl = '';
var genCountThisSession = 0;

// ---- BREAKDOWN FIELD DEFINITIONS (used for rendering + tooltips) ----
const BREAKDOWN_FIELDS = [
  { key: 'curiosityGap', label: 'Curiosity gap', tip: 'How much the title makes you want to know more. High = strong pull, Low = weak intrigue.' },
  { key: 'emotionalTrigger', label: 'Emotion', tip: 'The emotion the title evokes: curiosity, fear, aspiration, humor, urgency, nostalgia, surprise, or authority.' },
  { key: 'powerWords', label: 'Power words', isArray: true, tip: 'Specific words in the title that carry emotional weight and grab attention.' },
  { key: 'lengthAnalysis', label: 'Length', tip: 'Whether the title length is optimal, short, or long for its intended medium.' },
  { key: 'specificity', label: 'Specificity', tip: 'How concrete (references real things) vs abstract (conceptual) the title is.' },
  { key: 'uniqueness', label: 'Uniqueness', tip: 'How distinctive the name is — from common to rare.' },
  { key: 'memorability', label: 'Memorability', tip: 'How easy the name is to remember and pronounce.' },
  { key: 'meaningDepth', label: 'Meaning depth', tip: 'The depth of meaning or cultural significance behind the name.' },
  { key: 'pronunciationEase', label: 'Pronunciation', tip: 'How easy the name is to say aloud — from hard to easy.' },
  { key: 'originVibe', label: 'Origin / vibe', tip: 'The cultural origin, era, or overall feel of the name.' },
];
var activeTooltip = null;

// ---- HELPERS ----
function escapeHtml(str) {
  if (!str) return '';
  return String(str)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}

// Create a tooltip ? button that shows explanation on click
function createTipBtn(tipText) {
  var btn = document.createElement('span');
  btn.className = 'tip-btn';
  btn.textContent = '?';
  btn.setAttribute('role', 'button');
  btn.setAttribute('tabindex', '0');
  btn.addEventListener('click', function (e) {
    e.stopPropagation();
    var existing = document.querySelector('.tip-popup');
    if (existing) {
      if (existing._source === btn) { existing.remove(); return; }
      existing.remove();
    }
    var popup = document.createElement('div');
    popup.className = 'tip-popup';
    popup.textContent = tipText;
    popup._source = btn;
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

// ---- INIT ----
document.addEventListener('DOMContentLoaded', function () {

  // STEP 1: Always render categories immediately — never wait for anything
  renderCategories();
  setupStyleButtons();
  setupGenderButtons();
  setupFineTune();
  setupTranslateToggle();
  setupSlider();
  setupGenerateButton();
  setupUpgradeButton();
  setupBillingToggle();
  setupWaitlistModal();
  setupExitIntent();
  setupDashboardTabs();
  setupDashboardSearch();
  setupExportButtons();
  setupProjects();
  populateDashFilters();
  updateUsageDisplay();
  setupAvatarDropdown();
  setupFloatingGenerator();
  updateStickyCta();
  hideLoading();

  // STEP 2: Wire up nav buttons + auth UI (always work, don't wait for Supabase)
  wireNavButtons();
  setupAuthListeners();

  // STEP 3: Load guest usage from localStorage (always works)
  loadGuestUsage();
  updateUsageDisplay();

  // STEP 4: Try to init Supabase — but this is non-blocking
  tryInitSupabase();

  // STEP 5: Fetch Stripe links from config
  fetch('/.netlify/functions/config')
    .then(function (r) { return r.json(); })
    .then(function (cfg) {
      if (cfg.stripeProLink) stripeProLink = cfg.stripeProLink;
      if (cfg.stripePortalLink) stripePortalLink = cfg.stripePortalLink;
      if (cfg.stripeSuccessUrl) stripeSuccessUrl = cfg.stripeSuccessUrl;
    })
    .catch(function () {});
});

// ============================================
// CORE FUNCTIONS (ALWAYS WORK)
// ============================================

function hideLoading() {
  // Nothing to hide — we removed the loading state from the HTML
}

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
    if (!cat.free && !isPro) checkbox.disabled = true;

    var label = document.createElement('label');
    label.htmlFor = 'cat-' + cat.id;
    label.style.cursor = (cat.free || isPro) ? 'pointer' : 'not-allowed';
    label.innerHTML = cat.label + (cat.free ? '' : ' <span class="pro-badge">PRO</span>');

    div.appendChild(checkbox);
    div.appendChild(label);

    div.addEventListener('click', function (e) {
      if (e.target !== checkbox) {
        if (!checkbox.disabled) checkbox.checked = !checkbox.checked;
      }
      if (checkbox.checked) div.classList.add('checked');
      else div.classList.remove('checked');
      // Show/hide gender selector when any people-name category is checked
      if (cat.id === 'childname' || cat.id === 'character') {
        var anyGendered = document.querySelector('#cat-childname:checked, #cat-character:checked');
        var genderGroup = document.getElementById('genderGroup');
        if (genderGroup) genderGroup.style.display = anyGendered ? 'block' : 'none';
      }
    });

    grid.appendChild(div);
  });
}

function setupStyleButtons() {
  var buttons = document.querySelectorAll('.style-btn');
  buttons.forEach(function (btn) {
    // Clone and replace to remove old event listeners (makes this re-runnable)
    var newBtn = btn.cloneNode(true);
    btn.parentNode.replaceChild(newBtn, btn);
    var styleId = newBtn.getAttribute('data-style');
    var styleDef = STYLES.find(function (s) { return s.id === styleId; });

    if (styleDef && !styleDef.free && !isPro) {
      newBtn.classList.add('pro-locked');
      newBtn.addEventListener('click', function (e) {
        e.preventDefault();
        showUpgradeModal();
      });
      newBtn.querySelector('.pro-badge') || (newBtn.innerHTML += ' <span class="pro-badge">PRO</span>');
      return;
    }

    newBtn.classList.remove('pro-locked');
    newBtn.addEventListener('click', function () {
      var siblings = newBtn.parentNode.querySelectorAll('.style-btn');
      siblings.forEach(function (b) { b.classList.remove('active'); });
      newBtn.classList.add('active');
      selectedStyle = styleId;
    });
  });
}

function setupTranslateToggle() {
  var toggle = document.getElementById('translateToggle');
  var langs = document.getElementById('translateLangs');
  if (toggle) {
    toggle.addEventListener('change', function () {
      if (langs) langs.style.display = toggle.checked ? 'block' : 'none';
    });
  }
}

// ============================================
// FINE-TUNE EXPANDER (7 optional questions)
// ============================================

function setupFineTune() {
  var toggle = document.getElementById('finetuneToggle');
  var panel = document.getElementById('finetunePanel');
  if (!toggle || !panel) return;

  toggle.addEventListener('click', function () {
    var isOpen = panel.style.display !== 'none';
    panel.style.display = isOpen ? 'none' : 'block';
    toggle.setAttribute('aria-expanded', String(!isOpen));
    var arrow = toggle.querySelector('.finetune-arrow');
    if (arrow) arrow.textContent = isOpen ? '▸' : '▾';
  });
}

// Read all 7 fine-tune fields, return object (empty values omitted)
function collectFineTune() {
  var ft = {};
  var map = {
    audience: 'ftAudience',
    emotion: 'ftEmotion',
    length: 'ftLength',
    angle: 'ftAngle',
    mustInclude: 'ftMustInclude',
    avoid: 'ftAvoid',
    beatTitle: 'ftBeat'
  };
  Object.keys(map).forEach(function (key) {
    var el = document.getElementById(map[key]);
    if (el && el.value && el.value.trim()) ft[key] = el.value.trim();
  });
  return Object.keys(ft).length ? ft : null;
}

// Read selected gender (only meaningful when a name category is picked)
var selectedGender = 'any';
function setupGenderButtons() {
  var buttons = document.querySelectorAll('.gender-btn');
  buttons.forEach(function (btn) {
    btn.addEventListener('click', function () {
      buttons.forEach(function (b) { b.classList.remove('active'); });
      btn.classList.add('active');
      selectedGender = btn.getAttribute('data-gender') || 'any';
    });
  });
}

function setupSlider() {
  var slider = document.getElementById('quantity');
  if (!slider) return;
  updateSliderTrack(slider);
  if (!isPro) slider.max = FREE_MAX_TITLES;
  else slider.max = 100;
  updateQuantityLabel();

  slider.addEventListener('input', function () {
    var val = parseInt(slider.value);
    if (!isPro && val > FREE_MAX_TITLES) slider.value = FREE_MAX_TITLES;
    updateSliderTrack(slider);
    updateQuantityLabel();
  });
}

function updateSliderTrack(slider) {
  var val = parseInt(slider.value);
  var max = parseInt(slider.max);
  var pct = (val / max) * 100;
  slider.style.background =
    'linear-gradient(to right, #2563eb 0%, #2563eb ' + pct + '%, #bfdbfe ' + pct + '%, #bfdbfe 100%)';
}

function updateQuantityLabel() {
  var slider = document.getElementById('quantity');
  var display = document.getElementById('qtyDisplay');
  if (!slider || !display) return;
  var val = parseInt(slider.value);
  if (!isPro) display.textContent = val;
  else display.textContent = val;
}

function setupGenerateButton() {
  var btn = document.getElementById('generateBtn');
  if (!btn) return;
  btn.addEventListener('click', handleGenerate);
}

function setupUpgradeButton() {
  var btn = document.getElementById('upgradeBtn');
  if (!btn) return;
  btn.addEventListener('click', function (e) {
    if (!btn.href || btn.href === '#' || btn.href.endsWith('#')) {
      e.preventDefault();
      showUpgradeModal();
    }
  });
}

// Billing toggle (monthly / yearly)
function setupBillingToggle() {
  var toggle = document.getElementById('billingToggle');
  if (!toggle) return;

  toggle.addEventListener('change', function () {
    var yearly = toggle.checked;
    var priceEl = document.getElementById('webProPrice');
    var periodEl = document.getElementById('webProPeriod');
    var ctaEl = document.getElementById('webProCta');
    var monthlyLabel = document.getElementById('billingLabelMonthly');
    var yearlyLabel = document.getElementById('billingLabelYearly');

    if (yearly) {
      priceEl.textContent = '$15.83';
      periodEl.textContent = '/mo';
      ctaEl.textContent = '$15.83/mo';
      var billingNote = document.getElementById('billingNote');
      if (billingNote) billingNote.style.display = 'block';
      if (monthlyLabel) monthlyLabel.classList.remove('active');
      if (yearlyLabel) yearlyLabel.classList.add('active');
    } else {
      priceEl.textContent = '$19';
      periodEl.textContent = '/month';
      ctaEl.textContent = '$19/mo';
      var billingNote = document.getElementById('billingNote');
      if (billingNote) billingNote.style.display = 'none';
      if (monthlyLabel) monthlyLabel.classList.add('active');
      if (yearlyLabel) yearlyLabel.classList.remove('active');
    }
  });

  // Default to yearly
  toggle.checked = true;
  var evt = document.createEvent('HTMLEvents');
  evt.initEvent('change', false, true);
  toggle.dispatchEvent(evt);
}

function showPostGenUpgrade() {
  var results = document.getElementById('results');
  if (!results) return;
  var banner = document.createElement('div');
  banner.className = 'post-gen-upgrade';
  banner.innerHTML = '<p><strong>Liking what you see?</strong> Pro unlocks all 16 categories, 100 titles per batch, and full appeal breakdowns. <a href="#pricing" style="color:var(--forge);font-weight:600;">Upgrade from $15.83/mo</a></p>';
  results.parentNode.insertBefore(banner, results);
}

function setupWaitlistModal() {
  var modal = document.getElementById('waitlistModal');
  var closeBtn = document.getElementById('waitlistModalClose');
  var submitBtn = document.getElementById('waitlistSubmitBtn');
  var emailInput = document.getElementById('waitlistEmail');
  var msgEl = document.getElementById('waitlistMsg');

  document.querySelectorAll('.waitlist-btn').forEach(function (btn) {
    btn.addEventListener('click', function () { if (modal) modal.classList.add('active'); });
  });
  var desktopBtn = document.getElementById('desktopWaitlistBtn');
  if (desktopBtn) {
    desktopBtn.addEventListener('click', function () { if (modal) modal.classList.add('active'); });
  }
  if (closeBtn) closeBtn.addEventListener('click', function () { if (modal) modal.classList.remove('active'); });
  if (modal) modal.addEventListener('click', function (e) { if (e.target === modal) modal.classList.remove('active'); });

  if (submitBtn) {
    submitBtn.addEventListener('click', function () {
      var email = emailInput.value.trim();
      if (!email || email.indexOf('@') === -1) {
        msgEl.textContent = 'Please enter a valid email.';
        msgEl.style.color = '#b91c1c';
        msgEl.style.display = 'block';
        return;
      }
      submitBtn.textContent = 'Submitting...';
      submitBtn.disabled = true;
      fetch('/.netlify/functions/waitlist', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ email: email })
      }).then(function () {
        msgEl.textContent = 'You\'re on the list! We\'ll email you when it launches.';
        msgEl.style.color = 'var(--success)';
        msgEl.style.display = 'block';
        submitBtn.style.display = 'none';
        emailInput.style.display = 'none';
      }).catch(function () {
        msgEl.textContent = 'Something went wrong. Try again.';
        msgEl.style.color = '#b91c1c';
        msgEl.style.display = 'block';
        submitBtn.textContent = 'Join Waitlist →';
        submitBtn.disabled = false;
      });
    });
  }
}

var exitIntentShown = false;
function setupExitIntent() {
  if (isLoggedIn || isPro) return;
  document.addEventListener('mouseout', function (e) {
    if (exitIntentShown) return;
    if (e.clientY <= 0 && e.relatedTarget === null) {
      exitIntentShown = true;
      var modal = document.getElementById('exitModal');
      if (modal) setTimeout(function () { modal.classList.add('active'); }, 300);
    }
  });
  var closeBtn = document.getElementById('exitModalClose');
  var noThanks = document.getElementById('exitModalNoThanks');
  var modal = document.getElementById('exitModal');
  if (closeBtn) closeBtn.addEventListener('click', function () { if (modal) modal.classList.remove('active'); });
  if (noThanks) noThanks.addEventListener('click', function () { if (modal) modal.classList.remove('active'); });
  if (modal) modal.addEventListener('click', function (e) { if (e.target === modal) modal.classList.remove('active'); });
}

function wireNavButtons() {
  var signInBtn = document.getElementById('navSignIn');
  var signUpBtn = document.getElementById('navSignUp');
  var logoutBtn = document.getElementById('navLogout');
  var logoutBtn2 = document.getElementById('logoutBtn');
  var guestLink = document.getElementById('guestSignUpLink');

  if (signInBtn) signInBtn.addEventListener('click', function () { openAuthModal('signin'); });
  if (signUpBtn) signUpBtn.addEventListener('click', function () { openAuthModal('signup'); });
  if (logoutBtn) logoutBtn.addEventListener('click', handleLogout);
  if (logoutBtn2) logoutBtn2.addEventListener('click', handleLogout);
  if (guestLink) guestLink.addEventListener('click', function (e) { e.preventDefault(); openAuthModal('signup'); });
}

// ============================================
// GUEST USAGE (localStorage — always works)
// ============================================

function loadGuestUsage() {
  var today = new Date().toDateString();
  try {
    var stored = localStorage.getItem('titleforge_guest_usage');
    if (stored) {
      var data = JSON.parse(stored);
      if (data.date === today) { dailyUsage = data.count; }
      else { dailyUsage = 0; }
    } else { dailyUsage = 0; }
  } catch (e) { dailyUsage = 0; }
  saveGuestUsage();
}

function saveGuestUsage() {
  try {
    var today = new Date().toDateString();
    localStorage.setItem('titleforge_guest_usage', JSON.stringify({ date: today, count: dailyUsage }));
  } catch (e) { /* localStorage disabled */ }
}

function getUsageLimit() {
  if (isPro) return Infinity;
  if (isLoggedIn) return FREE_DAILY_LIMIT;
  return GUEST_FREE_LIMIT;
}

function canGenerate() {
  if (isPro) return true;
  return dailyUsage < getUsageLimit();
}

function updateUsageDisplay() {
  var usageText = document.getElementById('usageText');
  var usageBar = document.getElementById('usageBar');
  if (!usageBar || !usageText) return;

  if (isPro) {
    usageBar.style.display = 'block';
    usageBar.style.background = '#e8f5e9';
    usageBar.style.borderColor = '#c8e6c9';
    usageBar.style.color = '#2e7d32';
    usageText.innerHTML = 'Pro — ' + dailyUsage + ' generations used today <span style="font-weight:400;">(unlimited)</span>';
    return;
  }

  var limit = getUsageLimit();
  var remaining = limit - dailyUsage;

  usageBar.classList.remove('limit');

  if (limit === Infinity || remaining > 0) {
    if (isGuest) {
      usageText.innerHTML = 'Guest mode: ' + dailyUsage + ' / ' + limit + ' <span style="font-weight:400;">(' + remaining + ' remaining — <a href="#" onclick="openAuthModal(\'signup\');return false;" style="color:#2563eb;font-weight:600;">create account</a> for 5/day)</span>';
    } else {
      usageText.innerHTML = 'Free generations today: ' + dailyUsage + ' / ' + limit + ' <span style="font-weight:400;">(' + remaining + ' remaining — up to ' + FREE_MAX_TITLES + ' titles each)</span>';
    }
  } else {
    usageBar.classList.add('limit');
    if (isGuest) {
      usageText.innerHTML = 'Guest limit reached — you\'ve used all ' + GUEST_FREE_LIMIT + ' free generations. <a href="#" onclick="openAuthModal(\'signup\');return false;" style="color:#e65100;font-weight:600;">Create a free account</a> for ' + FREE_DAILY_LIMIT + '/day.';
    } else {
      usageText.innerHTML = 'Free limit reached for today — you\'ve used all ' + FREE_DAILY_LIMIT + ' generations. <a href="#pricing" style="color:#e65100;font-weight:600;">Upgrade to Pro</a> for unlimited.';
    }
  }
}

// ============================================
// GENERATE TITLES
// ============================================

function handleGenerate() {
  var keyword = document.getElementById('keyword').value.trim();
  if (!keyword) { showError('Please enter a keyword or existing title.'); return; }

  if (!canGenerate()) {
    if (isGuest) { openAuthModal('signup'); }
    else { showUpgradeModal(); }
    return;
  }

  var checkedCategories = [];
  var checkboxes = document.querySelectorAll('#categoryGrid input:checked');
  checkboxes.forEach(function (cb) { checkedCategories.push(cb.value); });
  if (checkedCategories.length === 0) { showError('Please select at least one category.'); return; }

  var wantCrossMedium = document.getElementById('crossMediumToggle').checked;
  var quantity = parseInt(document.getElementById('quantity').value);
  var genre = document.getElementById('genre').value;
  var wantSubtitles = document.getElementById('subtitlesToggle').checked;
  var wantTranslation = document.getElementById('translateToggle').checked;
  var translateLang = wantTranslation ? document.getElementById('translateLang').value : null;

  // Gender for name categories — only send for people-name categories
  var GENDERED_CATS = ['childname', 'character'];
  var hasGenderedCat = checkedCategories.some(function (c) { return GENDERED_CATS.indexOf(c) !== -1; });
  var gender = hasGenderedCat ? selectedGender : null;

  // Fine-tune answers (optional) — null if all blank
  var finetune = collectFineTune();

  document.getElementById('loading').style.display = 'block';
  document.getElementById('results').innerHTML = '';
  document.getElementById('error').style.display = 'none';
  document.getElementById('generateBtn').disabled = true;

  var headers = { 'Content-Type': 'application/json' };
  if (authToken) headers['Authorization'] = 'Bearer ' + authToken;

  fetch('/.netlify/functions/generate', {
    method: 'POST',
    headers: headers,
    body: JSON.stringify({
      keyword: keyword,
      categories: checkedCategories,
      genre: genre,
      style: selectedStyle,
      gender: gender,
      quantity: quantity,
      crossMedium: wantCrossMedium,
      includeSubtitles: wantSubtitles,
      includeTranslation: wantTranslation,
      translateLang: translateLang,
      finetune: finetune
    })
  }).then(function (response) {
    if (!response.ok) {
      return response.json().catch(function () { return {}; }).then(function (err) {
        throw new Error(err.error || 'Server error (' + response.status + ')');
      });
    }
    return response.json();
  }).then(function (data) {
    if (data.crossMedium) {
      displayCrossMediumResults(data.crossMedium);
    } else {
      displayResults(data.titles, keyword);
    }
    dailyUsage++;
    saveGuestUsage();
    updateUsageDisplay();

    // Server-side usage increment for logged-in users
    if (isLoggedIn && authToken) {
      fetch('/.netlify/functions/usage', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'Authorization': 'Bearer ' + authToken },
        body: JSON.stringify({ action: 'increment' })
      }).catch(function () {});
    }

    // Track free gen for upgrade prompt
    genCountThisSession++;
    if (!isPro && genCountThisSession >= 3) {
      showPostGenUpgrade();
    }

    if (isLoggedIn && authToken) {
      saveToHistory(keyword, checkedCategories, genre, selectedStyle, data.titles || []);
    }
  }).catch(function (err) {
    showError(err.message || 'Something went wrong. Please try again.');
  }).finally(function () {
    document.getElementById('loading').style.display = 'none';
    document.getElementById('generateBtn').disabled = false;
  });
}

// ============================================
// DISPLAY RESULTS
// ============================================

function displayResults(titles, currentKeyword) {
  var container = document.getElementById('results');
  container.innerHTML = '';

  if (!titles || titles.length === 0) {
    container.innerHTML = '<p style="text-align:center;color:#6b6b8b;padding:20px;">No titles generated. Try a different keyword or category.</p>';
    return;
  }

  titles.forEach(function (item, idx) {
    var div = document.createElement('div');
    div.className = 'result-item';

    // === LEFT: Score + bar ===
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

    // === CENTER: Title + tags + subtitle/translation ===
    var body = document.createElement('div');
    body.className = 'result-body';

    var titleEl = document.createElement('div');
    titleEl.className = 'result-title';
    titleEl.textContent = item.title;
    body.appendChild(titleEl);

    // Category tags
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

    // Star/Favorite + Project buttons (inline after tags, before subtitle)
    if (authToken && item.title) {
      var starBtn = document.createElement('button');
      starBtn.className = 'result-star' + (isFavorited(item.title) ? ' starred' : '');
      starBtn.title = 'Save to favorites';
      starBtn.innerHTML = isFavorited(item.title) ? '★' : '☆';
      (function (titleText, btn) {
        btn.addEventListener('click', function () {
          toggleFavorite(titleText, btn, currentKeyword);
          btn.innerHTML = isFavorited(titleText) ? '★' : '☆';
        });
      })(item.title, starBtn);
      body.appendChild(starBtn);

      var projBtn = document.createElement('button');
      projBtn.className = 'proj-add-btn';
      projBtn.title = isPro ? 'Add to project' : 'Upgrade to use projects';
      if (!isPro) projBtn.classList.add('proj-locked');
      projBtn.textContent = '📁';
      projBtn.addEventListener('click', function (e) {
        e.stopPropagation();
        if (!isPro) { showUpgradeModal(); return; }
        var existing = document.querySelector('.proj-dropdown.active');
        if (existing) {
          if (existing._title === item.title) { existing.remove(); return; }
          existing.remove();
        }
        showProjectPicker(item.title, currentKeyword, item.score || null, projBtn);
      });
      body.appendChild(projBtn);
    }

    // Subtitle toggle (Pro) or teaser (free)
    if (item.subtitle) {
      if (isPro) {
        var subToggle = document.createElement('button');
        subToggle.className = 'result-ext-toggle';
        subToggle.setAttribute('type', 'button');
        subToggle.textContent = '▸ Subtitle';
        var subContent = document.createElement('div');
        subContent.className = 'result-ext-content';
        subContent.style.display = 'none';
        subContent.textContent = item.subtitle;
        subToggle.addEventListener('click', function () {
          var open = subContent.style.display === 'block';
          subContent.style.display = open ? 'none' : 'block';
          subToggle.textContent = open ? '▸ Subtitle' : '▾ Subtitle';
        });
        body.appendChild(subToggle);
        body.appendChild(subContent);
      } else {
        var subTeaser = document.createElement('span');
        subTeaser.className = 'result-pro-teaser';
        subTeaser.innerHTML = '<span class="pro-badge">PRO</span> Subtitle';
        subTeaser.addEventListener('click', function (e) { e.preventDefault(); showUpgradeModal(); });
        body.appendChild(subTeaser);
      }
    }

    // Translation toggle (Pro) or teaser (free)
    if (item.translation) {
      if (isPro) {
        var transToggle = document.createElement('button');
        transToggle.className = 'result-ext-toggle';
        transToggle.setAttribute('type', 'button');
        transToggle.textContent = '▸ ' + (item.translationLang || 'Translation');
        var transContent = document.createElement('div');
        transContent.className = 'result-ext-content';
        transContent.style.display = 'none';
        transContent.textContent = item.translation;
        transToggle.addEventListener('click', function () {
          var open = transContent.style.display === 'block';
          transContent.style.display = open ? 'none' : 'block';
          transToggle.textContent = open ? '▸ ' + (item.translationLang || 'Translation') : '▾ ' + (item.translationLang || 'Translation');
        });
        body.appendChild(transToggle);
        body.appendChild(transContent);
      } else {
        var transTeaser = document.createElement('span');
        transTeaser.className = 'result-pro-teaser';
        transTeaser.innerHTML = '<span class="pro-badge">PRO</span> Translation';
        transTeaser.addEventListener('click', function (e) { e.preventDefault(); showUpgradeModal(); });
        body.appendChild(transTeaser);
      }
    }

    div.appendChild(body);

    // === RIGHT: Breakdown panel (always visible) ===
    if (item.breakdown) {
      var bdCol = document.createElement('div');
      bdCol.className = 'result-breakdown';

      var bd = item.breakdown;

      // Determine if this is a name rubric or title rubric
      var isNameBreakdown = bd.uniqueness || bd.memorability || bd.meaningDepth || bd.pronunciationEase || bd.originVibe;
      var visibleFields = BREAKDOWN_FIELDS.filter(function (f) {
        if (isNameBreakdown) {
          return ['uniqueness', 'memorability', 'meaningDepth', 'pronunciationEase', 'originVibe'].indexOf(f.key) !== -1;
        }
        return ['curiosityGap', 'emotionalTrigger', 'powerWords', 'lengthAnalysis', 'specificity'].indexOf(f.key) !== -1;
      });

      visibleFields.forEach(function (field) {
        var val = bd[field.key];
        if (!isPro) {
          var row = document.createElement('div');
          row.className = 'bd-row';
          row.style.cursor = 'pointer';
          var labelEl = document.createElement('span');
          labelEl.className = 'bd-label';
          labelEl.textContent = field.label;
          labelEl.appendChild(createTipBtn(field.tip));
          row.appendChild(labelEl);
          var valEl = document.createElement('span');
          valEl.className = 'bd-value';
          valEl.innerHTML = '<span class="pro-badge">PRO</span>';
          row.appendChild(valEl);
          row.addEventListener('click', function (e) { if (!e.target.closest('.tip-btn')) { e.preventDefault(); showUpgradeModal(); } });
          bdCol.appendChild(row);
          return;
        }
        if (!val || (Array.isArray(val) && val.length === 0)) return;
        var row = document.createElement('div');
        row.className = 'bd-row';
        var labelEl = document.createElement('span');
        labelEl.className = 'bd-label';
        labelEl.textContent = field.label;
        labelEl.appendChild(createTipBtn(field.tip));
        row.appendChild(labelEl);
        var valEl = document.createElement('span');
        valEl.className = 'bd-value';
        if (field.isArray) {
          val.forEach(function (w) {
            var wSpan = document.createElement('span');
            wSpan.className = 'bd-power-word';
            wSpan.textContent = w;
            valEl.appendChild(wSpan);
          });
        } else {
          valEl.textContent = val;
        }
        row.appendChild(valEl);
        bdCol.appendChild(row);
      });

      div.appendChild(bdCol);
    }

    container.appendChild(div);
  });
}

// ============================================
// CROSS-MEDIUM DISPLAY
// ============================================

function displayCrossMediumResults(crossMediumData) {
  var container = document.getElementById('results');
  container.innerHTML = '';

  var keys = Object.keys(crossMediumData);
  if (keys.length === 0) {
    container.innerHTML = '<p style="text-align:center;color:#6b6b8b;padding:20px;">No titles generated. Try a different keyword or category.</p>';
    return;
  }

  // Tab bar
  var tabBar = document.createElement('div');
  tabBar.className = 'cm-tab-bar';

  // Tab content panels
  var tabContent = document.createElement('div');
  tabContent.className = 'cm-tab-content';

  keys.forEach(function (mediumKey, tabIdx) {
    var tab = document.createElement('button');
    tab.className = 'cm-tab' + (tabIdx === 0 ? ' active' : '');
    tab.textContent = mediumKey;
    tab.setAttribute('type', 'button');

    var panel = document.createElement('div');
    panel.className = 'cm-tab-panel' + (tabIdx === 0 ? ' active' : '');

    var titles = crossMediumData[mediumKey] || [];
    titles.forEach(function (item) {
      var card = document.createElement('div');
      card.className = 'cm-full-card';

      // Score column
      if (item.score !== undefined && item.score !== null) {
        var scoreCol = document.createElement('div');
        scoreCol.className = 'cm-score';

        var scoreNum = document.createElement('div');
        scoreNum.className = 'cm-score-num';
        scoreNum.textContent = item.score;

        var bar = document.createElement('div');
        bar.className = 'cm-score-bar';
        var fill = document.createElement('div');
        fill.className = 'cm-score-fill';
        var color = '#c62828';
        if (item.score >= 75) color = '#4caf50';
        else if (item.score >= 50) color = '#e8a040';
        else if (item.score >= 25) color = '#ff9800';
        fill.style.background = color;
        fill.style.width = item.score + '%';
        bar.appendChild(fill);

        scoreCol.appendChild(scoreNum);
        scoreCol.appendChild(bar);
        card.appendChild(scoreCol);
      }

      // Title body
      var bodyEl = document.createElement('div');
      bodyEl.className = 'cm-body';
      bodyEl.textContent = item.title;
      card.appendChild(bodyEl);

      // Breakdown with Pro gating
      if (item.breakdown) {
        var bdMini = document.createElement('div');
        bdMini.className = 'cm-breakdown';
        var bd = item.breakdown;
        var isCmName = bd.uniqueness || bd.memorability;
        var cmFields = isCmName ? [
          { key: 'uniqueness', label: 'Unique', tip: 'How distinctive the name is.' },
          { key: 'memorability', label: 'Memory', tip: 'How easy to remember and pronounce.' },
          { key: 'meaningDepth', label: 'Meaning', tip: 'Depth of meaning or cultural significance.' },
          { key: 'pronunciationEase', label: 'Speak', tip: 'How easy to say aloud.' },
          { key: 'originVibe', label: 'Origin', tip: 'Cultural origin or overall feel.' },
        ] : [
          { key: 'curiosityGap', label: 'Curiosity', tip: 'How much the title makes you want to know more.' },
          { key: 'emotionalTrigger', label: 'Emotion', tip: 'The emotion the title evokes.' },
          { key: 'powerWords', label: 'Words', isArray: true, tip: 'Words that carry emotional weight.' },
          { key: 'specificity', label: 'Spec.', tip: 'How concrete vs abstract the title is.' },
        ];
        cmFields.forEach(function (f) {
          var v = bd[f.key];
          if (!isPro) {
            var row = document.createElement('div');
            row.className = 'cm-bd-row';
            row.style.cursor = 'pointer';
            var labelSpan = document.createElement('span');
            labelSpan.className = 'cm-bd-label';
            labelSpan.textContent = f.label;
            labelSpan.appendChild(createTipBtn(f.tip));
            row.appendChild(labelSpan);
            var valSpan = document.createElement('span');
            valSpan.className = 'cm-bd-val';
            valSpan.innerHTML = '<span class="pro-badge">PRO</span>';
            row.appendChild(valSpan);
            row.addEventListener('click', function (e) { if (!e.target.closest('.tip-btn')) { e.preventDefault(); showUpgradeModal(); } });
            bdMini.appendChild(row);
            return;
          }
          if (!v || (Array.isArray(v) && v.length === 0)) return;
          var row = document.createElement('div');
          row.className = 'cm-bd-row';
          var labelSpan = document.createElement('span');
          labelSpan.className = 'cm-bd-label';
          labelSpan.textContent = f.label;
          labelSpan.appendChild(createTipBtn(f.tip));
          row.appendChild(labelSpan);
          var valSpan = document.createElement('span');
          valSpan.className = 'cm-bd-val';
          valSpan.textContent = Array.isArray(v) ? v.join(', ') : v;
          row.appendChild(valSpan);
          bdMini.appendChild(row);
        });
        card.appendChild(bdMini);
      }

      panel.appendChild(card);
    });

    tab.addEventListener('click', function () {
      var tabs = tabBar.querySelectorAll('.cm-tab');
      tabs.forEach(function (t) { t.classList.remove('active'); });
      var panels = tabContent.querySelectorAll('.cm-tab-panel');
      panels.forEach(function (p) { p.classList.remove('active'); });
      tab.classList.add('active');
      panel.classList.add('active');
    });

    tabBar.appendChild(tab);
    tabContent.appendChild(panel);
  });

  var wrapper = document.createElement('div');
  wrapper.className = 'cm-tabbed';
  wrapper.appendChild(tabBar);
  wrapper.appendChild(tabContent);
  container.appendChild(wrapper);
}

// ============================================
// ERRORS & MODALS
// ============================================

function showError(msg) {
  var el = document.getElementById('error');
  if (!el) return;
  el.textContent = msg;
  el.style.display = 'block';
}

function showUpgradeModal() {
  // Already handled in HTML
  var existing = document.getElementById('upgradeModal');
  if (existing) { existing.classList.add('active'); return; }

  var modal = document.createElement('div');
  modal.id = 'upgradeModal';
  modal.className = 'modal-overlay active';
  var upgradeBtnHtml = stripeProLink
    ? '<a href="' + stripeProLink + '" target="_blank" class="btn btn-primary btn-full">Upgrade Now — $19/mo</a>'
    : '<a href="#" id="modalUpgradeLink" class="btn btn-primary btn-full">Upgrade Now — $19/mo</a>';
  modal.innerHTML = '<div class="modal"><h3>Upgrade to Pro</h3><p>Pro unlocks all 16 categories, up to 100 titles, AI-estimated appeal scores, subtitle generation, and translation into 12 languages — all for $19/month.</p><p style="font-size:13px;margin-bottom:16px;">Cancel anytime. No contracts.</p>' + upgradeBtnHtml + '<div class="modal-close" onclick="document.getElementById(\'upgradeModal\').classList.remove(\'active\')">Maybe later</div></div>';
  document.body.appendChild(modal);
}

// ============================================
// AUTH
// ============================================

function openAuthModal(tab) {
  var modal = document.getElementById('authModal');
  if (!modal) { alert('Auth modal not found on page.'); return; }

  modal.classList.add('active');

  var signUpForm = document.getElementById('signUpForm');
  var loginForm = document.getElementById('loginForm');
  var tabSignUp = document.getElementById('tabSignUp');
  var tabSignIn = document.getElementById('tabSignIn');

  if (tab === 'signup') {
    if (signUpForm) signUpForm.style.display = 'block';
    if (loginForm) loginForm.style.display = 'none';
    if (tabSignUp) tabSignUp.classList.add('active');
    if (tabSignIn) tabSignIn.classList.remove('active');
  } else {
    if (signUpForm) signUpForm.style.display = 'none';
    if (loginForm) loginForm.style.display = 'block';
    if (tabSignUp) tabSignUp.classList.remove('active');
    if (tabSignIn) tabSignIn.classList.add('active');
  }

  var authErr = document.getElementById('authError');
  if (authErr) authErr.style.display = 'none';
}

function tryInitSupabase() {
  // Restore auth from localStorage if available (for when Supabase CDN is blocked)
  try {
    var stored = localStorage.getItem('titleforge_auth');
    if (stored) {
      var authData = JSON.parse(stored);
      if (authData.isLoggedIn && authData.token) {
        authToken = authData.token;
        isLoggedIn = true;
        isGuest = false;
        isPro = localStorage.getItem('titleforge_pro') === 'true';
        onAuthRestoredFromStorage(authData);
        applyProUI();
      }
    }
  } catch (e) {}

  // Only try if the supabase object is available globally
  if (typeof window.supabase === 'undefined' || !window.supabase.createClient) {
    setTimeout(function () {
      if (typeof window.supabase !== 'undefined' && window.supabase.createClient) {
        initSupabaseClient();
      } else {
        console.log('Supabase library not loaded — guest mode only');
      }
    }, 1500);
    return;
  }
  initSupabaseClient();
}

function onAuthRestoredFromStorage(authData) {
  updateNavUserUI(authData.email);
  var navDashboard = document.getElementById('navDashboard');
  if (navDashboard) navDashboard.style.display = 'inline';
}

function initSupabaseClient() {
  fetch('/.netlify/functions/config')
    .then(function (res) { return res.json(); })
    .then(function (config) {
      if (config.stripeProLink) stripeProLink = config.stripeProLink;
      if (config.stripePortalLink) stripePortalLink = config.stripePortalLink;
      if (!config.supabaseUrl || !config.supabaseAnonKey) {
        console.log('Supabase config not set — guest mode only');
        return;
      }
      sbClient = window.supabase.createClient(config.supabaseUrl, config.supabaseAnonKey);
      // Auth listeners already wired at DOMContentLoaded — no need to call again
      return sbClient.auth.getSession();
    })
    .then(function (result) {
      if (result && result.data && result.data.session) {
        onAuthSuccess(result.data.session);
      }
    })
    .catch(function (err) {
      console.error('Supabase init error (guest mode continues):', err);
    });
}

function setupAuthListeners() {
  var closeBtn = document.getElementById('authModalClose');
  var modal = document.getElementById('authModal');

  if (closeBtn) closeBtn.addEventListener('click', function () { modal.classList.remove('active'); });
  if (modal) {
    modal.addEventListener('click', function (e) {
      if (e.target === modal) modal.classList.remove('active');
    });
  }

  var tabSignUp = document.getElementById('tabSignUp');
  var tabSignIn = document.getElementById('tabSignIn');
  if (tabSignUp) tabSignUp.addEventListener('click', function () { openAuthModal('signup'); });
  if (tabSignIn) tabSignIn.addEventListener('click', function () { openAuthModal('signin'); });

  var signUpBtn = document.getElementById('signUpBtn');
  if (signUpBtn) signUpBtn.addEventListener('click', handleSignUp);

  var loginBtn = document.getElementById('loginBtn');
  if (loginBtn) loginBtn.addEventListener('click', handleSignIn);
}

function handleSignUp() {
  if (!sbClient) {
    if (typeof window.supabase === 'undefined') {
      showAuthError('Supabase library failed to load. Check your internet connection and refresh.');
    } else {
      showAuthError('Supabase is not configured. Set SUPABASE_URL and SUPABASE_ANON_KEY in your Netlify environment variables.');
    }
    return;
  }

  var email = document.getElementById('signupEmail').value.trim();
  var password = document.getElementById('signupPassword').value;

  if (!email || !password) { showAuthError('Please enter email and password.'); return; }
  if (password.length < 8) { showAuthError('Password must be at least 8 characters.'); return; }

  showAuthLoading(true);
  sbClient.auth.signUp({ email: email, password: password })
    .then(function (result) {
      if (result.error) throw result.error;
      if (result.data.session) {
        document.getElementById('authModal').classList.remove('active');
        onAuthSuccess(result.data.session);
        window.location.href = 'dashboard.html';
      } else {
        showAuthError('Account created! Sign in with your email and password.');
        document.getElementById('loginEmail').value = email;
        openAuthModal('signin');
      }
    })
    .catch(function (err) { showAuthError(err.message || 'Sign up failed.'); })
    .finally(function () { showAuthLoading(false); });
}

function handleSignIn() {
  if (!sbClient) {
    if (typeof window.supabase === 'undefined') {
      showAuthError('Supabase library failed to load. Check your internet connection and refresh.');
    } else {
      showAuthError('Supabase is not configured. Set SUPABASE_URL and SUPABASE_ANON_KEY in your Netlify environment variables.');
    }
    return;
  }

  var email = document.getElementById('loginEmail').value.trim();
  var password = document.getElementById('loginPassword').value;

  if (!email || !password) { showAuthError('Please enter email and password.'); return; }

  showAuthLoading(true);
  sbClient.auth.signInWithPassword({ email: email, password: password })
    .then(function (result) {
      if (result.error) throw result.error;
      document.getElementById('authModal').classList.remove('active');
      onAuthSuccess(result.data.session);
      window.location.href = 'dashboard.html';
    })
    .catch(function (err) { showAuthError(err.message || 'Sign in failed.'); })
    .finally(function () { showAuthLoading(false); });
}

function onAuthSuccess(session) {
  currentUser = session.user;
  authToken = session.access_token;
  isLoggedIn = true;
  isGuest = false;

  // Persist auth state in localStorage for cross-page access
  try {
    localStorage.setItem('titleforge_auth', JSON.stringify({
      email: session.user.email,
      token: session.access_token,
      isLoggedIn: true
    }));
  } catch (e) {}

  isPro = localStorage.getItem('titleforge_pro') === 'true' ||
    (session.user && session.user.user_metadata && session.user.user_metadata.isPro);

  updateNavUserUI(session.user.email);

  // Reload usage from server
  loadUsageFromServer();

  // Apply Pro UI (style buttons, toggles, info text)
  applyProUI();
}

function handleLogout() {
  if (sbClient) sbClient.auth.signOut();
  currentUser = null;
  authToken = null;
  isLoggedIn = false;
  isGuest = true;
  isPro = false;

  // Clear persisted auth state
  try { localStorage.removeItem('titleforge_auth'); } catch (e) {}

  var userBar = document.getElementById('userBar');
  var guestBanner = document.getElementById('guestBanner');
  var navSignIn = document.getElementById('navSignIn');
  var navSignUp = document.getElementById('navSignUp');
  var navUser = document.getElementById('navUser');
  var navGuestBtns = document.getElementById('navGuestBtns');
  var navDashboard = document.getElementById('navDashboard');

  if (userBar) userBar.style.display = 'none';
  if (guestBanner) guestBanner.style.display = 'flex';
  if (navSignIn) navSignIn.style.display = 'inline-block';
  if (navSignUp) navSignUp.style.display = 'inline-block';
  if (navUser) navUser.style.display = 'none';
  if (navGuestBtns) navGuestBtns.style.display = 'flex';
  if (navDashboard) navDashboard.style.display = 'none';

  loadGuestUsage();
  renderCategories();
  setupSlider();
  updateUsageDisplay();
  updateStickyCta();

  // Redirect to home if on dashboard page
  if (window.location.pathname.indexOf('dashboard') !== -1) {
    window.location.href = 'index.html';
  }
}

// Show logged-in nav state with avatar dropdown
function updateNavUserUI(email) {
  var navSignIn = document.getElementById('navSignIn');
  var navSignUp = document.getElementById('navSignUp');
  var navUser = document.getElementById('navUser');
  var navGuestBtns = document.getElementById('navGuestBtns');
  var navDashboard = document.getElementById('navDashboard');
  var navAvatar = document.getElementById('navAvatar');
  var navDropdownEmail = document.getElementById('navDropdownEmail');
  var navDropdownName = document.getElementById('navDropdownName');
  var userBar = document.getElementById('userBar');
  var guestBanner = document.getElementById('guestBanner');
  var userEmail = document.getElementById('userEmail');

  if (userBar) userBar.style.display = 'flex';
  if (guestBanner) guestBanner.style.display = 'none';
  if (userEmail && email) userEmail.textContent = email;
  if (navSignIn) navSignIn.style.display = 'none';
  if (navSignUp) navSignUp.style.display = 'none';
  if (navGuestBtns) navGuestBtns.style.display = 'none';
  if (navUser) navUser.style.display = 'block';
  if (navDashboard) navDashboard.style.display = 'inline';

  // Set avatar letter
  if (navAvatar && email) navAvatar.textContent = email.charAt(0).toUpperCase();
  if (navDropdownEmail && email) navDropdownEmail.textContent = email;
  if (navDropdownName && email) navDropdownName.textContent = email.split('@')[0];

  // Update mobile CTA
  updateStickyCta();
}

// Wire avatar dropdown (called once from init)
function setupAvatarDropdown() {
  var avatar = document.getElementById('navAvatar');
  var dropdown = document.getElementById('navUserDropdown');
  if (!avatar || !dropdown) return;
  avatar.addEventListener('click', function (e) {
    e.stopPropagation();
    dropdown.classList.toggle('show');
  });
  document.addEventListener('click', function (e) {
    if (e.target !== avatar && !dropdown.contains(e.target)) {
      dropdown.classList.remove('show');
    }
  });
}

// ============================================
// FLOATING GENERATOR
// ============================================
var floatingGenOpen = false;
function setupFloatingGenerator() {
  var fab = document.createElement('div');
  fab.id = 'floatingGenFab';
  fab.innerHTML = '⚡';
  fab.title = 'Quick Generate';
  fab.addEventListener('click', function () { floatingGenOpen ? closeFloatingGenerator() : openFloatingGenerator(); });
  var modal = document.createElement('div');
  modal.id = 'floatingGenModal';
  modal.style.display = 'none';
  modal.innerHTML = '<div class="floating-gen-header"><span class="floating-gen-title">Quick Generate</span><div class="floating-gen-actions"><button class="floating-gen-minimize" id="floatingGenMinimize" title="Minimize">_</button><button class="floating-gen-close" id="floatingGenClose" title="Close">&times;</button></div></div><div class="floating-gen-body"><div class="form-group"><input type="text" id="floatingKeyword" placeholder="Keyword or existing title..." class="floating-gen-input" /></div><div class="form-group"><label class="floating-gen-label">Categories</label><div class="floating-gen-cats" id="floatingGenCats"></div></div><div class="form-group"><label class="floating-gen-label">Style</label><div class="floating-gen-styles" id="floatingGenStyles"></div></div><div class="floating-gen-row"><div class="form-group" style="flex:1;"><label class="floating-gen-label">Genre</label><select id="floatingGenre" class="floating-gen-select"><option value="any">Any</option><option value="fiction">Fiction</option><option value="nonfiction">Non-fiction</option><option value="business">Business</option><option value="science">Science</option><option value="selfhelp">Self-help</option><option value="history">History</option><option value="fantasy">Fantasy</option><option value="romance">Romance</option><option value="mystery">Mystery</option></select></div><div class="form-group" style="flex:0 0 80px;"><label class="floating-gen-label">Qty</label><input type="number" id="floatingQty" value="5" min="1" max="100" class="floating-gen-input" style="width:80px;text-align:center;" /></div></div><div class="floating-gen-toggles"><label class="floating-gen-toggle"><input type="checkbox" id="floatingCrossMedium" /><span>Cross-medium</span></label><label class="floating-gen-toggle"><input type="checkbox" id="floatingSubtitles" disabled /><span>Subtitles <span class="pro-badge">PRO</span></span></label><label class="floating-gen-toggle"><input type="checkbox" id="floatingTranslate" disabled /><span>Translate <span class="pro-badge">PRO</span></span></label></div><div id="floatingTranslateLangs" class="floating-gen-langs" style="display:none;"><select id="floatingTransLang" class="floating-gen-select"><option value="es">Spanish</option><option value="fr">French</option><option value="de">German</option><option value="it">Italian</option><option value="pt">Portuguese</option><option value="ja">Japanese</option><option value="ko">Korean</option><option value="zh">Chinese</option></select></div><button id="floatingGenBtn" class="btn btn-primary btn-full" style="margin-top:6px;">Generate</button><div id="floatingGenError" class="error-msg" style="display:none;margin-top:8px;"></div><div id="floatingGenLoading" class="floating-gen-loading" style="display:none;">Generating...</div><div id="floatingGenResults" class="floating-gen-results"></div></div>';
  document.body.appendChild(fab);
  document.body.appendChild(modal);

  // Populate categories
  var catsContainer = document.getElementById('floatingGenCats');
  ALL_CATEGORIES.forEach(function (cat) {
    var label = document.createElement('label');
    label.className = 'floating-gen-cat';
    var cb = document.createElement('input');
    cb.type = 'checkbox';
    cb.value = cat.id;
    if (!cat.free && !isPro) cb.disabled = true;
    label.appendChild(cb);
    label.appendChild(document.createTextNode(' ' + cat.label));
    if (!cat.free) {
      var badge = document.createElement('span');
      badge.className = 'pro-badge';
      badge.textContent = 'PRO';
      label.appendChild(badge);
    }
    catsContainer.appendChild(label);
  });

  // Populate styles
  var stylesContainer = document.getElementById('floatingGenStyles');
  var selectedStyle = 'normal';
  STYLES.forEach(function (s) {
    var btn = document.createElement('button');
    btn.className = 'floating-gen-style-btn' + (s.id === selectedStyle ? ' active' : '');
    btn.setAttribute('data-style', s.id);
    btn.textContent = s.label;
    btn.setAttribute('type', 'button');
    if (!s.free && !isPro) {
      btn.classList.add('pro-locked');
      btn.addEventListener('click', function (e) { e.preventDefault(); showUpgradeModal(); });
    } else {
      btn.addEventListener('click', function () {
        stylesContainer.querySelectorAll('.floating-gen-style-btn').forEach(function (b) { b.classList.remove('active'); });
        btn.classList.add('active');
        selectedStyle = s.id;
      });
    }
    stylesContainer.appendChild(btn);
  });

  // Wire toggles
  var ft = document.getElementById('floatingTranslate');
  var fl = document.getElementById('floatingTranslateLangs');
  if (ft && fl) ft.addEventListener('change', function () { fl.style.display = ft.checked ? 'block' : 'none'; });

  // Close and generate wiring
  document.getElementById('floatingGenClose').addEventListener('click', closeFloatingGenerator);
  document.getElementById('floatingGenMinimize').addEventListener('click', closeFloatingGenerator);
  document.getElementById('floatingGenBtn').addEventListener('click', generateFromFloating);
  document.getElementById('floatingKeyword').addEventListener('keydown', function (e) { if (e.key === 'Enter') generateFromFloating(); });

  // Wire sticky CTA button to open floating generator
  var stickyBtn = document.getElementById('stickyCtaBtn');
  if (stickyBtn) {
    stickyBtn.addEventListener('click', function (e) {
      e.preventDefault();
      openFloatingGenerator();
    });
  }
}
function openFloatingGenerator() {
  floatingGenOpen = true;
  var modal = document.getElementById('floatingGenModal');
  var fab = document.getElementById('floatingGenFab');
  if (modal) modal.style.display = 'block';
  if (fab) fab.style.display = 'none';
  var input = document.getElementById('floatingKeyword');
  if (input) setTimeout(function () { input.focus(); }, 100);
}
function closeFloatingGenerator() {
  floatingGenOpen = false;
  var modal = document.getElementById('floatingGenModal');
  var fab = document.getElementById('floatingGenFab');
  if (modal) modal.style.display = 'none';
  if (fab) fab.style.display = 'flex';
}
function generateFromFloating() {
  var keyword = document.getElementById('floatingKeyword').value.trim();
  if (!keyword) { showFloatingError('Enter a keyword.'); return; }
  if (!canGenerate()) {
    if (isGuest) { closeFloatingGenerator(); openAuthModal('signup'); }
    else { closeFloatingGenerator(); showUpgradeModal(); }
    return;
  }
  var checkedCats = [];
  document.querySelectorAll('#floatingGenCats input:checked').forEach(function (cb) { checkedCats.push(cb.value); });
  if (checkedCats.length === 0) { showFloatingError('Select at least one category.'); return; }
  var activeStyle = document.querySelector('#floatingGenStyles .active');
  var style = activeStyle ? activeStyle.getAttribute('data-style') : 'normal';
  var genre = document.getElementById('floatingGenre').value;
  var quantity = Math.min(Math.max(parseInt(document.getElementById('floatingQty').value) || 5, 1), 100);
  var crossMedium = document.getElementById('floatingCrossMedium').checked;
  var wantSubtitles = document.getElementById('floatingSubtitles').checked;
  var wantTranslate = document.getElementById('floatingTranslate').checked;
  var transLang = wantTranslate ? document.getElementById('floatingTransLang').value : null;
  document.getElementById('floatingGenLoading').style.display = 'block';
  document.getElementById('floatingGenError').style.display = 'none';
  document.getElementById('floatingGenBtn').disabled = true;
  document.getElementById('floatingGenResults').innerHTML = '';
  var headers = { 'Content-Type': 'application/json' };
  if (authToken) headers['Authorization'] = 'Bearer ' + authToken;
  fetch('/.netlify/functions/generate', {
    method: 'POST', headers: headers,
    body: JSON.stringify({
      keyword: keyword, categories: checkedCats, genre: genre, style: style,
      quantity: quantity, crossMedium: crossMedium,
      includeSubtitles: wantSubtitles, includeTranslation: wantTranslate, translateLang: transLang
    })
  }).then(function (r) {
    if (!r.ok) return r.json().then(function (e) { throw new Error(e.error || 'Server error'); });
    return r.json();
  }).then(function (data) {
    if (data.crossMedium) {
      var allTitles = [];
      Object.keys(data.crossMedium).forEach(function (k) {
        (data.crossMedium[k] || []).forEach(function (t) { allTitles.push(t); });
      });
      displayFloatingResults(allTitles);
    } else {
      displayFloatingResults(data.titles);
    }
    dailyUsage++;
    saveGuestUsage();
  }).catch(function (err) { showFloatingError(err.message); })
  .finally(function () {
    document.getElementById('floatingGenLoading').style.display = 'none';
    document.getElementById('floatingGenBtn').disabled = false;
  });
}
function displayFloatingResults(titles) {
  var container = document.getElementById('floatingGenResults');
  container.innerHTML = '';
  if (!titles || titles.length === 0) { container.innerHTML = '<p style="font-size:13px;color:var(--text-secondary);padding:10px;">No titles generated. Try again.</p>'; return; }
  titles.slice(0, 5).forEach(function (item, i) {
    var el = document.createElement('div');
    el.className = 'floating-gen-item';
    el.innerHTML = '<span class="floating-gen-num">' + (i + 1) + '</span><span class="floating-gen-title">' + escapeHtml(item.title) + '</span><span class="floating-gen-score" style="color:' + (item.score >= 75 ? '#16a34a' : item.score >= 50 ? '#d97706' : '#dc2626') + '">' + (item.score || '') + '</span>';
    container.appendChild(el);
  });
}
function showFloatingError(msg) {
  var el = document.getElementById('floatingGenError');
  if (!el) return;
  el.textContent = msg;
  el.style.display = 'block';
}

// Refresh floating generator Pro gating (called after login / Pro status change)
function updateFloatingGenLocked() {
  var catsContainer = document.getElementById('floatingGenCats');
  if (catsContainer) {
    catsContainer.querySelectorAll('.floating-gen-cat').forEach(function (label) {
      var input = label.querySelector('input');
      if (!input) return;
      var catDef = ALL_CATEGORIES.find(function (c) { return c.id === input.value; });
      if (catDef && !catDef.free) {
        input.disabled = !isPro;
        label.style.opacity = isPro ? '1' : '';
      }
    });
  }
  var stylesContainer = document.getElementById('floatingGenStyles');
  if (stylesContainer) {
    stylesContainer.querySelectorAll('.floating-gen-style-btn').forEach(function (btn) {
      var styleId = btn.getAttribute('data-style');
      var styleDef = STYLES.find(function (s) { return s.id === styleId; });
      if (styleDef && !styleDef.free) {
        if (isPro) {
          btn.classList.remove('pro-locked');
          // Replace the locked click handler with a normal one
          btn.onclick = function () {
            stylesContainer.querySelectorAll('.floating-gen-style-btn').forEach(function (b) { b.classList.remove('active'); });
            btn.classList.add('active');
          };
        } else {
          btn.classList.add('pro-locked');
          btn.onclick = function (e) { e.preventDefault(); showUpgradeModal(); };
        }
      }
    });
  }
}

function updateStickyCta() {
  var textEl = document.getElementById('stickyCtaText');
  var btnEl = document.getElementById('stickyCtaBtn');
  var bar = document.getElementById('stickyCta');
  if (!textEl || !btnEl || !bar) return;
  if (isPro) {
    textEl.textContent = 'Quick Generate';
    btnEl.textContent = '⚡ Generate';
    bar.classList.remove('hide-on-mobile');
  } else if (isLoggedIn) {
    textEl.textContent = 'Upgrade to Pro';
    btnEl.textContent = 'See Plans →';
  } else {
    textEl.textContent = 'Generate titles that get clicked';
    btnEl.textContent = 'Try Free →';
  }
}

function showAuthError(msg) {
  var el = document.getElementById('authError');
  if (!el) return;
  el.textContent = msg;
  el.style.display = 'block';
}

function showAuthLoading(show) {
  var el = document.getElementById('authLoading');
  if (!el) return;
  el.style.display = show ? 'block' : 'none';
}

// ============================================
// SERVER USAGE (logged in only)
// ============================================

// Update all Pro-gated UI elements when isPro changes
function applyProUI() {
  // Re-render style buttons (Pro styles become selectable)
  setupStyleButtons();
  // Enable/disable subtitles and translate toggles
  var subToggle = document.getElementById('subtitlesToggle');
  var transToggle = document.getElementById('translateToggle');
  if (subToggle) subToggle.disabled = !isPro;
  if (transToggle) transToggle.disabled = !isPro;
  // Show/hide translate language selector
  var translateLangs = document.getElementById('translateLangs');
  if (translateLangs) translateLangs.style.display = (isPro && transToggle && transToggle.checked) ? 'block' : 'none';
  // Update the info text above the tool
  var sectionSub = document.querySelector('.tool-container .section-sub');
  if (sectionSub) {
    if (isPro) {
      sectionSub.innerHTML = 'You\'re on <strong>Pro</strong> — unlimited generations, all 16 categories, up to 100 titles per batch, appeal scores, subtitles &amp; translation.';
    } else {
      sectionSub.innerHTML = 'Free tier: 5 generations per day, up to 10 titles each, 5 categories. <a href="#pricing" class="inline-link">Upgrade to Pro</a> for 100 titles per generation, all 16 categories, appeal scores, subtitles &amp; translation.';
    }
  }
  renderCategories();
  setupSlider();
  updateUsageDisplay();
  updateFloatingGenLocked();
  // Update floating generator toggles
  var fSub = document.getElementById('floatingSubtitles');
  var fTrans = document.getElementById('floatingTranslate');
  if (fSub) fSub.disabled = !isPro;
  if (fTrans) fTrans.disabled = !isPro;
}

function loadUsageFromServer() {
  if (!authToken) return;
  fetch('/.netlify/functions/usage', { headers: { 'Authorization': 'Bearer ' + authToken } })
    .then(function (r) { return r.json(); })
    .then(function (data) {
      dailyUsage = data.usage || 0;
      isPro = data.isPro || false;
      if (isPro) localStorage.setItem('titleforge_pro', 'true');
      updateUsageDisplay();
      applyProUI();
    })
    .catch(function () { /* fallback to localStorage */ });
}

// Verify subscription with the server (checks Stripe status)
function verifySubscription() {
  if (!authToken) return Promise.resolve(false);
  return fetch('/.netlify/functions/verify-subscription', {
    headers: { 'Authorization': 'Bearer ' + authToken }
  })
    .then(function (r) { return r.json(); })
    .then(function (data) {
      if (data.isPro) {
        isPro = true;
        localStorage.setItem('titleforge_pro', 'true');
        updateUsageDisplay();
        applyProUI();
      }
      return data.isPro;
    })
    .catch(function () { return false; });
}

// ============================================
// DASHBOARD
// ============================================

var dashHistory = [];        // full history from server
var dashFavorites = [];      // favorited titles
var dashProjects = [];       // projects
var dashCurrentTab = 'overview';
var dashSearchQuery = '';
var dashFilterCategory = '';
var dashFilterSort = 'newest';

function saveToHistory(keyword, categories, genre, style, titles) {
  if (!authToken) return;
  fetch('/.netlify/functions/usage', {
    method: 'POST',
    headers: { 'Authorization': 'Bearer ' + authToken, 'Content-Type': 'application/json' },
    body: JSON.stringify({
      saveHistory: true,
      keyword: keyword, categories: categories, genre: genre, style: style,
      titles: titles.slice(0, 100)
    })
  }).then(function () { loadDashboard(); }).catch(function () {});
}

function loadDashboard() {
  if (!authToken) return;
  fetch('/.netlify/functions/usage', { headers: { 'Authorization': 'Bearer ' + authToken } })
    .then(function (r) { return r.json(); })
    .then(function (data) {
      dashHistory = data.history || [];
      dashFavorites = data.favorites || [];
      dashProjects = data.projects || [];
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
  renderSettingsTab();
}

// ============================================
// STATS BAR
// ============================================

function renderStatsBar() {
  var container = document.getElementById('dashStats');
  if (!container) return;

  var totalTitles = 0;
  dashHistory.forEach(function (entry) {
    var titles = typeof entry.titles === 'string' ? JSON.parse(entry.titles) : (entry.titles || []);
    totalTitles += titles.length;
  });

  var favCount = dashFavorites.length;
  var projCount = dashProjects.length;
  var planColor = isPro ? 'var(--forge)' : 'var(--text-secondary)';
  var planLabel = isPro ? 'Pro' : 'Free';

  container.innerHTML =
    '<div class="stat-card"><span class="stat-number">' + totalTitles + '</span><span class="stat-label">Titles generated</span></div>' +
    '<div class="stat-card"><span class="stat-number">' + favCount + '</span><span class="stat-label">Favorites</span></div>' +
    '<div class="stat-card"><span class="stat-number">' + projCount + '</span><span class="stat-label">Projects</span></div>' +
    '<div class="stat-card"><span class="stat-badge" style="background:' + planColor + ';">' + planLabel + '</span><span class="stat-label">Current plan</span></div>';
}

// ============================================
// OVERVIEW TAB
// ============================================

function renderOverviewTab() {
  var container = document.getElementById('dashOverviewList');
  if (!container) return;

  var html = '';

  // Usage summary card
  var limit = isPro ? Infinity : getUsageLimit();
  var remaining = isPro ? 'Unlimited' : (limit - dailyUsage);
  var usagePct = isPro ? 100 : Math.min(Math.round((dailyUsage / limit) * 100), 100);

  html += '<div class="overview-card">';
  html += '<h3 class="overview-card-title">Your usage today</h3>';
  html += '<div class="usage-row"><span>' + dailyUsage + ' generation' + (dailyUsage !== 1 ? 's' : '') + '</span><span>' + (isPro ? 'Unlimited' : 'of ' + limit) + '</span></div>';
  if (!isPro) {
    html += '<div class="usage-bar-track"><div class="usage-bar-fill" style="width:' + usagePct + '%;background:var(--forge);"></div></div>';
    html += '<p class="overview-remaining">' + remaining + (remaining === 1 ? ' generation' : ' generations') + ' remaining today</p>';
  }
  html += '</div>';

  // Recent activity
  var recentHistory = dashHistory.slice(0, 3);
  if (recentHistory.length > 0) {
    html += '<h3 class="overview-section-title">Recent activity</h3>';
    recentHistory.forEach(function (entry) {
      var date = new Date(entry.created_at).toLocaleDateString();
      var titleCount = (typeof entry.titles === 'string' ? JSON.parse(entry.titles) : (entry.titles || [])).length;
      html += '<div class="overview-item">';
      html += '<div class="overview-item-icon">✦</div>';
      html += '<div class="overview-item-body"><strong>' + escapeHtml(entry.keyword) + '</strong><span class="overview-item-meta">' + titleCount + ' title' + (titleCount !== 1 ? 's' : '') + ' &middot; ' + date + '</span></div>';
      html += '</div>';
    });
    html += '<a href="#" onclick="switchDashTab(\'history\');return false;" class="overview-view-all">View all history →</a>';
  } else {
    html += '<div class="overview-empty">';
    html += '<div class="overview-empty-icon">🎯</div>';
    html += '<h3>No titles generated yet</h3>';
    html += '<p>Go to the generator and create your first batch of titles.</p>';
    html += '<a href="index.html#tool" class="btn btn-primary" style="display:inline-block;margin-top:12px;">Generate Your First Titles →</a>';
    html += '</div>';
  }

  // Quick actions
  html += '<h3 class="overview-section-title" style="margin-top:24px;">Quick actions</h3>';
  html += '<div class="overview-actions">';
  html += '<a href="index.html#tool" class="overview-action-btn"><span class="overview-action-icon">⚡</span> Generate Titles</a>';
  if (dashFavorites.length > 0) {
    html += '<a href="#" onclick="switchDashTab(\'favorites\');return false;" class="overview-action-btn"><span class="overview-action-icon">★</span> Browse Favorites</a>';
  }
  if (dashProjects.length > 0) {
    html += '<a href="#" onclick="switchDashTab(\'projects\');return false;" class="overview-action-btn"><span class="overview-action-icon">📁</span> Open Projects</a>';
  }
  html += '</div>';

  container.innerHTML = html;
}

function switchDashTab(tabName) {
  var tabs = document.querySelectorAll('.dash-tab');
  tabs.forEach(function (t) { t.classList.remove('active'); });
  tabs.forEach(function (t) {
    if (t.getAttribute('data-dashtab') === tabName) {
      t.classList.add('active');
    }
  });
  dashCurrentTab = tabName;
  var panels = ['overview', 'history', 'favorites', 'projects', 'export', 'settings'];
  panels.forEach(function (p) {
    var panel = document.getElementById('dash' + p.charAt(0).toUpperCase() + p.slice(1));
    if (panel) panel.style.display = (p === dashCurrentTab) ? 'block' : 'none';
  });
}

function getFilteredHistory() {
  var filtered = dashHistory.slice();

  // Search filter
  if (dashSearchQuery) {
    var q = dashSearchQuery.toLowerCase();
    filtered = filtered.filter(function (entry) {
      if (entry.keyword && entry.keyword.toLowerCase().indexOf(q) !== -1) return true;
      var titles = typeof entry.titles === 'string' ? JSON.parse(entry.titles) : (entry.titles || []);
      return titles.some(function (t) {
        var titleText = typeof t === 'string' ? t : t.title;
        return titleText && titleText.toLowerCase().indexOf(q) !== -1;
      });
    });
  }

  // Category filter
  if (dashFilterCategory) {
    filtered = filtered.filter(function (entry) {
      var cats = entry.categories || [];
      return cats.indexOf(dashFilterCategory) !== -1;
    });
  }

  // Sort
  if (dashFilterSort === 'oldest') {
    filtered.sort(function (a, b) { return new Date(a.created_at) - new Date(b.created_at); });
  } else if (dashFilterSort === 'alpha') {
    filtered.sort(function (a, b) { return (a.keyword || '').localeCompare(b.keyword || ''); });
  } else if (dashFilterSort === 'score') {
    filtered.sort(function (a, b) {
      var aTop = getTopScore(a);
      var bTop = getTopScore(b);
      return bTop - aTop;
    });
  } else {
    // newest
    filtered.sort(function (a, b) { return new Date(b.created_at) - new Date(a.created_at); });
  }

  return filtered;
}

function getTopScore(entry) {
  var titles = typeof entry.titles === 'string' ? JSON.parse(entry.titles) : (entry.titles || []);
  var max = 0;
  titles.forEach(function (t) {
    var s = typeof t === 'object' ? (t.score || 0) : 0;
    if (s > max) max = s;
  });
  return max;
}

// ============================================
// FAVORITES
// ============================================

function isFavorited(titleText) {
  return dashFavorites.some(function (f) { return f.title === titleText; });
}

function toggleFavorite(titleText, starBtn, sourceKeyword) {
  if (isFavorited(titleText)) {
    // Remove
    dashFavorites = dashFavorites.filter(function (f) { return f.title !== titleText; });
    if (starBtn) starBtn.classList.remove('starred');
    removeFavoriteFromServer(titleText);
  } else {
    // Add
    var fav = { title: titleText, keyword: sourceKeyword || currentKeyword, created_at: new Date().toISOString() };
    dashFavorites.unshift(fav);
    if (starBtn) starBtn.classList.add('starred');
    saveFavoriteToServer(fav);
  }

  // Re-render favorites tab if it's visible
  if (dashCurrentTab === 'favorites') renderFavoritesTab();
}

function saveFavoriteToServer(fav) {
  if (!authToken) return;

  fetch('/.netlify/functions/usage', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', 'Authorization': 'Bearer ' + authToken },
    body: JSON.stringify({ action: 'add_favorite', favorite: fav })
  }).catch(function () {});
}

function removeFavoriteFromServer(titleText) {
  if (!authToken) return;

  fetch('/.netlify/functions/usage', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', 'Authorization': 'Bearer ' + authToken },
    body: JSON.stringify({ action: 'remove_favorite', title: titleText })
  }).catch(function () {});
}

function renderHistoryTab() {
  var container = document.getElementById('dashHistoryList');
  if (!container) return;

  var filtered = getFilteredHistory();

  if (filtered.length === 0) {
    container.innerHTML = '<div class="dash-empty"><div class="dash-empty-icon">🎯</div><p class="dash-empty-text">' + (dashSearchQuery ? 'No results match your search.' : 'No titles generated yet.') + '</p>' + (dashSearchQuery ? '' : '<a href="index.html#tool" class="btn btn-primary" style="display:inline-block;margin-top:12px;">Generate Your First Titles →</a>') + '</div>';
    return;
  }

  container.innerHTML = '';
  filtered.forEach(function (entry) {
    var card = document.createElement('div');
    card.className = 'history-card';

    var date = new Date(entry.created_at).toLocaleString();
    var titles = typeof entry.titles === 'string' ? JSON.parse(entry.titles) : (entry.titles || []);

    var header = document.createElement('div');
    header.className = 'history-header';
    header.innerHTML = '<span class="history-keyword">"' + escapeHtml(entry.keyword) + '"</span><span class="history-date">' + date + '</span>';
    card.appendChild(header);

    var meta = document.createElement('div');
    meta.className = 'history-meta';
    meta.innerHTML =
      '<span class="history-tag">' + escapeHtml((entry.categories || []).join(', ')) + '</span>' +
      '<span class="history-tag">' + escapeHtml(entry.genre || 'any genre') + '</span>' +
      '<span class="history-tag">' + escapeHtml(entry.style || 'normal') + '</span>';
    card.appendChild(meta);

    var titlesList = document.createElement('div');
    titlesList.className = 'history-titles';
    titles.slice(0, 10).forEach(function (t) {
      var titleText = typeof t === 'string' ? t : t.title;
      var score = typeof t === 'object' ? t.score : null;

      var itemDiv = document.createElement('div');
      itemDiv.className = 'history-title-item';
      itemDiv.style.display = 'flex';
      itemDiv.style.alignItems = 'center';
      itemDiv.style.gap = '10px';
      itemDiv.style.padding = '6px 0';

      // Title text
      var textSpan = document.createElement('span');
      textSpan.style.flex = '1';
      textSpan.textContent = titleText;
      itemDiv.appendChild(textSpan);

      // Score badge
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

      // Breakdown toggle (inline) — show labels with PRO badges for free users
      if (typeof t === 'object' && t.breakdown) {
        var bd = t.breakdown;
        var bdLabels = [];
        if (bd.curiosityGap) bdLabels.push('Curiosity gap');
        if (bd.emotionalTrigger) bdLabels.push('Emotion');
        if (bd.powerWords && bd.powerWords.length) bdLabels.push('Power words');
        if (bd.lengthAnalysis) bdLabels.push('Length');
        if (bd.specificity) bdLabels.push('Specificity');
        if (bd.uniqueness) bdLabels.push('Uniqueness');
        if (bd.memorability) bdLabels.push('Memorability');
        if (bd.meaningDepth) bdLabels.push('Meaning');
        if (bd.pronunciationEase) bdLabels.push('Pronunciation');
        if (bd.originVibe) bdLabels.push('Origin');

        if (isPro) {
          var bdInlineBtn = document.createElement('button');
          bdInlineBtn.className = 'dash-bd-btn';
          bdInlineBtn.textContent = 'View';
          bdInlineBtn.title = bdLabels.join(' | ');
          bdInlineBtn.addEventListener('click', function () {
            var lines = [];
            if (bd.curiosityGap) lines.push('<div class="popup-row"><span class="popup-label">Curiosity gap</span><span class="popup-value">' + bd.curiosityGap + '</span></div>');
            if (bd.emotionalTrigger) lines.push('<div class="popup-row"><span class="popup-label">Emotion</span><span class="popup-value">' + bd.emotionalTrigger + '</span></div>');
            if (bd.powerWords && bd.powerWords.length) lines.push('<div class="popup-row"><span class="popup-label">Power words</span><span class="popup-value">' + bd.powerWords.join(', ') + '</span></div>');
            if (bd.lengthAnalysis) lines.push('<div class="popup-row"><span class="popup-label">Length</span><span class="popup-value">' + bd.lengthAnalysis + '</span></div>');
            if (bd.specificity) lines.push('<div class="popup-row"><span class="popup-label">Specificity</span><span class="popup-value">' + bd.specificity + '</span></div>');
            if (bd.uniqueness) lines.push('<div class="popup-row"><span class="popup-label">Uniqueness</span><span class="popup-value">' + bd.uniqueness + '</span></div>');
            if (bd.memorability) lines.push('<div class="popup-row"><span class="popup-label">Memorability</span><span class="popup-value">' + bd.memorability + '</span></div>');
            if (bd.meaningDepth) lines.push('<div class="popup-row"><span class="popup-label">Meaning</span><span class="popup-value">' + bd.meaningDepth + '</span></div>');
            if (bd.pronunciationEase) lines.push('<div class="popup-row"><span class="popup-label">Pronunciation</span><span class="popup-value">' + bd.pronunciationEase + '</span></div>');
            if (bd.originVibe) lines.push('<div class="popup-row"><span class="popup-label">Origin</span><span class="popup-value">' + bd.originVibe + '</span></div>');
            showBreakdownPopup(lines.join(''));
          });
          itemDiv.appendChild(bdInlineBtn);
        } else if (bdLabels.length > 0) {
          // Show all breakdown labels with PRO badges, just like the main results page
          bdLabels.forEach(function (lbl) {
            var bdPro = document.createElement('span');
            bdPro.className = 'dash-pro-badge-item';
            bdPro.innerHTML = '<span class="pro-badge">PRO</span> ' + lbl;
            bdPro.style.cursor = 'pointer';
            bdPro.addEventListener('click', function () { showUpgradeModal(); });
            itemDiv.appendChild(bdPro);
          });
        }
      }

      // Project button (Pro only)
      if (authToken && titleText) {
        if (isPro) {
          var hProj = document.createElement('button');
          hProj.className = 'dash-proj-btn';
          hProj.title = 'Add to project';
          hProj.textContent = '📁';
          hProj.addEventListener('click', function (e) {
            e.stopPropagation();
            var existing = document.querySelector('.proj-dropdown.active');
            if (existing) { existing.remove(); }
            showProjectPicker(titleText, entry.keyword, (typeof t === 'object' ? t.score : null) || null, hProj);
          });
          itemDiv.appendChild(hProj);
        } else {
          var hProj = document.createElement('button');
          hProj.className = 'dash-proj-btn locked';
          hProj.title = 'Upgrade to use projects';
          hProj.textContent = '📁';
          hProj.addEventListener('click', function () { showUpgradeModal(); });
          itemDiv.appendChild(hProj);
        }
      }

      // Favorite star
      if (authToken && titleText) {
        var hStar = document.createElement('button');
        var isFav = isFavorited(titleText);
        hStar.className = 'dash-star' + (isFav ? ' starred' : '');
        hStar.innerHTML = isFav ? '★' : '☆';
        hStar.title = 'Save to favorites';
        hStar.addEventListener('click', function () {
          toggleFavorite(titleText, hStar, entry.keyword);
          hStar.className = 'dash-star' + (isFavorited(titleText) ? ' starred' : '');
          hStar.innerHTML = isFavorited(titleText) ? '★' : '☆';
        });
        itemDiv.appendChild(hStar);
      }

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

function renderFavoritesTab() {
  var container = document.getElementById('dashFavoritesList');
  if (!container) return;

  if (dashFavorites.length === 0) {
    container.innerHTML = '<div class="dash-empty"><div class="dash-empty-icon">★</div><p class="dash-empty-text">Build your collection.</p><p style="font-size:13px;color:var(--text-secondary);margin-bottom:16px;">Star any title from your history to save it here.</p><a href="#" onclick="switchDashTab(\'history\');return false;" class="btn btn-outline" style="display:inline-block;">Browse Generated Titles →</a></div>';
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

function renderProjectsTab() {
  var container = document.getElementById('dashProjectsList');
  if (!container) return;

  if (dashProjects.length === 0) {
    container.innerHTML = '<div class="dash-empty"><div class="dash-empty-icon">📁</div><p class="dash-empty-text">Organize your work.</p><p style="font-size:13px;color:var(--text-secondary);margin-bottom:16px;">Group your best titles into projects for easy access.</p></div>';
    return;
  }

  container.innerHTML = '';
  dashProjects.forEach(function (proj) {
    var card = document.createElement('div');
    card.className = 'history-card';
    var projTitles = typeof proj.titles === 'string' ? JSON.parse(proj.titles) : (proj.titles || []);
    var count = projTitles.length;

    var header = document.createElement('div');
    header.className = 'history-header';
    var delBtn = document.createElement('button');
    delBtn.className = 'project-delete-btn';
    delBtn.textContent = '✕';
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

        // Notes expand
        if (typeof t === 'object') {
          var noteToggle = document.createElement('span');
          noteToggle.className = 'proj-note-toggle';
          noteToggle.textContent = t.notes ? ' 💬' : ' ✏️';
          noteToggle.title = t.notes ? 'View note' : 'Add note';
          noteToggle.style.cursor = 'pointer';
          noteToggle.style.fontSize = '12px';
          noteToggle.style.marginLeft = '8px';
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
            saveBtn.style.marginTop = '4px';
            saveBtn.style.padding = '4px 12px';
            saveBtn.addEventListener('click', function () {
              t.notes = textarea.value;
              if (authToken) {
                fetch('/.netlify/functions/usage', {
                  method: 'POST',
                  headers: { 'Content-Type': 'application/json', 'Authorization': 'Bearer ' + authToken },
                  body: JSON.stringify({
                    action: 'update_title_notes',
                    projectId: proj.id,
                    title: titleText,
                    notes: textarea.value
                  })
                }).catch(function () {});
              }
              noteToggle.textContent = textarea.value ? ' 💬' : ' ✏️';
              editor.remove();
            });
            editor.appendChild(textarea);
            editor.appendChild(saveBtn);
            item.appendChild(editor);
          });
          item.appendChild(noteToggle);
        }
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

function renderExportTab() {
  var preview = document.getElementById('exportPreview');
  if (!preview) return;

  var items = [];

  dashHistory.forEach(function (entry) {
    var titles = typeof entry.titles === 'string' ? JSON.parse(entry.titles) : (entry.titles || []);
    titles.forEach(function (t) {
      var titleText = typeof t === 'string' ? t : t.title;
      var score = typeof t === 'object' ? t.score : '';
      if (titleText) {
        items.push({
          title: titleText,
          score: score,
          keyword: entry.keyword || '',
          category: (entry.categories || []).join('; '),
          genre: entry.genre || '',
          style: entry.style || '',
          date: entry.created_at || ''
        });
      }
    });
  });

  if (items.length === 0) {
    preview.innerHTML = '<div class="dash-empty"><div class="dash-empty-icon">⬇</div><p class="dash-empty-text">Nothing to export yet.</p><p style="font-size:13px;color:var(--text-secondary);margin-bottom:16px;">Generate some titles first, then come back here to download them.</p><a href="index.html#tool" class="btn btn-primary" style="display:inline-block;">Generate Titles →</a></div>';
    return;
  }

  // Render selection list
  var html = '<div class="export-count-bar">' + items.length + ' titles — <span id="exportSelectedCount">0</span> selected</div>';
  html += '<div class="export-list">';
  items.forEach(function (item, i) {
    var scoreColor = '#c62828';
    if (item.score >= 75) scoreColor = '#4caf50';
    else if (item.score >= 50) scoreColor = '#e8a040';
    else if (item.score >= 25) scoreColor = '#ff9800';
    html += '<label class="export-item" data-index="' + i + '">';
    html += '<input type="checkbox" class="export-checkbox" data-index="' + i + '" />';
    html += '<span class="export-score" style="color:' + scoreColor + '">' + (item.score || '-') + '</span>';
    html += '<span class="export-title">' + escapeHtml(item.title) + '</span>';
    html += '<span class="export-meta">' + escapeHtml(item.keyword) + '</span>';
    html += '</label>';
  });
  html += '</div>';
  preview.innerHTML = html;

  // Wire checkboxes
  preview.querySelectorAll('.export-checkbox').forEach(function (cb) {
    cb.addEventListener('change', updateExportCount);
  });
  updateExportCount();

  function updateExportCount() {
    var checked = preview.querySelectorAll('.export-checkbox:checked').length;
    var el = document.getElementById('exportSelectedCount');
    if (el) el.textContent = checked;
  }
}

function renderSettingsTab() {
  var container = document.getElementById('settingsContent');
  if (!container) return;

  // Refresh usage data from server
  loadUsageFromServer();

  var plan = isPro ? 'Pro' : 'Free';
  var planColor = isPro ? '#16a34a' : 'var(--text-light)';

  // Get email from currentUser or localStorage fallback
  var userEmail = '';
  if (currentUser && currentUser.email) {
    userEmail = currentUser.email;
  } else {
    try {
      var stored = localStorage.getItem('titleforge_auth');
      if (stored) {
        var authData = JSON.parse(stored);
        userEmail = authData.email || '';
      }
    } catch (e) {}
  }

  var html = '<div class="settings-card">';
  html += '<div class="settings-row"><span class="settings-label">Current plan</span><span class="settings-value" style="color:' + planColor + ';font-weight:700;">' + plan + '</span></div>';
  html += '<div class="settings-row"><span class="settings-label">Email</span><span class="settings-value">' + escapeHtml(userEmail) + '</span></div>';
  html += '<div class="settings-row"><span class="settings-label">Daily usage</span><span class="settings-value">' + dailyUsage + ' generation' + (dailyUsage !== 1 ? 's' : '') + ' today</span></div>';
  html += '</div>';

  if (!isPro) {
    html += '<div class="settings-card settings-card-highlight">';
    html += '<h4 style="margin:0 0 8px;font-size:15px;color:var(--primary-dark);">Upgrade to Pro</h4>';
    html += '<p style="font-size:13px;color:var(--text-light);margin-bottom:14px;">Unlock all 16 categories, up to 100 titles per generation, appeal scores, subtitles, translation, and more.</p>';
    if (stripeProLink) {
      html += '<a href="' + stripeProLink + '" target="_blank" class="btn btn-primary" style="display:inline-block;">Upgrade Now — $19/mo</a>';
    } else {
      html += '<button onclick="showUpgradeModal()" class="btn btn-primary">Upgrade Now — $19/mo</button>';
    }
    html += '<p style="font-size:12px;color:var(--text-light);margin-top:10px;">Already paid? <button onclick="verifySubscriptionAndRefresh()" class="btn btn-ghost-dark btn-small" style="display:inline;padding:4px 12px;">Check Subscription</button></p>';
    html += '</div>';
  } else {
    html += '<div class="settings-card">';
    html += '<h4 style="margin:0 0 8px;font-size:15px;color:var(--primary-dark);">Subscription</h4>';
    html += '<p style="font-size:13px;color:var(--text-light);margin-bottom:14px;">You\'re on the Pro plan — unlimited generations, all features unlocked.</p>';
    if (stripePortalLink) {
      html += '<a href="' + stripePortalLink + '" target="_blank" class="btn btn-primary" style="display:inline-block;">Manage Billing</a>';
    } else {
      html += '<p style="font-size:12px;color:var(--text-light);font-style:italic;">Billing management link not configured.</p>';
    }
    html += '</div>';
  }

  html += '<div class="settings-card" style="margin-top:12px;">';
  html += '<h4 style="margin:0 0 8px;font-size:14px;color:var(--text-light);">Desktop Licenses</h4>';
  html += '<div id="licensesList" class="licenses-list"><p style="font-size:13px;color:var(--text-light);">Loading licenses...</p></div>';
  html += '</div>';

  html += '<div class="settings-card" style="margin-top:12px;">';
  html += '<h4 style="margin:0 0 8px;font-size:14px;color:var(--text-light);">Account</h4>';
  html += '<button onclick="handleLogout();window.location.href=\'index.html\';" class="btn btn-ghost-dark btn-small" style="padding:8px 20px;">Sign Out</button>';
  html += '</div>';

  container.innerHTML = html;

  // Load licenses after rendering
  loadLicenses();
}

function loadLicenses() {
  var list = document.getElementById('licensesList');
  if (!list) {
    // Element may not be in DOM yet — retry shortly
    setTimeout(loadLicenses, 200);
    return;
  }

  if (!authToken) {
    try {
      var stored = localStorage.getItem('titleforge_auth');
      if (stored) {
        var authData = JSON.parse(stored);
        if (authData.token) authToken = authData.token;
      }
    } catch (e) {}
  }

  if (!authToken) {
    list.innerHTML = '<p style="font-size:13px;color:var(--text-light);">Sign in to manage licenses.</p>';
    return;
  }

  fetch('/.netlify/functions/licenses', { headers: { 'Authorization': 'Bearer ' + authToken } })
    .then(function (r) {
      if (!r.ok) throw new Error('Status ' + r.status);
      return r.json();
    })
    .then(function (data) {
      var lic = data.licenses || [];
      if (lic.length === 0) {
        list.innerHTML = '<p style="font-size:13px;color:var(--text-light);margin-bottom:10px;">No desktop licenses yet.</p>';
        if (isPro) {
          list.innerHTML += '<button onclick="generateDesktopLicense()" class="btn btn-primary btn-small" style="padding:6px 16px;">Generate Desktop License</button>';
          list.innerHTML += '<p style="font-size:11px;color:var(--text-light);margin-top:6px;">As a Web Pro subscriber, you get a free Desktop Basic license.</p>';
        }
        return;
      }
      var html = '';
      lic.forEach(function (l) {
        var statusColor = l.is_active ? '#16a34a' : '#dc2626';
        var statusText = l.is_active ? 'Active' : 'Deactivated';
        var machines = (l.activated_machines || []).join(', ') || 'None';
        var expiresText = l.expires_at ? new Date(l.expires_at).toLocaleDateString() : 'Never';
        html += '<div class="license-item">';
        html += '<div class="license-header">';
        html += '<span class="license-badge license-badge-' + l.tier + '">' + l.tier.toUpperCase() + '</span>';
        html += '<span class="license-status" style="color:' + statusColor + ';">● ' + statusText + '</span>';
        if (l.is_active) {
          html += '<button onclick="deactivateLicense(\'' + l.license_key + '\')" class="btn-ghost-dark btn-small" style="padding:2px 10px;font-size:11px;margin-left:auto;">Deactivate</button>';
        }
        html += '</div>';
        html += '<code class="license-key">' + l.license_key + '</code>';
        html += '<div class="license-meta">Devices: ' + machines + ' &middot; Expires: ' + expiresText + ' &middot; Source: ' + l.source + '</div>';
        html += '<button onclick="copyLicenseKey(\'' + l.license_key + '\')" class="btn-ghost-dark btn-small" style="padding:2px 10px;font-size:11px;">Copy Key</button>';
        html += '</div>';
      });
      if (isPro) {
        html += '<button onclick="generateDesktopLicense()" class="btn btn-primary btn-small" style="margin-top:10px;padding:6px 16px;">+ Generate Another License</button>';
      }
      list.innerHTML = html;
    })
    .catch(function () {
      list.innerHTML = '<p style="font-size:13px;color:var(--text-light);">Could not load licenses. Make sure the licenses table exists in Supabase.</p>';
    });
}

function generateDesktopLicense() {
  if (!authToken) return;
  fetch('/.netlify/functions/licenses', {
    method: 'POST',
    headers: { 'Authorization': 'Bearer ' + authToken, 'Content-Type': 'application/json' },
    body: JSON.stringify({ action: 'generate', tier: 'basic', source: 'web-pro-subscriber' })
  })
    .then(function (r) { return r.json(); })
    .then(function (data) {
      if (data.license) {
        loadLicenses();
      } else {
        alert('Failed to generate license: ' + (data.error || 'unknown'));
      }
    })
    .catch(function () { alert('Could not generate license.'); });
}

function deactivateLicense(licenseKey) {
  if (!authToken || !confirm('Deactivate this license? It will no longer work on any device.')) return;
  fetch('/.netlify/functions/licenses', {
    method: 'POST',
    headers: { 'Authorization': 'Bearer ' + authToken, 'Content-Type': 'application/json' },
    body: JSON.stringify({ action: 'deactivate', licenseKey: licenseKey })
  })
    .then(function () { loadLicenses(); })
    .catch(function () {});
}

function copyLicenseKey(key) {
  if (navigator.clipboard) {
    navigator.clipboard.writeText(key);
  }
}

function setupDashboardTabs() {
  var tabs = document.querySelectorAll('.dash-tab');
  tabs.forEach(function (tab) {
    tab.addEventListener('click', function () {
      tabs.forEach(function (t) { t.classList.remove('active'); });
      tab.classList.add('active');
      dashCurrentTab = tab.getAttribute('data-dashtab');

      // Show/hide panels
      var panels = ['overview', 'history', 'favorites', 'projects', 'export', 'settings'];
      panels.forEach(function (p) {
        var panel = document.getElementById('dash' + p.charAt(0).toUpperCase() + p.slice(1));
        if (panel) panel.style.display = (p === dashCurrentTab) ? 'block' : 'none';
      });

      if (dashCurrentTab === 'export') {
        renderExportTab();
      }
      if (dashCurrentTab === 'settings') {
        renderSettingsTab();
      }
    });
  });
}

function setupDashboardSearch() {
  var search = document.getElementById('dashSearch');
  if (search) {
    search.addEventListener('input', function () {
      dashSearchQuery = search.value;
      renderHistoryTab();
    });
  }

  var filterCat = document.getElementById('dashFilterCat');
  if (filterCat) {
    filterCat.addEventListener('change', function () {
      dashFilterCategory = filterCat.value;
      renderHistoryTab();
    });
  }

  var filterSort = document.getElementById('dashFilterSort');
  if (filterSort) {
    filterSort.addEventListener('change', function () {
      dashFilterSort = filterSort.value;
      renderHistoryTab();
    });
  }
}

function getSelectedExportItems() {
  var preview = document.getElementById('exportPreview');
  if (!preview) return [];
  var items = [];
  var checkboxes = preview.querySelectorAll('.export-checkbox:checked');
  checkboxes.forEach(function (cb) {
    var label = cb.closest('.export-item');
    if (!label) return;
    var idx = parseInt(label.getAttribute('data-index'));
    // Rebuild items list from dashHistory
    var allItems = [];
    dashHistory.forEach(function (entry) {
      var titles = typeof entry.titles === 'string' ? JSON.parse(entry.titles) : (entry.titles || []);
      titles.forEach(function (t) {
        var titleText = typeof t === 'string' ? t : t.title;
        var score = typeof t === 'object' ? t.score : '';
        if (titleText) {
          allItems.push({
            title: titleText,
            score: score,
            keyword: entry.keyword || '',
            category: (entry.categories || []).join('; '),
            genre: entry.genre || '',
            style: entry.style || '',
            date: entry.created_at || ''
          });
        }
      });
    });
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
      items.forEach(function (item) {
        rows.push([
          csvEscape(item.title),
          item.score,
          csvEscape(item.keyword),
          csvEscape(item.category),
          csvEscape(item.genre),
          csvEscape(item.style),
          csvEscape(item.date)
        ]);
      });
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
        navigator.clipboard.writeText(text).then(function () {
          copySel.textContent = 'Copied!';
          setTimeout(function () { copySel.textContent = 'Copy Selected'; }, 2000);
        });
      } else {
        // Fallback
        var ta = document.createElement('textarea');
        ta.value = text;
        document.body.appendChild(ta);
        ta.select();
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
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

// Populate category filter dropdown with all categories
function populateDashFilters() {
  var filterCat = document.getElementById('dashFilterCat');
  if (!filterCat) return;
  // Keep the "All categories" option, add the rest
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

// --- Projects ---
function setupProjects() {
  var createBtn = document.getElementById('createProjectBtn');
  var nameInput = document.getElementById('newProjectName');

  if (createBtn && nameInput) {
    createBtn.addEventListener('click', function () {
      // Pro-only feature
      if (!isPro) { showUpgradeModal(); return; }
      if (!authToken) { alert('Please sign in to create projects.'); return; }

      var name = nameInput.value.trim();
      if (!name) { nameInput.focus(); return; }

      createBtn.textContent = 'Creating...';
      createBtn.disabled = true;

      fetch('/.netlify/functions/usage', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'Authorization': 'Bearer ' + authToken },
        body: JSON.stringify({
          action: 'create_project',
          name: name
        })
      })
        .then(function (r) { return r.json(); })
        .then(function (data) {
          createBtn.textContent = 'Create Project';
          createBtn.disabled = false;
          if (data.error) {
            alert(data.error);
          } else {
            nameInput.value = '';
            // Reload dashboard to show new project
            loadDashboard();
          }
        })
        .catch(function () {
          createBtn.textContent = 'Create Project';
          createBtn.disabled = false;
          alert('Could not create project. Please try again.');
        });
    });

    // Enter key on input also creates
    nameInput.addEventListener('keydown', function (e) {
      if (e.key === 'Enter') { createBtn.click(); }
    });
  }
}

// Delete a project
function deleteProject(projId) {
  if (!confirm('Delete this project? Titles assigned to it will be unassigned but not deleted.')) return;

  fetch('/.netlify/functions/usage', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', 'Authorization': 'Bearer ' + authToken },
    body: JSON.stringify({
      action: 'delete_project',
      projectId: projId
    })
  })
    .then(function (r) { return r.json(); })
    .then(function (data) {
      if (!data.error) { loadDashboard(); }
    })
    .catch(function () {});
}

// Add a title to a project
function addTitleToProject(titleText, sourceKeyword, score, projId) {
  if (!authToken) return;
  fetch('/.netlify/functions/usage', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', 'Authorization': 'Bearer ' + authToken },
    body: JSON.stringify({
      action: 'add_to_project',
      projectId: projId,
      title: titleText,
      keyword: sourceKeyword || '',
      score: score
    })
  }).catch(function () {});
}

// Show project picker dropdown
function showProjectPicker(titleText, sourceKeyword, score, anchorBtn) {
  if (dashProjects.length === 0) {
    alert('No projects yet. Create one on the Dashboard first.');
    return;
  }
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
      dropdown.textContent = '✓ Added!';
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
      if (d && !ev.target.closest('.proj-add-btn')) { d.remove(); }
      document.removeEventListener('click', closeDrop);
    });
  }, 10);
}

// Expose openAuthModal globally (for inline onclick handlers)
window.openAuthModal = openAuthModal;
window.showUpgradeModal = showUpgradeModal;
window.generateDesktopLicense = generateDesktopLicense;
window.deactivateLicense = deactivateLicense;
window.copyLicenseKey = copyLicenseKey;

// Expose verify subscription for Settings inline button
window.verifySubscriptionAndRefresh = function () {
  var btn = document.querySelector('.settings-card-highlight .btn-ghost-dark');
  if (btn) { btn.textContent = 'Checking...'; btn.disabled = true; }
  verifySubscription().then(function (pro) {
    if (btn) { btn.textContent = 'Check Subscription'; btn.disabled = false; }
    if (pro) {
      alert('Your Pro account is active! Refreshing...');
      location.reload();
    } else {
      alert('Your account is still on Free. If you just paid, it may take a moment. Try again in 30 seconds, or contact support.');
    }
  });
};
window.deleteProject = deleteProject;

// Show a native-looking breakdown popup instead of alert
function showBreakdownPopup(htmlContent) {
  var overlay = document.createElement('div');
  overlay.className = 'modal-overlay';
  overlay.style.display = 'flex';
  overlay.style.alignItems = 'center';
  overlay.style.justifyContent = 'center';

  var popup = document.createElement('div');
  popup.className = 'modal';
  popup.style.maxWidth = '380px';
  popup.style.maxHeight = '80vh';
  popup.style.overflowY = 'auto';
  popup.style.padding = '24px';

  var title = document.createElement('h3');
  title.textContent = 'Title Breakdown';
  title.style.marginBottom = '12px';
  popup.appendChild(title);

  var content = document.createElement('div');
  content.innerHTML = htmlContent;
  popup.appendChild(content);

  var closeBtn = document.createElement('button');
  closeBtn.className = 'btn btn-primary btn-full';
  closeBtn.textContent = 'Close';
  closeBtn.style.marginTop = '16px';
  closeBtn.addEventListener('click', function () { overlay.remove(); });
  popup.appendChild(closeBtn);

  overlay.appendChild(popup);
  overlay.addEventListener('click', function (e) { if (e.target === overlay) overlay.remove(); });
  document.body.appendChild(overlay);
}
