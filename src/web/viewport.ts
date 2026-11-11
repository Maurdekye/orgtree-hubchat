// An iPhone's keyboard covers the bottom of the page instead of resizing it,
// and the phone layout keeps its composer at the bottom. While the keyboard
// is up this module tells the stylesheet (web.css) which part of the page is
// still visible: html[data-keyboard], with --visible-top and
// --visible-height. (Android's Chrome resizes the page itself:
// interactive-widget=resizes-content in web/index.html.)

let following = false;

export function followKeyboard(): void {
  const vv = window.visualViewport;
  if (!vv || following) return;
  following = true;
  const html = document.documentElement;
  let frame = 0;
  const apply = () => {
    frame = 0;
    // a keyboard, not a pinch zoom or a toolbar: the visible part is much shorter than the page
    const up = vv.scale <= 1.01 && html.clientHeight - vv.height > 120;
    if (up) {
      html.style.setProperty("--visible-top", Math.max(0, Math.round(vv.offsetTop)) + "px");
      html.style.setProperty("--visible-height", Math.round(vv.height) + "px");
      html.dataset.keyboard = "";
    } else if ("keyboard" in html.dataset) {
      delete html.dataset.keyboard;
      html.style.removeProperty("--visible-top");
      html.style.removeProperty("--visible-height");
    }
  };
  const later = () => { frame ||= requestAnimationFrame(apply); };
  vv.addEventListener("resize", later);
  vv.addEventListener("scroll", later);
}
