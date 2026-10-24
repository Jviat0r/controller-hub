// The live test dialog's DOM. It builds a skeleton once from the layout the
// engine hands back, then only writes values into it, so a frame thirty
// times a second never rebuilds anything or steals focus.

import { esc } from "./state.js";
import {
  agreementText, coverageSummary, dpadName, driftTag, driftText, gameDot, hwDot, notPressed, slotText,
} from "./tester.js";

let layout = null;
let refs = null;

const el = (id) => document.getElementById(id);

function chips(names) {
  return names.map((n, i) => `<span class="chip" data-chip="${i}"><b class="mark">○</b>${esc(n)}</span>`).join("");
}

function column(title, sub, stickLabels, trigLabels, names, showDpad) {
  return `<section class="tcol">
    <h3>${esc(title)}</h3>
    <p class="muted">${esc(sub)}</p>
    <p class="tblank" hidden>Nothing arriving.</p>
    <div class="tbody">
      <div class="sticks">
        ${stickLabels.map((n, i) => `<div class="stickbox">
          <span class="tlabel">${esc(n)}</span>
          <div class="stick"><i class="stick-dot" data-dot="${i}"></i></div>
          <code data-sval="${i}">—</code>
        </div>`).join("")}
      </div>
      <div class="triggers">
        ${trigLabels.map((n, i) => `<div class="trig">
          <span class="tlabel">${esc(n)}</span>
          <div class="bar"><i data-bar="${i}"></i></div>
          <code data-tval="${i}">—</code>
        </div>`).join("")}
      </div>
      ${showDpad ? `<p class="tline">D-pad: <code data-dpad>—</code></p>` : ""}
      <div class="chips">${chips(names)}</div>
    </div>
  </section>`;
}

function grab(root) {
  return {
    root,
    blank: root.querySelector(".tblank"),
    body: root.querySelector(".tbody"),
    dots: [...root.querySelectorAll("[data-dot]")],
    svals: [...root.querySelectorAll("[data-sval]")],
    bars: [...root.querySelectorAll("[data-bar]")],
    tvals: [...root.querySelectorAll("[data-tval]")],
    dpad: root.querySelector("[data-dpad]"),
    chips: [...root.querySelectorAll("[data-chip]")],
  };
}

export function buildTester(l, controllerName) {
  layout = l;
  el("test-title").textContent = `Testing ${controllerName}`;
  const cols = [];
  if (layout.hardware_available) {
    cols.push(column(
      "From the controller",
      "Read straight off the controller over its own cable or link.",
      ["Left stick", "Right stick"], ["L2", "R2"], layout.hardware, true,
    ));
  }
  cols.push(column(
    "What games see",
    "Read back from Windows the same way a game reads it.",
    ["Left stick", "Right stick"], ["LT", "RT"], layout.game, false,
  ));
  const host = el("test-cols");
  host.innerHTML = cols.join("");
  host.dataset.single = layout.hardware_available ? "0" : "1";
  const sections = [...host.querySelectorAll(".tcol")];
  refs = {
    hw: layout.hardware_available ? grab(sections[0]) : null,
    game: grab(sections[layout.hardware_available ? 1 : 0]),
  };

  el("test-drift").innerHTML = ["Left stick", "Right stick"]
    .map((n, i) => `<li><span><b>${esc(n)}</b><br><span data-drift="${i}" class="muted">Waiting.</span></span><span class="tag" data-drifttag="${i}">no reading</span></li>`)
    .join("");

  const coverage = [];
  if (layout.hardware_available) {
    coverage.push(`<section><h4>On the controller <span class="muted" data-cov-sum="hw"></span></h4>
      <div class="chips">${chips(layout.hardware)}</div>
      <p class="muted" data-cov-missing="hw"></p></section>`);
  }
  coverage.push(`<section><h4>Reaching games <span class="muted" data-cov-sum="game"></span></h4>
    <div class="chips">${chips(layout.game)}</div>
    <p class="muted" data-cov-missing="game"></p></section>`);
  el("test-coverage").innerHTML = coverage.join("");
  const covSections = [...el("test-coverage").querySelectorAll("section")];
  refs.covHw = layout.hardware_available ? [...covSections[0].querySelectorAll("[data-chip]")] : [];
  refs.covGame = [...covSections[layout.hardware_available ? 1 : 0].querySelectorAll("[data-chip]")];

  el("test-verdict").textContent = "";
  el("test-slot").textContent = "";
  el("test-gone").hidden = true;
}

