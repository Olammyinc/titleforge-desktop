/* ============================================
   TitleForge — Dashboard Page Logic
   Loads after app.js on dashboard.html
   ============================================ */

document.addEventListener('DOMContentLoaded', function () {

  // Check auth from localStorage (Supabase session may not restore if CDN is blocked)
  var authData = null;
  try {
    var stored = localStorage.getItem('titleforge_auth');
    if (stored) authData = JSON.parse(stored);
  } catch (e) {}

  if (!authData || !authData.isLoggedIn || !authData.token) {
    window.location.href = 'index.html';
    return;
  }

  // Restore auth state from localStorage if Supabase didn't load
  if (!authToken && authData.token) {
    authToken = authData.token;
    isLoggedIn = true;
    isGuest = false;
  }

  // Check if redirected from Stripe after payment
  var urlParams = new URLSearchParams(window.location.search);
  if (urlParams.get('session_id')) {
    // Show upgrading message
    var container = document.querySelector('.dashboard-container');
    if (container) {
      container.innerHTML = '<div style="text-align:center;padding:60px 20px;"><div class="spinner"></div><p style="margin-top:16px;font-size:16px;color:var(--text);">Verifying your payment and upgrading account...</p></div>';
    }
    verifySubscription().then(function (pro) {
      if (pro) {
        window.location.href = window.location.pathname + '#pro-activated';
      } else {
        window.location.href = window.location.pathname + '#payment-verification-failed';
      }
      location.reload();
    });
    return;
  }

  // Update nav
  updateNavUserUI(authData.email);

  // Wire sign out button
  var logoutBtn = document.getElementById('navLogout');
  if (logoutBtn) logoutBtn.addEventListener('click', function () {
    handleLogout();
    window.location.href = 'index.html';
  });

  // Populate category filter with all categories
  var filterCat = document.getElementById('dashFilterCat');
  if (filterCat) {
    filterCat.innerHTML = '<option value="">All categories</option>';
    ALL_CATEGORIES.forEach(function (cat) {
      var opt = document.createElement('option');
      opt.value = cat.label;
      opt.textContent = cat.label;
      filterCat.appendChild(opt);
    });
  }

  // Load usage from server for latest data
  loadUsageFromServer();

  // Load dashboard data and render
  loadDashboard();

  // Setup dashboard tabs
  setupDashboardTabs();

  // Setup dashboard search/filter/sort
  setupDashboardSearch();

  // Setup export buttons
  setupExportButtons();

  // Setup projects
  setupProjects();
});
