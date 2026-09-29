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

const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;
if (!reduced) {
  setInterval(() => {
    i = (i + 1) % bars.length;
    show();
  }, 2400);
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
