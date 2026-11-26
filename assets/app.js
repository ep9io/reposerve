/* reposerve: the one script every page loads.
 *
 * Every control works without this file: the theme and the reading mode are
 * links, the file tree is a list of details elements, and a form searches the
 * files. This file makes those controls instant and adds the two things a
 * server cannot do: a fuzzy file palette and a find bar. It holds no state that
 * the next request needs.
 */
(function () {
  "use strict";

  var THEME_COOKIE = "reposerve_theme";
  var READING_COOKIE = "reposerve_reading";
  var root = document.documentElement;

  function setCookie(name, value) {
    document.cookie =
      name + "=" + value + "; Path=/; SameSite=Lax; Max-Age=31536000";
  }

  function prefersDark() {
    return (
      window.matchMedia &&
      window.matchMedia("(prefers-color-scheme: dark)").matches
    );
  }

  function isDark() {
    var theme = root.getAttribute("data-theme");
    return theme === "dark" || (theme === "auto" && prefersDark());
  }

  function isTyping(target) {
    if (!target) {
      return false;
    }
    var tag = target.tagName;
    return (
      tag === "INPUT" ||
      tag === "TEXTAREA" ||
      tag === "SELECT" ||
      target.isContentEditable
    );
  }

  /* Theme ---------------------------------------------------------------- */

  function applyTheme(theme) {
    root.setAttribute("data-theme", theme);
    setCookie(THEME_COOKIE, theme);
    var options = document.querySelectorAll(".theme-option");
    for (var index = 0; index < options.length; index += 1) {
      var option = options[index];
      var chosen = option.getAttribute("href").indexOf("/theme/" + theme) === 0;
      if (chosen) {
        option.setAttribute("aria-current", "true");
      } else {
        option.removeAttribute("aria-current");
      }
    }
    var summary = document.querySelector(".theme-menu summary");
    if (summary) {
      var label = theme === "dark" ? "Dark" : theme === "light" ? "Light" : "System";
      summary.setAttribute("aria-label", "Theme: " + label);
      summary.setAttribute("title", "Theme: " + label);
    }
    renderDiagrams();
  }

  function cycleTheme() {
    var order = ["auto", "light", "dark"];
    var current = root.getAttribute("data-theme") || "auto";
    var next = order[(order.indexOf(current) + 1) % order.length];
    applyTheme(next);
  }

  /* Reading mode --------------------------------------------------------- */

  function setReading(on) {
    root.setAttribute("data-reading", on ? "on" : "off");
    setCookie(READING_COOKIE, on ? "on" : "off");
    var button = document.getElementById("reading-toggle");
    if (button) {
      button.setAttribute("aria-pressed", String(on));
    }
    if (on) {
      var nav = document.getElementById("nav-toggle");
      if (nav) {
        nav.checked = false;
      }
    }
  }

  function toggleReading() {
    setReading(root.getAttribute("data-reading") !== "on");
  }

  /* Copy ----------------------------------------------------------------- */

  function legacyCopy(text) {
    var area = document.createElement("textarea");
    area.value = text;
    area.setAttribute("readonly", "");
    area.style.position = "fixed";
    area.style.top = "-1000px";
    area.style.opacity = "0";
    document.body.appendChild(area);
    area.select();
    area.setSelectionRange(0, text.length);
    var ok = false;
    try {
      ok = document.execCommand("copy");
    } catch (error) {
      ok = false;
    }
    document.body.removeChild(area);
    return ok;
  }

  function copyText(text) {
    if (navigator.clipboard && window.isSecureContext) {
      return navigator.clipboard.writeText(text).then(
        function () {
          return true;
        },
        function () {
          return legacyCopy(text);
        }
      );
    }
    return Promise.resolve(legacyCopy(text));
  }

  function copyCode(button) {
    var box = button.closest(".copyable");
    var code = box ? box.querySelector("code") : null;
    if (!code) {
      return;
    }
    copyText(code.textContent || "").then(function (ok) {
      button.textContent = ok ? "Copied" : "Failed";
      button.classList.add("shown");
      window.setTimeout(function () {
        button.textContent = "Copy";
        button.classList.remove("shown");
      }, 1400);
    });
  }

  /* Diagrams and math ---------------------------------------------------- */

  function mermaidTheme() {
    return isDark() ? "dark" : "default";
  }

  function renderDiagrams() {
    if (!window.mermaid) {
      return;
    }
    var nodes = document.querySelectorAll("pre.mermaid");
    if (!nodes.length) {
      return;
    }
    for (var index = 0; index < nodes.length; index += 1) {
      var node = nodes[index];
      if (!node.dataset.code) {
        node.dataset.code = node.textContent;
      }
      node.removeAttribute("data-processed");
      node.textContent = node.dataset.code;
      var frame = node.closest(".diagram");
      if (frame) {
        frame.removeAttribute("data-failed");
      }
    }
    try {
      window.mermaid.initialize({
        startOnLoad: false,
        securityLevel: "strict",
        theme: mermaidTheme(),
        fontFamily: "inherit"
      });
      window.mermaid.run({ nodes: Array.prototype.slice.call(nodes) }).catch(markDiagramsFailed);
    } catch (error) {
      markDiagramsFailed();
    }
  }

  function markDiagramsFailed() {
    var frames = document.querySelectorAll(".diagram");
    for (var index = 0; index < frames.length; index += 1) {
      frames[index].setAttribute("data-failed", "yes");
    }
  }

  function renderMath() {
    if (!window.katex) {
      return;
    }
    var nodes = document.querySelectorAll(".math:not([data-rendered])");
    for (var index = 0; index < nodes.length; index += 1) {
      var node = nodes[index];
      try {
        window.katex.render(node.textContent || "", node, {
          displayMode: node.classList.contains("math-display"),
          throwOnError: false,
          strict: "ignore"
        });
        node.setAttribute("data-rendered", "yes");
      } catch (error) {
        /* The source stays visible, which is the honest fallback. */
      }
    }
  }

  function startLibraries() {
    renderDiagrams();
    renderMath();
  }

  /* File palette --------------------------------------------------------- */

  var paths = null;

  function openPalette() {
    var dialog = document.getElementById("palette");
    if (!dialog) {
      return;
    }
    if (!dialog.open) {
      dialog.showModal();
    }
    var input = document.getElementById("palette-input");
    if (input) {
      input.value = "";
      input.focus();
    }
    drawPalette("");
    if (paths !== null) {
      return;
    }
    fetch("/__index.json")
      .then(function (response) {
        return response.ok ? response.json() : [];
      })
      .then(function (list) {
        paths = Array.isArray(list) ? list : [];
        drawPalette(input ? input.value : "");
      })
      .catch(function () {
        paths = [];
      });
  }

  function score(path, needle) {
    var lower = path.toLowerCase();
    if (!needle) {
      return 1;
    }
    var name = lower.split("/").pop();
    if (name.indexOf(needle) === 0) {
      return 1000 - path.length;
    }
    if (name.indexOf(needle) !== -1) {
      return 500 - path.length;
    }
    if (lower.indexOf(needle) !== -1) {
      return 100 - path.length;
    }
    // Every character in order, which catches an abbreviation.
    var at = 0;
    for (var index = 0; index < needle.length; index += 1) {
      at = lower.indexOf(needle.charAt(index), at);
      if (at === -1) {
        return 0;
      }
      at += 1;
    }
    return 10;
  }

  function drawPalette(rawQuery) {
    var list = document.getElementById("palette-results");
    if (!list) {
      return;
    }
    list.textContent = "";
    var needle = rawQuery.trim().toLowerCase();
    var matches = [];
    var source = paths || [];
    for (var index = 0; index < source.length; index += 1) {
      var value = score(source[index], needle);
      if (value > 0) {
        matches.push({ path: source[index], score: value });
      }
    }
    matches.sort(function (left, right) {
      return right.score - left.score;
    });
    if (!matches.length) {
      var none = document.createElement("li");
      none.className = "none";
      none.textContent = paths === null ? "Loading files…" : "No file matches.";
      list.appendChild(none);
      return;
    }
    matches.slice(0, 40).forEach(function (match, position) {
      var item = document.createElement("li");
      item.setAttribute("role", "option");
      item.dataset.path = match.path;
      item.setAttribute("aria-selected", position === 0 ? "true" : "false");
      if (needle) {
        var at = match.path.toLowerCase().indexOf(needle);
        if (at === -1) {
          item.textContent = match.path;
        } else {
          item.appendChild(document.createTextNode(match.path.slice(0, at)));
          var mark = document.createElement("mark");
          mark.textContent = match.path.slice(at, at + needle.length);
          item.appendChild(mark);
          item.appendChild(document.createTextNode(match.path.slice(at + needle.length)));
        }
      } else {
        item.textContent = match.path;
      }
      list.appendChild(item);
    });
  }

  function paletteMove(step) {
    var list = document.getElementById("palette-results");
    if (!list) {
      return;
    }
    var items = Array.prototype.slice.call(list.querySelectorAll("li[data-path]"));
    if (!items.length) {
      return;
    }
    var current = 0;
    for (var index = 0; index < items.length; index += 1) {
      if (items[index].getAttribute("aria-selected") === "true") {
        current = index;
      }
      items[index].setAttribute("aria-selected", "false");
    }
    var next = (current + step + items.length) % items.length;
    items[next].setAttribute("aria-selected", "true");
    items[next].scrollIntoView({ block: "nearest" });
  }

  function paletteOpen() {
    var item = document.querySelector("#palette-results li[aria-selected='true']");
    if (item && item.dataset.path) {
      window.location.href = "/" + item.dataset.path.split("/").map(encodeURIComponent).join("/");
    }
  }

  /* Find in page --------------------------------------------------------- */

  var findRanges = [];
  var findIndex = 0;
  var findSupported = Boolean(window.Highlight && window.CSS && CSS.highlights);

  function findBar() {
    return document.getElementById("findbar");
  }

  function findInput() {
    return document.getElementById("find-input");
  }

  function openFind() {
    var bar = findBar();
    if (!bar) {
      return;
    }
    bar.hidden = false;
    var input = findInput();
    if (input) {
      input.focus();
      input.select();
    }
    if (input && input.value) {
      runFind(input.value);
    }
  }

  function closeFind() {
    var bar = findBar();
    if (bar) {
      bar.hidden = true;
    }
    findRanges = [];
    findIndex = 0;
    paintFind();
  }

  function matchesInPage(needle) {
    var main = document.getElementById("main");
    var ranges = [];
    if (!main || !needle) {
      return ranges;
    }
    var lowered = needle.toLowerCase();
    var walker = document.createTreeWalker(main, NodeFilter.SHOW_TEXT, {
      acceptNode: function (node) {
        var parent = node.parentElement;
        if (!parent || parent.closest("script, style, .findbar")) {
          return NodeFilter.FILTER_REJECT;
        }
        return node.nodeValue.trim() ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT;
      }
    });
    var node = walker.nextNode();
    while (node) {
      var text = node.nodeValue;
      var haystack = text.toLowerCase();
      var from = 0;
      var at = haystack.indexOf(lowered, from);
      while (at !== -1) {
        var range = document.createRange();
        range.setStart(node, at);
        range.setEnd(node, at + needle.length);
        ranges.push(range);
        if (ranges.length >= 500) {
          return ranges;
        }
        from = at + needle.length;
        at = haystack.indexOf(lowered, from);
      }
      node = walker.nextNode();
    }
    return ranges;
  }

  function paintFind() {
    var count = document.getElementById("find-count");
    if (count) {
      count.textContent = findRanges.length
        ? findIndex + 1 + " of " + findRanges.length
        : "no match";
    }
    if (!findSupported) {
      return;
    }
    try {
      if (findRanges.length) {
        CSS.highlights.set("reposerve-find", new Highlight(...findRanges));
        var current = findRanges[findIndex];
        if (current) {
          CSS.highlights.set("reposerve-find-current", new Highlight(current));
        }
      } else {
        CSS.highlights.delete("reposerve-find");
        CSS.highlights.delete("reposerve-find-current");
      }
    } catch (error) {
      /* A browser without the full API keeps the count and the scrolling. */
    }
  }

  function scrollToFind() {
    var range = findRanges[findIndex];
    if (!range) {
      return;
    }
    var rect = range.getBoundingClientRect();
    window.scrollBy({
      top: rect.top - window.innerHeight / 3,
      behavior: "smooth"
    });
  }

  function runFind(query) {
    findRanges = matchesInPage(query.trim());
    findIndex = 0;
    paintFind();
    scrollToFind();
  }

  function findStep(direction) {
    if (direction === "close") {
      closeFind();
      return;
    }
    if (!findRanges.length) {
      return;
    }
    if (direction === "next") {
      findIndex = (findIndex + 1) % findRanges.length;
    } else {
      findIndex = (findIndex - 1 + findRanges.length) % findRanges.length;
    }
    paintFind();
    scrollToFind();
  }

  /* Live reload ---------------------------------------------------------- */

  function currentPath() {
    try {
      return decodeURIComponent(window.location.pathname).replace(/^\/+/, "");
    } catch (error) {
      return window.location.pathname.replace(/^\/+/, "");
    }
  }

  function showChip() {
    var chip = document.getElementById("update-chip");
    if (chip) {
      chip.hidden = false;
    }
  }

  function softReload() {
    try {
      sessionStorage.setItem("reposerve-scroll", String(window.scrollY));
    } catch (error) {
      /* Private mode may refuse storage; the reload still happens. */
    }
    window.location.reload();
  }

  function watchUpdates() {
    if (!window.EventSource) {
      return;
    }
    var source = new EventSource("/__reload");
    source.onmessage = function (event) {
      var message = null;
      try {
        message = JSON.parse(event.data);
      } catch (error) {
        softReload();
        return;
      }
      var changed = (message && message.paths) || [];
      if (!changed.length) {
        softReload();
        return;
      }
      var page = currentPath();
      var affected = page === "";
      if (!affected) {
        for (var index = 0; index < changed.length; index += 1) {
          var path = changed[index];
          if (
            path === page ||
            page.indexOf(path + "/") === 0 ||
            path.indexOf(page + "/") === 0
          ) {
            affected = true;
            break;
          }
        }
      }
      if (affected) {
        softReload();
      } else {
        showChip();
      }
    };
  }

  function restoreScroll() {
    var saved = null;
    try {
      saved = sessionStorage.getItem("reposerve-scroll");
    } catch (error) {
      return;
    }
    if (saved === null) {
      return;
    }
    try {
      sessionStorage.removeItem("reposerve-scroll");
    } catch (error) {
      /* Nothing to clear. */
    }
    window.requestAnimationFrame(function () {
      window.scrollTo(0, Number(saved) || 0);
    });
  }

  /* Contents ------------------------------------------------------------- */

  function watchContents() {
    var links = Array.prototype.slice.call(
      document.querySelectorAll('.toc a[href^="#"]')
    );
    if (!links.length) {
      return;
    }
    var targets = [];
    links.forEach(function (link) {
      var id = link.getAttribute("href").slice(1);
      var element = document.getElementById(id);
      if (element) {
        targets.push({ link: link, element: element });
      }
    });
    if (!targets.length) {
      return;
    }
    var timer = null;
    var update = function () {
      var best = null;
      targets.forEach(function (target) {
        var top = target.element.getBoundingClientRect().top;
        if (top <= 140) {
          best = target;
        }
      });
      targets.forEach(function (target) {
        if (target === best) {
          target.link.setAttribute("aria-current", "true");
        } else {
          target.link.removeAttribute("aria-current");
        }
      });
    };
    window.addEventListener(
      "scroll",
      function () {
        if (timer) {
          return;
        }
        timer = window.setTimeout(function () {
          timer = null;
          update();
        }, 120);
      },
      { passive: true }
    );
    /* A page opened at `#section` scrolls after this script runs, and a jump
     * between headings may not raise a scroll event. Both cases need a pass. */
    window.addEventListener("load", update);
    window.addEventListener("hashchange", update);
    update();
  }

  /* Wiring --------------------------------------------------------------- */

  document.addEventListener("click", function (event) {
    var theme = event.target.closest(".theme-option");
    if (theme) {
      event.preventDefault();
      var mode = theme.getAttribute("href").split("/theme/")[1].split("?")[0];
      applyTheme(mode);
      var menu = theme.closest("details");
      if (menu) {
        menu.open = false;
      }
      return;
    }
    if (event.target.closest("[data-copy]")) {
      copyCode(event.target.closest("[data-copy]"));
      return;
    }
    if (event.target.closest("#reading-toggle")) {
      toggleReading();
      return;
    }
    if (event.target.closest("[data-reload]")) {
      softReload();
      return;
    }
    if (event.target.closest("[data-find]")) {
      findStep(event.target.closest("[data-find]").dataset.find);
      return;
    }
    var option = event.target.closest("#palette-results li[data-path]");
    if (option) {
      window.location.href =
        "/" + option.dataset.path.split("/").map(encodeURIComponent).join("/");
      return;
    }
    var navLink = event.target.closest(".nav a");
    if (navLink) {
      var toggle = document.getElementById("nav-toggle");
      if (toggle) {
        toggle.checked = false;
      }
    }
    var tocLink = event.target.closest('.toc a[href^="#"]');
    if (tocLink && window.matchMedia("(max-width: 77.99rem)").matches) {
      var toc = tocLink.closest("details");
      if (toc) {
        toc.open = false;
      }
    }
  });

  document.addEventListener("keydown", function (event) {
    var typing = isTyping(event.target);
    var command = event.metaKey || event.ctrlKey;

    if (command && event.key.toLowerCase() === "k") {
      event.preventDefault();
      openPalette();
      return;
    }
    if (command && event.key.toLowerCase() === "f") {
      event.preventDefault();
      openFind();
      return;
    }
    if (event.key === "Escape") {
      var bar = findBar();
      if (bar && !bar.hidden) {
        closeFind();
        return;
      }
    }
    if (typing || command || event.altKey) {
      return;
    }
    if (event.key === "/") {
      event.preventDefault();
      openFind();
    } else if (event.key === "t") {
      cycleTheme();
    } else if (event.key === "f") {
      toggleReading();
    }
  });

  function wire() {
    var navToggle = document.getElementById("nav-toggle");
    var navButton = document.querySelector(".nav-button");
    if (navToggle && navButton) {
      navButton.setAttribute("aria-expanded", String(navToggle.checked));
      navToggle.addEventListener("change", function () {
        navButton.setAttribute("aria-expanded", String(navToggle.checked));
      });
    }

    var readingButton = document.getElementById("reading-toggle");
    if (readingButton) {
      readingButton.setAttribute(
        "aria-pressed",
        String(root.getAttribute("data-reading") === "on")
      );
    }

    var input = findInput();
    if (input) {
      input.addEventListener("input", function () {
        runFind(input.value);
      });
      input.addEventListener("keydown", function (event) {
        if (event.key === "Enter") {
          event.preventDefault();
          findStep(event.shiftKey ? "previous" : "next");
        }
      });
    }

    var paletteInput = document.getElementById("palette-input");
    if (paletteInput) {
      paletteInput.addEventListener("input", function () {
        drawPalette(paletteInput.value);
      });
      paletteInput.addEventListener("keydown", function (event) {
        if (event.key === "ArrowDown") {
          event.preventDefault();
          paletteMove(1);
        } else if (event.key === "ArrowUp") {
          event.preventDefault();
          paletteMove(-1);
        } else if (event.key === "Enter") {
          event.preventDefault();
          paletteOpen();
        }
      });
    }

    var libraries = document.querySelectorAll("script[data-lib]");
    for (var index = 0; index < libraries.length; index += 1) {
      libraries[index].addEventListener("load", startLibraries);
    }

    if (window.matchMedia) {
      var query = window.matchMedia("(prefers-color-scheme: dark)");
      var listener = function () {
        renderDiagrams();
      };
      if (query.addEventListener) {
        query.addEventListener("change", listener);
      } else if (query.addListener) {
        query.addListener(listener);
      }
    }

    restoreScroll();
    startLibraries();
    watchContents();
    watchUpdates();

    var contents = document.querySelector(".toc details");
    if (contents && window.matchMedia("(min-width: 78rem)").matches) {
      contents.open = true;
    }
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", wire);
  } else {
    wire();
  }
})();
