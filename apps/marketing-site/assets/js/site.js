/* ============================================================================
   Walkaway: marketing surface behaviour
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
      }, { rootMargin: '0px 0px -8% 0px', threshold: 0.05 });

      reveals.forEach(function (el) { io.observe(el); });
    } else {
      reveals.forEach(function (el) { el.classList.add('in'); });
    }
  }

  /* --- 3. Paid-placement filter ------------------------------------------ */

  var tabs = Array.prototype.slice.call(document.querySelectorAll('.slot-tab'));
  var placements = Array.prototype.slice.call(document.querySelectorAll('.banner-item'));

  if (tabs.length && placements.length) {
    var applySlot = function (slot) {
      tabs.forEach(function (t) {
        t.classList.toggle('active', t.getAttribute('data-slot') === slot);
        t.setAttribute('aria-pressed', t.getAttribute('data-slot') === slot ? 'true' : 'false');
      });
      placements.forEach(function (item) {
        item.hidden = !(slot === 'all' || item.getAttribute('data-slot') === slot);
      });
    };

    tabs.forEach(function (tab) {
      tab.setAttribute('aria-pressed', tab.classList.contains('active') ? 'true' : 'false');
      tab.addEventListener('click', function () {
        applySlot(tab.getAttribute('data-slot'));
      });
    });

    applySlot('all');
  }
})();
