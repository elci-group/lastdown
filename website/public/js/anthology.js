(function () {
  var links = Array.prototype.slice.call(document.querySelectorAll(".spine nav a"));
  var path = location.pathname.replace(/\/index\.html$/, "/").replace(/\.html$/, "");
  if (path.length > 1 && path.endsWith("/")) {
    path = path.slice(0, -1);
  }
  links.forEach(function (link) {
    var href = link.getAttribute("href");
    if (href === path) {
      link.setAttribute("aria-current", "page");
    }
  });

  document.addEventListener("keydown", function (event) {
    if (event.altKey || event.metaKey || event.ctrlKey) {
      return;
    }
    var tag = event.target && event.target.tagName;
    if (tag === "INPUT" || tag === "TEXTAREA") {
      return;
    }
    var current = -1;
    links.forEach(function (link, index) {
      if (link.getAttribute("aria-current") === "page") {
        current = index;
      }
    });
    if (current < 0) {
      return;
    }
    var next = current;
    if (event.key === "ArrowRight" || event.key === "j") {
      next = Math.min(links.length - 1, current + 1);
    } else if (event.key === "ArrowLeft" || event.key === "k") {
      next = Math.max(0, current - 1);
    } else {
      return;
    }
    if (next !== current) {
      location.href = links[next].href;
    }
  });
})();
