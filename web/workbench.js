"use strict";

const get = (id) => document.getElementById(id);
const svgNs = "http://www.w3.org/2000/svg";
const positions = {
  1: [88, 136], 2: [288, 69], 3: [282, 224], 4: [436, 151], 5: [118, 254],
};
const layerNames = {road: "Road", bike: "Bike", walk: "Walking", bus: "Bus", rail: "Rail"};
const layerColors = {road: "#606b76", bike: "#9e682a", walk: "#237358", bus: "#534397", rail: "#a94a61"};
const layerOffsets = {road: -8, bike: -16, walk: 8, bus: 18, rail: 24};
const colors = {best: "#20715d", mid: "#e7aa48", slow: "#a96049", none: "#fff"};
const names = {"parking-walk": "Parking approach", "direct-door": "Clinic approach", "junction-walk": "Junction approach"};
const activeLayers = new Set(Object.keys(layerNames));
let fixture;
let selection = [];
let previewVersion = 0;
let studyVersion = 0;
let studyProject = null;
let comparisonVersion = 0;
let acceptedPlan = null;
let submitting = false;
let pendingRun = null;
let completed = null;
let displayed = null;
let activeComparison = null;
let playback = null;

function create(tag, content, className) {
  const node = document.createElement(tag);
  if (content !== undefined) node.textContent = content;
  if (className) node.className = className;
  return node;
}

function graphic(tag, attributes) {
  const node = document.createElementNS(svgNs, tag);
  for (const [key, value] of Object.entries(attributes)) node.setAttribute(key, value);
  return node;
}

function status(id, message, error = false) {
  get(id).textContent = message;
  get(id).classList.toggle("error", error);
}

async function api(route, fields = {}, write = false) {
  // Route names and keys are fixed. The Rust adapter validates every field.
  const query = new URLSearchParams(fields).toString().replaceAll("%3A", ":").replaceAll("%2C", ",");
  const response = await fetch("/api/e17/v1/" + route + (query ? "?" + query : ""), {
    method: write ? "POST" : "GET",
    headers: write ? {"X-Carfree-Workbench": "1"} : {},
  });
  const value = await response.json();
  if (!response.ok) {
    const error = new Error(value.error);
    error.status = response.status;
    throw error;
  }
  return value;
}

function values(form) {
  return Object.fromEntries(new FormData(form));
}

function option(value, label) {
  const node = create("option", label);
  node.value = value;
  return node;
}

function boundary(rings) {
  let west = Infinity, east = -Infinity, south = Infinity, north = -Infinity;
  for (const ring of rings) {
    for (const [lon, lat] of ring) {
      west = Math.min(west, lon);
      east = Math.max(east, lon);
      south = Math.min(south, lat);
      north = Math.max(north, lat);
    }
  }
  const scale = Math.min(466 / (east - west), 256 / (north - south));
  const left = (520 - (east - west) * scale) / 2;
  const top = (310 - (north - south) * scale) / 2;
  const path = rings.map((ring) => ring.map(([lon, lat], index) => {
    return (index ? "L" : "M") + (left + (lon - west) * scale) + "," + (top + (north - lat) * scale);
  }).join(" ") + " Z").join(" ");
  get("boundary").replaceChildren(graphic("path", {
    d: path, fill: "#99cdb0", stroke: "#176654", "stroke-width": 2, "fill-rule": "evenodd",
  }));
}

async function study(initialize = false) {
  const revision = ++studyVersion;
  const fields = values(get("study-form"));
  studyProject = null;
  get("save-study").hidden = true;
  if (!fields.area || !fields.date) return;
  status("study-status", initialize ? "Initializing boundary…" : "Checking dataset coverage…");
  try {
    const result = await api(initialize ? "initialize" : "study", fields, initialize);
    if (revision !== studyVersion) return;
    boundary(result.rings);
    get("coverage").replaceChildren(...result.coverage.flatMap(([kind, state, source, version, detail]) => [
      create("dt", kind), create("dd", state + " · " + detail + (source === "Unknown" ? "" : " · " + source + " (" + version + ")")),
    ]));
    if (result.projectTsv) {
      studyProject = {tsv: result.projectTsv, filename: fields.area.replace(":", "-") + ".tsv"};
      get("save-study").hidden = false;
    }
    const message = result.projectTsv ? "Boundary initialized. " : "Boundary preview. ";
    status("study-status", message + (result.missing.length ? "Model inputs missing: " + result.missing.join(", ") + "." : "All required inputs reported available."));
  } catch (error) {
    if (revision !== studyVersion) return;
    get("boundary").replaceChildren();
    get("coverage").replaceChildren();
    status("study-status", error.message, true);
  }
}

