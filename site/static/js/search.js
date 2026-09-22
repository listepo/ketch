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
