/* ============================================================================
   Walkaway × Muse Blitz — marketing surface behaviour
   Progressive enhancement only: the page is fully readable without this file.
   ========================================================================= */
(function () {
  'use strict';

  /* --- 1. Sticky nav ---------------------------------------------------- */

  var nav = document.getElementById('nav');
  if (nav) {
    var onScroll = function () {
      nav.classList.toggle('stuck', window.scrollY > 24);
    };
    onScroll();
    window.addEventListener('scroll', onScroll, { passive: true });
  }

  /* --- 2. Reveal on scroll ----------------------------------------------- */

  var reveals = Array.prototype.slice.call(document.querySelectorAll('.reveal'));
  if (reveals.length) {
    if ('IntersectionObserver' in window) {
      var io = new IntersectionObserver(function (entries) {
        entries.forEach(function (entry) {
          if (entry.isIntersecting) {
            entry.target.classList.add('in');
            io.unobserve(entry.target);
          }
        });
      }, { rootMargin: '0px 0px -12% 0px', threshold: 0.08 });

      reveals.forEach(function (el, i) {
        el.style.transitionDelay = (i % 4) * 60 + 'ms';
        io.observe(el);
      });
    } else {
      reveals.forEach(function (el) { el.classList.add('in'); });
    }
  }

  /* --- 3. Acts accordion -------------------------------------------------- */

  var actList = document.getElementById('actList');
  var scenes = Array.prototype.slice.call(document.querySelectorAll('.act-scene'));

  function openAct(index) {
    if (!actList) return;
    Array.prototype.forEach.call(actList.querySelectorAll('.act-item'), function (item) {
      var isTarget = item.getAttribute('data-act') === String(index);
      item.classList.toggle('open', isTarget);
      var btn = item.querySelector('.act-btn');
      if (btn) btn.setAttribute('aria-expanded', isTarget ? 'true' : 'false');
    });
    scenes.forEach(function (scene) {
      scene.classList.toggle('active', scene.getAttribute('data-scene') === String(index));
    });
  }

  if (actList) {
    actList.addEventListener('click', function (event) {
      var btn = event.target.closest('.act-btn');
      if (!btn) return;
      var item = btn.closest('.act-item');
      var index = item.getAttribute('data-act');
      if (item.classList.contains('open')) {
        item.classList.remove('open');
        btn.setAttribute('aria-expanded', 'false');
        scenes.forEach(function (scene) { scene.classList.remove('active'); });
      } else {
        openAct(index);
      }
    });
  }

  /* --- 4. FAQ accordion --------------------------------------------------- */

  Array.prototype.forEach.call(document.querySelectorAll('.faq-item'), function (item) {
    var btn = item.querySelector('.faq-q');
    if (!btn) return;
    btn.addEventListener('click', function () {
      var willOpen = !item.classList.contains('open');
      Array.prototype.forEach.call(document.querySelectorAll('.faq-item'), function (other) {
        other.classList.remove('open');
        var otherBtn = other.querySelector('.faq-q');
        if (otherBtn) otherBtn.setAttribute('aria-expanded', 'false');
      });
      if (willOpen) {
        item.classList.add('open');
        btn.setAttribute('aria-expanded', 'true');
      }
    });
  });

  /* --- 5. Ad slot filter -------------------------------------------------- */

  var tabs = Array.prototype.slice.call(document.querySelectorAll('.slot-tab'));
  var adItems = Array.prototype.slice.call(document.querySelectorAll('.ad-item'));

  if (tabs.length && adItems.length) {
    tabs.forEach(function (tab) {
      tab.addEventListener('click', function () {
        var slot = tab.getAttribute('data-slot');
        tabs.forEach(function (t) { t.classList.toggle('active', t === tab); });
        adItems.forEach(function (item) {
          var show = slot === 'all' || item.getAttribute('data-slot') === slot;
          item.hidden = !show;
        });
      });
    });
  }

  /* --- 6. Banner export ---------------------------------------------------
     Serialises the live inline SVG (so the page's real fonts are used) and
     embeds the woff2 faces as data URIs, producing a self-contained SVG
     that opens correctly on any machine.
  ------------------------------------------------------------------------ */

  var FONT_FILES = [
    { file: 'assets/fonts/DrukWide-Heavy.woff2', family: 'Druk Wide', weight: 900 },
    { file: 'assets/fonts/DrukWide-Medium.woff2', family: 'Druk Wide', weight: 500 },
    { file: 'assets/fonts/GTPressuraMono-Bold.woff2', family: 'GT Pressura Mono', weight: 700 },
    { file: 'assets/fonts/DepartureMono-Regular.woff2', family: 'Departure Mono', weight: 400 }
  ];

  var fontCssPromise = null;

  function toBase64(buffer) {
    var bytes = new Uint8Array(buffer);
    var binary = '';
    var chunk = 0x8000;
    for (var i = 0; i < bytes.length; i += chunk) {
      binary += String.fromCharCode.apply(null, bytes.subarray(i, i + chunk));
    }
    return btoa(binary);
  }

  function loadFontCss() {
    if (fontCssPromise) return fontCssPromise;
    fontCssPromise = Promise.all(FONT_FILES.map(function (font) {
      return fetch(font.file)
        .then(function (res) { return res.arrayBuffer(); })
        .then(function (buf) {
          return "@font-face{font-family:'" + font.family + "';font-style:normal;font-weight:" +
            font.weight + ";src:url(data:font/woff2;base64," + toBase64(buf) + ") format('woff2');}";
        })
        .catch(function () { return ''; });
    })).then(function (parts) {
      return parts.join('\n');
    });
    return fontCssPromise;
  }

  function downloadBanner(svg, filename) {
    loadFontCss().then(function (fontCss) {
      var clone = svg.cloneNode(true);
      clone.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
      clone.setAttribute('width', svg.getAttribute('viewBox').split(' ')[2]);
      clone.setAttribute('height', svg.getAttribute('viewBox').split(' ')[3]);

      var style = document.createElementNS('http://www.w3.org/2000/svg', 'style');
      style.textContent = fontCss;
      clone.insertBefore(style, clone.firstChild);

      var title = document.createElementNS('http://www.w3.org/2000/svg', 'title');
      title.textContent = filename.replace(/-/g, ' ');
      clone.insertBefore(title, clone.firstChild);

      var source = new XMLSerializer().serializeToString(clone);
      var blob = new Blob(['<?xml version="1.0" encoding="UTF-8"?>\n' + source], {
        type: 'image/svg+xml;charset=utf-8'
      });
      var url = URL.createObjectURL(blob);
      var a = document.createElement('a');
      a.href = url;
      a.download = filename + '.svg';
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      setTimeout(function () { URL.revokeObjectURL(url); }, 2000);
    });
  }

  Array.prototype.forEach.call(document.querySelectorAll('.ad-dl'), function (button) {
    button.addEventListener('click', function () {
      var item = button.closest('.ad-item');
      var svg = item ? item.querySelector('svg.banner-svg') : null;
      if (!svg) return;
      var name = button.getAttribute('data-dl') || 'walkaway-banner';
      button.textContent = 'Preparing…';
      downloadBanner(svg, name);
      setTimeout(function () { button.textContent = 'Download SVG'; }, 2200);
    });
  });

  /* --- 7. Footer year ----------------------------------------------------- */

  var year = document.querySelector('.footer-base span');
  if (year) {
    year.textContent = 'Walkaway × Muse Blitz — 2026';
  }
})();
