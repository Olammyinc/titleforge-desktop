/* ============================================
   TitleForge Desktop — Dashboard Page Logic
   Loads after app.js on dashboard.html
   Desktop: no auth, direct invoke() to SQLite
   ============================================ */

document.addEventListener('DOMContentLoaded', function () {
  // Desktop Pro — no auth check needed
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

  // Setup dashboard tabs
  setupDashboardTabs();

  // Setup dashboard search/filter/sort
  setupDashboardSearch();

  // Setup export buttons
  setupExportButtons();

  // Setup projects
  setupProjects();

  // Load dashboard data and render
  loadDashboardData();
});