function saveStudy() {
  if (!studyProject) return;
  const url = URL.createObjectURL(new Blob([studyProject.tsv], {type: "text/tab-separated-values"}));
  const download = create("a");
  download.href = url;
  download.download = studyProject.filename;
  download.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function check(group, label, checked, change) {
  const field = create("label");
  const input = create("input");
  input.type = "checkbox";
  input.checked = checked;
  input.addEventListener("change", () => change(input.checked));
  field.append(input, document.createTextNode(label));
  group.append(field);
  return input;
}

function swatch(className, text) {
  const item = create("li");
  const color = create("span", undefined, "swatch " + className);
  color.setAttribute("aria-hidden", "true");
  item.append(color, document.createTextNode(text));
  return item;
}

function edgePath(from, to, bend) {
  // Canonical endpoint order keeps reciprocal links on the same schematic path.
  const [a, b] = from < to ? [from, to] : [to, from];
  const [x, y] = positions[a], [u, v] = positions[b];
  const magnitude = Math.hypot(u - x, v - y);
  const midX = (x + u) / 2 - (v - y) / magnitude * bend;
  const midY = (y + v) / 2 + (u - x) / magnitude * bend;
  return "M" + x + "," + y + " Q" + midX + "," + midY + " " + u + "," + v;
}

function labelFor(node) {
  return node === 4 ? "4 · Clinic" : "Node " + node;
}

function networkMap(element, links, overlay = null) {
  element.replaceChildren();
  for (const [id, from, to, mode, access] of links) {
    if (!activeLayers.has(mode)) continue;
    const added = overlay && !overlay.baselineLinks.has(id);
    element.append(graphic("path", {
      d: edgePath(from, to, layerOffsets[mode] + (added ? 17 : 0)), fill: "none",
      stroke: added ? "#166555" : layerColors[mode], "stroke-width": added ? 5 : 3.5,
      "stroke-dasharray": access === "Unknown" ? "2 5" : added ? "9 4" : "none",
      ...(added ? {"data-phase-link": id} : {}),
    }));
  }
  if (!overlay) {
    fixture.projects.forEach(([id, from, to], index) => {
      const selected = selection.includes(index);
      const d = edgePath(from, to, -34);
      element.append(graphic("path", {
        d, fill: "none", stroke: selected ? "#122a26" : "#526860",
        "stroke-width": selected ? 5 : 2.5, "stroke-dasharray": "8 4",
      }));
      const hit = graphic("path", {
        d, fill: "none", stroke: "transparent", "stroke-width": 24,
        class: "proposal", "aria-hidden": "true", "data-project": id,
      });
      hit.addEventListener("click", () => toggleProject(index, !selected));
      element.append(hit);
    });
  }
  const scores = new Map((overlay?.scores ?? []).map((row) => [row[0], row]));
  for (const node of fixture.nodes) {
    const [cx, cy] = positions[node];
    const origin = overlay?.origin === node;
    const fill = origin ? "#122a26" : scores.has(node) ? colors[band(scores.get(node), overlay.metric)] : "#fff";
    element.append(graphic("circle", {cx, cy, r: origin ? 10 : 8, fill, stroke: "#122a26", "stroke-width": 2}));
    const text = graphic("text", {x: cx + 11, y: cy - 11});
    text.textContent = labelFor(node);
    element.append(text);
  }
}

function drawScenario() {
  networkMap(get("scenario-map"), fixture.links);
  const legend = get("layer-legend");
  legend.replaceChildren();
  for (const mode of activeLayers) {
    const links = fixture.links.filter((link) => link[3] === mode);
    const unknown = links.filter((link) => link[4] === "Unknown").length;
    const text = links.length ? links.length + " modeled · " + unknown + " unknown access" : "absent in fixture";
    legend.append(swatch(mode, layerNames[mode] + " · " + text));
  }
}

function toggleProject(index, selected) {
  selection = selected ? [...selection, index] : selection.filter((item) => item !== index);
  invalidatePreview();
  refreshSelection();
}

function refreshSelection() {
  for (const [index, input] of get("projects").querySelectorAll("input").entries()) {
    const order = selection.indexOf(index);
    input.checked = order >= 0;
    input.nextSibling.textContent = projectLabel(fixture.projects[index][0]) + (order < 0 ? "" : " · Phase " + (order + 1));
  }
  drawScenario();
}

function projectLabel(id) {
  return names[id] ?? id;
}

function invalidatePreview() {
  ++previewVersion;
  acceptedPlan = null;
  get("run").disabled = true;
  get("costs").replaceChildren();
  status("plan-status", "");
}

function range(bounds) {
  return bounds[0] === bounds[2]
    ? bounds[1].toFixed(2)
    : bounds[1].toFixed(2) + " (" + bounds[0].toFixed(2) + "–" + bounds[2].toFixed(2) + ")";
}

async function previewPlan(event) {
  event.preventDefault();
  invalidatePreview();
  if (selection.length === 0 || selection.length > 2) {
    status("plan-status", "Select one or two candidate links.", true);
    return;
  }
  const revision = previewVersion;
  const fields = {...values(get("plan-form")), projects: selection.join(",")};
  status("plan-status", "Checking corridor, access and budgets…");
  try {
    const preview = await api("preview", fields);
    if (revision !== previewVersion) return;
    const costList = create("ul");
    for (const [id, capital, annual] of preview.costs) {
      costList.append(create("li", projectLabel(id) + " · " + range(capital) + " USD capital · " + range(annual) + " USD/year maintenance"));
    }
    const costs = get("costs");
    costs.replaceChildren(create("strong", "Modeled incremental costs · 2026 USD"), costList);
    costs.append(create("p", "Capital includes construction and disruption. These fixture lump sums do not vary with walkway width.", "caption"));
    if (preview.warning) {
      status("plan-status", preview.warning, true);
      return;
    }
    costs.append(create("p", "Final portfolio · " + range(preview.total[0]) + " USD capital · " + range(preview.total[1]) + " USD/year including opening commitments.", "caption"));
    status("plan-status", "The selected phases passed the fixture’s checks.");
    acceptedPlan = fields;
    get("run").disabled = submitting || pendingRun !== null;
  } catch (error) {
    if (revision === previewVersion) status("plan-status", error.message, true);
  }
}

async function trackRun(submit) {
  if (submitting || (submit && (!acceptedPlan || pendingRun !== null))) return;
  submitting = true;
  get("run").disabled = true;
  get("resume-run").hidden = true;
  try {
    if (submit) {
      status("run-status", "Submitting scenario…");
      const fields = acceptedPlan;
      const {id} = await api("runs", fields, true);
      pendingRun = id;
      showSubmittedPlan({...fields, projects: fields.projects.split(",").map(Number)});
      status("run-status", "Queued · run " + id);
    }
    await poll(pendingRun);
    pendingRun = null;
  } catch (error) {
    status("run-status", pendingRun === null ? error.message
      : "Could not check run " + pendingRun + " · " + error.message, true);
    if (error.status === 400) pendingRun = null;
  } finally {
    submitting = false;
    get("run").disabled = !acceptedPlan || pendingRun !== null;
    get("resume-run").hidden = pendingRun === null;
  }
}

function planSummary(plan) {
  const projects = plan.projects.map((index) => projectLabel(fixture.projects[index][0])).join(" → ");
  return projects + " · " + plan.width + " mm · budgets " + plan.capital
    + " USD capital and " + plan.annual + " USD/year (2026)";
}

function showSubmittedPlan(plan) {
  get("submitted-plan").textContent = "Submitted plan · " + planSummary(plan);
}

async function poll(id) {
  for (;;) {
    const {plan, state: result} = await api("run", {id});
    showSubmittedPlan(plan);
    if (result.status === "completed") {
      status("run-status", "Completed · run " + id);
      await showRun(id, plan, result.phases);
      return;
    }
    if (result.status === "failed") {
      status("run-status", "Failed · run " + id + " · " + result.error, true);
      return;
    }
    status("run-status", (result.status === "queued" ? "Queued" : "Running") + " · run " + id);
    await new Promise((resolve) => setTimeout(resolve, 350));
  }
}

async function showRun(id, plan, phases) {
  stopPlayback();
  completed = {id, phases};
  get("result-plan").textContent = "Run " + id + " · " + planSummary(plan);
  displayed = null;
  get("phase").replaceChildren(...phases.map((phase, i) => option(i, phaseLabel(phase, i))));
  get("phase").value = phases.length - 1;
  get("results-output").hidden = true;
  get("results-content").hidden = false;
  const form = get("comparison-form");
  if (activeComparison) {
    for (const [field, value] of Object.entries(activeComparison)) form.elements[field].value = value;
  }
  await showPhase(values(form));
}

function phaseLabel(phase, index) {
  return index ? "Phase " + index + " · " + projectLabel(phase.name) : "Baseline";
}

function band([, median, reached, samples], metric) {
  if (reached === 0) return "none";
  if (metric === "coverage") {
    if (reached === samples) return "best";
    if (reached * 2 >= samples) return "mid";
    return "slow";
  }
  if (median <= 15) return "best";
  if (median <= 30) return "mid";
  return "slow";
}

function drawComparison() {
  if (!displayed) return;
  const {phases, phase, before, after, fields, metric} = displayed;
  const origin = Number(fields.origin);
  const [x, y] = positions[origin];
  const viewBox = get("extent").value === "origin" ? [x - 140, y - 83.5, 280, 167].join(" ") : "0 0 520 310";
  const legend = metric === "time" ? ["≤15 s", "16–30 s", ">30 s"] : ["All departures", "At least half, below all", "Below half, above zero"];
  get("result-legend").replaceChildren(
    swatch("best", legend[0]), swatch("mid", legend[1]), swatch("slow", legend[2]), swatch("none", "No journey within limit"),
  );
  const baselineLinks = new Set(phases[0].links.map(([id]) => id));
  for (const [side, scores, snapshot] of [["before", before, phases[0]], ["after", after, phases[phase]]]) {
    const map = get(side + "-map");
    map.setAttribute("viewBox", viewBox);
    networkMap(map, snapshot.links, {scores, origin, metric, baselineLinks});
    const destinations = scores.map(([node, median, reached, samples]) => {
      const duration = median === null ? "no journey within limit" : median + " s median among reachable departures";
      return create("li", labelFor(node) + " · " + duration + " · " + reached + "/" + samples);
    });
    get(side + "-data").replaceChildren(...destinations);
  }
  const current = phases[phase];
  get("comparison-caption").textContent = (metric === "time" ? "Point isochrones at network nodes. " : "Share of departures reachable within the selected limit. ")
    + labelFor(origin) + " · " + fields.modes.replace("-", " + ") + " · " + fields.profile + " · departures " + fields.start + "–" + fields.end + " s · limit " + fields.seconds + " s";
  get("after-title").textContent = phaseLabel(current, phase);
  get("phase-column").textContent = phaseLabel(current, phase);
  drawMetrics(phases[0], current);
}

function drawMetrics(baseline, after) {
  const share = (value, total) => total > 0 ? (100 * value / total).toFixed(1) + "%" : "Unavailable";
  const rows = [
    ["Essential trips unserved · assignment", (p) => share(p.metrics.essentialUnserved, p.metrics.essentialTotal)],
    ["Essential trips with no feasible mode · reference", (p) => share(p.metrics.essentialNoMode, p.metrics.essentialReferenceTotal)],
    ["Essential trips requiring a car · reference", (p) => share(p.metrics.essentialCarRequired, p.metrics.essentialReferenceTotal)],
    ["Noncycling cohort unserved · weighted trips", (p) => p.metrics.noncyclingUnserved],
    ["Step-free cohort unserved · weighted trips", (p) => p.metrics.stepFreeUnserved],
    ["Assignment status", (p) => p.metrics.assignment],
    ["Capital · 2026 USD", (p) => range(p.budget[0])],
    ["Annual commitments · 2026 USD/year", (p) => range(p.budget[1])],
  ];
  get("phase-metrics").replaceChildren(...rows.map(([label, value]) => {
    const row = create("tr");
    const heading = create("th", label);
    heading.scope = "row";
    row.append(heading, create("td", value(baseline)), create("td", value(after)));
    return row;
  }));
}

async function showPhase(query = activeComparison ?? values(get("comparison-form"))) {
  const revision = ++comparisonVersion;
  const {id, phases} = completed;
  const phase = Number(get("phase").value);
  const {metric, ...fields} = query;
  phaseControls(phase, phases.length);
  status("results-status", "Routing matched departure samples…");
  try {
    const data = await api("comparison", {id, phase, ...fields});
    if (revision !== comparisonVersion) return;
    activeComparison = query;
    displayed = {...data, phases, phase, fields, metric};
    drawComparison();
    get("results-output").hidden = false;
    status("results-status", phaseLabel(phases[phase], phase) + " displayed for run " + id + ".");
  } catch (error) {
    if (revision !== comparisonVersion) return;
    status("results-status", error.message + (displayed ? " Maps still show the last successful comparison." : ""), true);
    stopPlayback();
    if (displayed) {
      get("phase").value = displayed.phase;
      phaseControls(displayed.phase, phases.length);
    }
  }
}

function phaseControls(phase, count) {
  get("phase-back").disabled = phase === 0;
  get("phase-next").disabled = phase === count - 1;
}

function stopPlayback() {
  playback = null;
  get("phase-play").textContent = "Play";
}

async function stepTo(index, manual = true) {
  if (manual) stopPlayback();
  get("phase").value = index;
  await showPhase();
}

async function play() {
  if (playback !== null) {
    stopPlayback();
    return;
  }
  const token = Symbol("playback");
  playback = token;
  get("phase-play").textContent = "Pause";
  for (let i = 0; i < completed.phases.length && playback === token; i++) {
    await stepTo(i, false);
    if (playback !== token) return;
    await new Promise((resolve) => setTimeout(resolve, 1200));
  }
  if (playback === token) stopPlayback();
}

get("study-form").addEventListener("submit", (event) => {
  event.preventDefault();
  study(true);
});
get("study-form").addEventListener("change", () => study());
get("save-study").addEventListener("click", saveStudy);
get("plan-form").addEventListener("submit", previewPlan);
get("plan-form").addEventListener("input", (event) => {
  if (event.target.type !== "checkbox") invalidatePreview();
});
get("run").addEventListener("click", () => trackRun(true));
get("resume-run").addEventListener("click", () => trackRun(false));
get("comparison-form").addEventListener("submit", (event) => {
  event.preventDefault();
  stopPlayback();
  showPhase(values(get("comparison-form")));
});
get("phase").addEventListener("change", () => stepTo(Number(get("phase").value)));
get("phase-back").addEventListener("click", () => stepTo(Number(get("phase").value) - 1));
get("phase-next").addEventListener("click", () => stepTo(Number(get("phase").value) + 1));
get("phase-play").addEventListener("click", play);
get("extent").addEventListener("change", drawComparison);

api("areas").then((areas) => {
  get("area").replaceChildren(...areas.map(([id, name]) => option(id, name)));
  study();
}).catch((error) => status("study-status", error.message, true));

api("network").then((data) => {
  fixture = data;
  get("provenance").textContent = [data.source, data.version, data.geography, data.date, data.license, data.uncertainty].join(" · ");
  for (const [mode, name] of Object.entries(layerNames)) {
    check(get("layers").querySelector(".layers"), name, true, (checked) => {
      if (checked) activeLayers.add(mode);
      else activeLayers.delete(mode);
      drawScenario();
      drawComparison();
    });
  }
  data.projects.forEach(([id], index) => check(get("projects"), projectLabel(id), false, (checked) => toggleProject(index, checked)));
  get("origin").replaceChildren(...data.nodes.map((node) => option(node, labelFor(node))));
  drawScenario();
}).catch((error) => status("plan-status", error.message, true));