function setChips(list, states) {
  list.forEach((c, i) => {
    const on = states && states[i] === true;
    if (c.dataset.on === (on ? "1" : "0")) return;
    c.dataset.on = on ? "1" : "0";
    c.querySelector(".mark").textContent = on ? "●" : "○";
  });
}

function setSeen(list, states) {
  list.forEach((c, i) => {
    const on = states && states[i] === true;
    if (c.dataset.on === (on ? "1" : "0")) return;
    c.dataset.on = on ? "1" : "0";
    c.querySelector(".mark").textContent = on ? "✓" : "·";
  });
}

function place(dot, pos) {
  dot.style.left = `${pos.left}%`;
  dot.style.top = `${pos.top}%`;
}

function setColumn(r, view, dotFn, trigMax) {
  if (!r) return;
  const has = !!view;
  r.blank.hidden = has;
  r.body.hidden = !has;
  if (!has) return;
  place(r.dots[0], dotFn(view.lx, view.ly));
  place(r.dots[1], dotFn(view.rx, view.ry));
  r.svals[0].textContent = `${view.lx}, ${view.ly}`;
  r.svals[1].textContent = `${view.rx}, ${view.ry}`;
  const lt = view.l2 ?? view.lt ?? 0;
  const rt = view.r2 ?? view.rt ?? 0;
  r.bars[0].style.width = `${(lt / trigMax) * 100}%`;
  r.bars[1].style.width = `${(rt / trigMax) * 100}%`;
  r.tvals[0].textContent = String(lt);
  r.tvals[1].textContent = String(rt);
  if (r.dpad) r.dpad.textContent = dpadName(view.dpad);
  setChips(r.chips, view.buttons);
}

export function updateTester(frame) {
  if (!refs || !layout) return;
  setColumn(refs.hw, frame.hardware, hwDot, 255);
  setColumn(refs.game, frame.game, gameDot, 255);

  const verdict = el("test-verdict");
  verdict.textContent = agreementText(frame);
  verdict.dataset.state = frame.agreement;
  el("test-slot").textContent = slotText(frame.slot);

  for (const [i, d] of [frame.left, frame.right].entries()) {
    el("test-drift").querySelector(`[data-drift="${i}"]`).textContent = driftText(d);
    el("test-drift").querySelector(`[data-drifttag="${i}"]`).textContent = driftTag(d);
  }

  const cov = frame.coverage || {};
  if (refs.covHw.length) {
    setSeen(refs.covHw, cov.hardware_seen);
    el("test-coverage").querySelector('[data-cov-sum="hw"]').textContent = coverageSummary(cov.hardware_seen);
    const missing = notPressed(layout.hardware, cov.hardware_seen);
    el("test-coverage").querySelector('[data-cov-missing="hw"]').textContent =
      missing.length ? `Still to try: ${missing.join(", ")}.` : "Every button was seen.";
  }
  setSeen(refs.covGame, cov.game_seen);
  el("test-coverage").querySelector('[data-cov-sum="game"]').textContent = coverageSummary(cov.game_seen);
  const missingGame = notPressed(layout.game, cov.game_seen);
  el("test-coverage").querySelector('[data-cov-missing="game"]').textContent =
    missingGame.length ? `Still to try: ${missingGame.join(", ")}.` : "Every button reached games.";
}

export function testerGone(gone) {
  el("test-gone").hidden = !gone;
}

export function clearTester() {
  layout = null;
  refs = null;
}
