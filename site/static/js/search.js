// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

/*! ketch site search — mounts PagefindUI on #search once the UI script is ready. */
(function () {
  function init() {
    var el = document.getElementById("search");
    if (!el || typeof window.PagefindUI !== "function") return;
    new window.PagefindUI({
      element: "#search",
      showImages: false,
    });
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
