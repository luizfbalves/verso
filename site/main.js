const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;

// Título cantado: o marca-texto passa palavra por palavra, como a letra no app, e fica só no <em>.
const h1 = document.querySelector("h1");
function splitWords(root) {
  for (const node of [...root.childNodes]) {
    if (node.nodeType !== Node.TEXT_NODE) {
      splitWords(node);
      continue;
    }
    const frag = document.createDocumentFragment();
    for (const part of node.textContent.split(/(\s+)/)) {
      if (!part) continue;
      if (/^\s+$/.test(part)) {
        frag.append(part);
      } else {
        const w = document.createElement("span");
        w.className = "w";
        w.textContent = part;
        frag.append(w);
      }
    }
    node.replaceWith(frag);
  }
}
splitWords(h1);
if (!reduced) {
  let at = 500;
  for (const w of h1.querySelectorAll(".w")) {
    const t = 140 + w.textContent.length * 30;
    w.style.setProperty("--d", `${at}ms`);
    w.style.setProperty("--t", `${t}ms`);
    at += t + 60;
  }
  requestAnimationFrame(() => requestAnimationFrame(() => h1.classList.add("sing")));
  setTimeout(() => h1.classList.add("settled"), at + 700);
}

// Demo: o selo do disco repete a mecânica do overlay — a linha atual centralizada e destacada.
// Larguras em % do selo, como linhas de letra de tamanhos diferentes.
const WIDTHS = [46, 62, 77, 40, 58, 70, 50, 66];

const track = document.getElementById("label-track");
const label = track.parentElement.parentElement;
const bars = WIDTHS.map((w) => {
  const el = document.createElement("div");
  el.className = "label-bar";
  el.style.width = `${w}%`;
  track.append(el);
  return el;
});

let i = 2;
function show() {
  bars.forEach((el, k) => el.classList.toggle("cur", k === i));
  // Centraliza pela posição final da barra (alturas mudam com transição, então calcula pelo CSS alvo).
  const cs = getComputedStyle(label);
  const unit = parseFloat(cs.width) / 100;
  const h = (k) => (k === i ? 11.5 : 6.2) * unit;
  let top = 0;
  for (let k = 0; k < i; k++) top += h(k) + 8 * unit;
  const ty = label.clientHeight / 2 - (top + h(i) / 2);
  track.style.transform = `translateY(${ty}px)`;
}
show();
new ResizeObserver(show).observe(label);

// Toca-discos: a agulha desce, o disco ganha velocidade até 33⅓ rpm e só então a letra anda.
// Pausar levanta a agulha e o disco para aos poucos.
const deck = document.getElementById("deck");
const arm = deck.querySelector(".arm");
const toggle = document.getElementById("deck-toggle");

if (reduced) {
  deck.classList.add("playing");
} else {
  const spin = deck.querySelector(".disc-grooves").animate(
    [{ transform: "rotate(0turn)" }, { transform: "rotate(1turn)" }],
    { duration: 1800, iterations: Infinity },
  );
  spin.playbackRate = 0;
  spin.pause();

  let rampId = 0;
  function ramp(to, ms, done) {
    const id = ++rampId;
    const from = spin.playbackRate;
    const t0 = performance.now();
    const step = (now) => {
      if (id !== rampId) return;
      const k = Math.min(1, (now - t0) / ms);
      spin.playbackRate = from + (to - from) * k * k * (3 - 2 * k);
      if (k < 1) requestAnimationFrame(step);
      else done?.();
    };
    requestAnimationFrame(step);
  }

  let playing = false;
  let lyrics = 0;
  function render() {
    toggle.textContent = playing ? toggle.dataset.on : toggle.dataset.off;
    toggle.dataset.state = playing ? "on" : "off";
  }
  function play() {
    playing = true;
    deck.classList.add("playing");
    render();
  }
  function stop() {
    playing = false;
    deck.classList.remove("playing");
    clearInterval(lyrics);
    lyrics = 0;
    ramp(0, 1400, () => spin.pause());
    render();
  }
  arm.addEventListener("transitionend", () => {
    if (!playing || lyrics) return;
    spin.play();
    ramp(1, 900);
    lyrics = setInterval(() => {
      i = (i + 1) % bars.length;
      show();
    }, 2400);
  });

  let touched = false;
  toggle.hidden = false;
  toggle.addEventListener("click", () => {
    touched = true;
    playing ? stop() : play();
  });
  render();

  // Só dá a partida quando o disco aparece (no celular ele fica abaixo da dobra).
  const seen = new IntersectionObserver(([e]) => {
    if (!e.isIntersecting) return;
    seen.disconnect();
    setTimeout(() => touched || play(), 250);
  }, { threshold: 0.5 });
  seen.observe(deck);

  // Tracklist: a faixa que cruza o meio da tela vira a "linha atual".
  const rows = new IntersectionObserver(
    (entries) => entries.forEach((e) => e.target.classList.toggle("cur", e.isIntersecting)),
    { rootMargin: "-47% 0px -47% 0px" },
  );
  document.querySelectorAll(".tracklist li").forEach((li) => rows.observe(li));
}

// Downloads: aponta os botões direto para os instaladores da última release.
const REPO = "luizfbalves/verso";
const PICK = {
  "mac-arm": (n) => /aarch64\.dmg$/i.test(n),
  "mac-intel": (n) => /x64\.dmg$/i.test(n),
  win: (n) => /x64-setup\.exe$/i.test(n) || /\.msi$/i.test(n),
};

fetch(`https://api.github.com/repos/${REPO}/releases/latest`)
  .then((r) => (r.ok ? r.json() : Promise.reject(r.status)))
  .then((rel) => {
    const v = rel.tag_name?.replace(/^v/, "");
    if (v) document.querySelectorAll("[data-version]").forEach((el) => (el.textContent = v));
    document.querySelectorAll("[data-dl]").forEach((a) => {
      const asset = rel.assets.find((x) => PICK[a.dataset.dl](x.name));
      if (asset) a.href = asset.browser_download_url;
    });
  })
  .catch(() => {
    // Sem release publicada ou API indisponível: os links já apontam para a página de Releases.
  });
