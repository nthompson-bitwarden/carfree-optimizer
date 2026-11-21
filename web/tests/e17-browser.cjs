// E17-S01–S09 and E18-S01–S07 executable Chromium/WebKit acceptance against the Rust binary.
"use strict";
const assert = require("node:assert/strict");
const {spawn} = require("node:child_process");
const {once} = require("node:events");
const {mkdir, readFile} = require("node:fs/promises");
const path = require("node:path");
const playwright = require(process.env.E17_PLAYWRIGHT_MODULE ?? "playwright");

const output = path.resolve(process.env.E17_SCREENSHOTS ?? ".codex/run/e17");
const engine = process.env.E17_BROWSER ?? "chromium";
const binary = path.resolve(process.env.E17_BINARY ?? "target/debug/spatial-workbench");

async function run() {
  const child = spawn(binary, ["0"], {stdio: ["ignore", "pipe", "inherit"]});
  let browser;
  try {
    const [startup] = await Promise.race([
      once(child.stdout, "data"),
      once(child, "exit").then(([code]) => { throw new Error("Workbench exited with " + code); }),
    ]);
    const base = startup.toString().match(/http:\/\/127\.0\.0\.1:\d+/)[0];
    browser = await playwright[engine].launch();
    await mkdir(output, {recursive: true});
    const page = await browser.newPage({viewport: {width: 1280, height: 900}, reducedMotion: "reduce"});
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));
    const requests = [];
    page.on("request", (request) => requests.push(new URL(request.url()).pathname));
    await page.goto(base);

    // S08: the example is ready on the first visit, without city requests or setup.
    await page.getByRole("checkbox", {name: "Parking approach", exact: true}).waitFor();
    assert.equal(await page.getByRole("heading", {name: "Try an example scenario"}).isVisible(), true);
    assert.equal(await page.locator("#study").isVisible(), false);
    assert.equal(requests.includes("/api/e17/v1/areas"), false);
    assert.equal(await page.getByRole("button", {name: "Check proposal"}).isDisabled(), true);
    assert.equal(await page.getByRole("button", {name: "Run comparison"}).isDisabled(), true);
    await page.screenshot({path: path.join(output, engine + "-first-desktop.png")});
    const skip = page.getByRole("link", {name: "Skip to workbench"});
    if (engine === "webkit") {
      // Default WebKit keyboard settings tab through form controls only.
      await skip.focus();
    } else {
      await page.keyboard.press("Tab");
    }
    assert.equal(await page.evaluate(() => document.activeElement.textContent), "Skip to workbench");
    await page.keyboard.press("Enter");
    assert.equal(new URL(page.url()).hash, "#workbench");

    // S01/S08: independent Census view, keyboard entry and saved E02 file.
    await page.getByRole("link", {name: "Explore a city"}).press("Enter");
    await page.locator("#boundary path").waitFor();
    assert.equal(new URL(page.url()).hash, "#study");
    assert.equal(await page.locator("#scenario").isVisible(), false);
    assert.equal(await page.evaluate(() => document.activeElement.id), "study-heading");
    assert.equal(requests.includes("/api/e17/v1/runs"), false);
    await skip.press("Enter");
    assert.equal(new URL(page.url()).hash, "#study");
    assert.equal(await page.evaluate(() => document.activeElement.id), "workbench");
    assert.deepEqual(await page.locator("#area option").allTextContents(), [
      "Portland-Vancouver-Hillsboro, OR-WA Metro Area", "Portland city, ME", "Portland city, OR",
    ]);
    await page.getByLabel("City or metro").selectOption("cbsa:38900");
    await page.locator("#study-status").filter({hasText: /Boundary preview.*Model inputs missing/}).waitFor();
    await page.getByRole("button", {name: "Initialize boundary"}).press("Enter");
    await page.locator("#study-status").filter({hasText: "Boundary initialized."}).waitFor();
    const [download] = await Promise.all([
      page.waitForEvent("download"),
      page.getByRole("button", {name: "Download project snapshot"}).press("Enter"),
    ]);
    assert.equal(download.suggestedFilename(), "cbsa-38900.tsv");
    const tsv = await readFile(await download.path(), "utf8");
    assert.match(tsv, /^carfree-study-project-v1\n/);
    assert.match(tsv, /missing\tRoads,Transit,Population,Demand/);

    await page.getByRole("link", {name: "Example scenario", exact: true}).press("Enter");
    await page.getByRole("heading", {name: "Try an example scenario"}).waitFor();
    await page.getByText("Map layers, legend and data source", {exact: true}).press("Enter");

    // S02: actual layer visibility and an explicit absent-versus-unknown legend.
    await page.locator("#layer-legend").filter({hasText: "Rail · absent in fixture"}).waitFor();
    assert.match(await page.locator("#layer-legend").innerText(), /Road · \d+ modeled · 0 unknown access/);
    await page.getByRole("checkbox", {name: "Road", exact: true}).press("Space");
    assert.equal(await page.locator('#scenario-map path[stroke="#606b76"]').count(), 0);
    await page.getByRole("checkbox", {name: "Road", exact: true}).press("Space");

    // S03/S04: pointer selection has the same ordered state as keyboard selection.
    const proposal = page.locator('[data-project="parking-walk"]');
    await proposal.scrollIntoViewIfNeeded();
    const {x, y} = await proposal.evaluate((element) => {
      const point = element.getPointAtLength(element.getTotalLength() * 0.55);
      const projected = point.matrixTransform(element.getScreenCTM());
      return {x: projected.x, y: projected.y};
    });
    await page.mouse.click(x, y);
    const parking = page.getByRole("checkbox", {name: /Parking approach/});
    assert.equal(await parking.isChecked(), true);
    await page.getByRole("checkbox", {name: /Clinic approach/}).press("Space");
    assert.match(await page.locator("#selection-summary").innerText(), /Parking approach → Clinic approach/);
    assert.equal(await page.locator("#check-plan").evaluate((button) => button.classList.contains("primary")), true);
    await page.getByText("Edit width and budgets", {exact: true}).press("Enter");
    await page.getByLabel("Capital budget (2026 USD)").fill("1");
    assert.match(await page.locator("#budget-summary").innerText(), /1 USD capital/);
    await page.getByRole("button", {name: "Check proposal"}).press("Enter");
    await page.locator("#plan-status").filter({hasText: /budget.*then check again/i}).waitFor();
    assert.equal(await parking.isChecked(), true);
    assert.equal(await page.getByRole("button", {name: "Run comparison"}).isDisabled(), true);
    await page.getByLabel("Capital budget (2026 USD)").fill("200");
    await page.getByLabel("Walkway width (mm)").fill("8000");
    await page.getByRole("button", {name: "Check proposal"}).press("Enter");
    await page.locator("#plan-status").filter({hasText: "right-of-way"}).waitFor();
    assert.equal(await page.getByRole("button", {name: "Run comparison"}).isDisabled(), true);
    assert.match(await page.locator("#costs").innerText(), /Modeled incremental costs/);
    await page.getByLabel("Walkway width (mm)").fill("1900");
    await page.getByRole("button", {name: "Check proposal"}).press("Enter");
    await page.locator("#plan-status").filter({hasText: "passed the fixture"}).waitFor();

    // An edit invalidates a passed check without losing the selected walkways.
    await page.getByLabel("Walkway width (mm)").fill("2100");
    await page.locator("#plan-status").filter({hasText: "Settings changed. Check the proposal again"}).waitFor();
    assert.equal(await page.getByRole("button", {name: "Run comparison"}).isDisabled(), true);
    assert.equal(await parking.isChecked(), true);
    await page.getByLabel("Walkway width (mm)").fill("1900");
    await page.getByRole("button", {name: "Check proposal"}).press("Enter");
    await page.locator("#plan-status").filter({hasText: "passed the fixture"}).waitFor();
    assert.equal(await page.getByRole("button", {name: "Run comparison"}).evaluate((button) => button.classList.contains("primary")), true);

    // S05: interrupt a real status request, then resume the same successful job.
    await page.route("**/api/e17/v1/run?id=1", (route) => route.abort(), {times: 1});
    await page.getByRole("button", {name: "Run comparison"}).press("Enter");
    await page.locator("#run-status").filter({hasText: /Could not check run 1.*reconnect/}).waitFor();
    await page.locator("#resume-run:focus").waitFor();
    assert.equal(await page.getByRole("button", {name: "Run comparison"}).isDisabled(), true);
    await page.getByRole("button", {name: "Check run status"}).press("Enter");
    await page.locator("#run-status").filter({hasText: "Completed · run"}).waitFor();
    assert.match(await page.locator("#submitted-plan").innerText(), /Parking approach → Clinic approach · 1900 mm/);
    await page.locator("#results-output").waitFor();
    assert.equal(await page.evaluate(() => document.activeElement.id), "results-heading");
    const summaryBox = await page.locator("#reachability-summary").boundingBox();
    assert.ok(summaryBox.y >= 0 && summaryBox.y + summaryBox.height < 900);
    assert.equal(await page.locator("#reachability-summary").isVisible(), true);
    assert.match(await page.locator(".result-summary").ariaSnapshot(), /status:.*reachable/s);
    assert.match(await page.locator("#comparison-assumptions").innerText(), /From Node 1.*Step-free.*Phase 2.*30 s limit.*at least one/);
    assert.equal(await page.locator("#comparison-settings").evaluate((details) => details.open), false);
    await page.screenshot({path: path.join(output, engine + "-first-results.png")});
    await page.getByLabel("Walkway width (mm)").fill("2100");
    assert.match(await page.locator("#submitted-plan").innerText(), /1900 mm/);
    await page.getByLabel("Walkway width (mm)").fill("1900");
    await page.getByRole("button", {name: "Check proposal"}).press("Enter");
    await page.locator("#plan-status").filter({hasText: "passed the fixture"}).waitFor();

    // S06/S07: real matched map updates and a shared extent and legend.
    await page.locator("#results-content").waitFor();
    await page.getByText("Comparison filters and map view", {exact: true}).press("Enter");
    await page.getByLabel("Mobility profile").selectOption("standard");
    await page.getByRole("button", {name: "Update comparison"}).press("Enter");
    await page.locator("#comparison-assumptions").filter({hasText: "Standard walking"}).waitFor();
    await page.getByLabel("Mobility profile").selectOption("step-free");
    await page.getByLabel("Travel modes").selectOption("walk-bus");
    await page.getByLabel("Map metric").selectOption("coverage");
    await page.getByLabel("Last departure (s)").fill("14");
    await page.getByLabel("Travel-time limit (s)").fill("16");
    await page.getByRole("button", {name: "Update comparison"}).press("Enter");
    await page.locator("#comparison-caption").filter({hasText: "Share of departures reachable"}).waitFor();
    const before = page.locator("#before-map");
    const after = page.locator("#after-map");
    await page.getByLabel("Both map extents").selectOption("origin");
    const extent = await before.getAttribute("viewBox");
    assert.equal(await after.getAttribute("viewBox"), extent);
    assert.notEqual(extent, "0 0 520 310");
    assert.equal(await page.locator("#result-legend li").count(), 4);
    assert.notEqual(await page.locator("#before-data").innerText(), await page.locator("#after-data").innerText());

    assert.match(await page.locator("#reachability-summary").innerText(), /more reachable destination/);
    assert.match(await page.locator("#reachability-difference").innerText(), /Newly reachable/);
    await page.getByText("Phase outcomes and modeled trip groups", {exact: true}).press("Enter");

    // E18-S01/S02: numerical differences, denominators and cohort rows share the committed origin.
    assert.match(await page.locator("#phase-metrics").innerText(), /Essential trips with no car-free option in reference.*66.667.*0.*-66.667 pp/);
    assert.match(await page.locator("#phase-denominators").innerText(), /Assignment 6 → 6 · Reference 6 → 6/);
    assert.deepEqual(await page.locator("#report-groups th").allTextContents(), ["adult", "mobility-aid"]);
    assert.match(await page.locator("#report-groups").innerText(), /adult.*4.*Car 4 → Car 4.*4 → 0/);
    await page.locator("#origin").selectOption("2");
    await page.getByRole("button", {name: "Update comparison"}).press("Enter");
    await page.locator("#groups-caption").filter({hasText: "has no modeled trip groups"}).waitFor();
    assert.equal(await page.locator("#groups-scroll").isVisible(), false);
    assert.notEqual(await before.getAttribute("viewBox"), extent);
    await page.locator("#origin").selectOption("1");
    await page.getByRole("button", {name: "Update comparison"}).press("Enter");
    await page.locator("#report-groups th").first().waitFor();

    await page.getByText("Methodology and audit trail", {exact: true}).press("Enter");
    await page.getByText("Phase changes, sensitivity and cost trade-offs", {exact: true}).press("Enter");

    // E18-S03/S04/S05/S07: trace, phase deltas, executed sensitivities and machine-readable export.
    assert.match(await page.locator("#phase-trace").innerText(), /opening\/e14\/parking-walk\/e14\/direct-door/);
    assert.match(await page.locator("#report-calibration").innerText(), /Uncalibrated synthetic fixture/);
    assert.match(await page.locator("#report-uncertainty").innerText(), /Empirical uncertainty unavailable/);
    assert.match(await page.locator("#sensitivity-range").innerText(), /-66.667 pp to 0 pp across 4 of 4 matched tests/);
    assert.match(await page.locator("#phase-changes").innerText(), /Phase 2.*0 pp.*-66.667 pp/);
    await page.locator("#report-metric").selectOption("6");
    assert.match(await page.locator("#phase-changes").innerText(), /Phase 2.*\+40 2026 USD.*\+100 2026 USD/);
    assert.equal(await page.locator("#phase-chart svg").count(), 1);
    await page.locator("#report-metric").selectOption("1");

    async function downloadReport() {
      const [file] = await Promise.all([
        page.waitForEvent("download"),
        page.getByRole("button", {name: "Download full report (JSON)"}).press("Enter"),
      ]);
      assert.equal(file.suggestedFilename(), "carfree-run-1-report-v1.json");
      return JSON.parse(await readFile(await file.path(), "utf8"));
    }

    const report = await downloadReport();
    assert.equal(report.version, 1);
    assert.deepEqual(report.plan.projects, [0, 1]);
    assert.deepEqual(report.phases.map((p) => p.trace.projects), [[], ["parking-walk"], ["parking-walk", "direct-door"]]);
    for (const phase of report.phases) {
      assert.equal(phase.trace.seed, 42);
      assert.equal(phase.trace.models.simulationVersion, 1);
    }
    assert.equal(report.sensitivity.cases[0].phases[0].referenceEssentialTotal, 10);
    assert.equal(report.sensitivity.cases[1].walkRestriction, true);
    assert.notDeepEqual(report.sensitivity.cases[1].phases[0].groups, report.phases[0].groups);
    assert.equal(report.sensitivity.cases[2].waitMultiplier, 2);
    assert.match(report.methodology.limitations, /causal/);
    assert.match(report.phases[0].trace.source, /repository-authored synthetic/);

    // E18-S06: recompute dominance from all screened rows and prove priority is read-only.
    const alternatives = report.tradeoffs.alternatives;
    for (const row of alternatives) {
      assert.equal(row.frontier, !alternatives.some((other) =>
        other.capital <= row.capital && other.reward >= row.reward
        && (other.capital < row.capital || other.reward > row.reward)));
    }
    assert.equal(report.tradeoffs.rejected.length, 2);
    assert.match(await page.locator("#report-frontier th").first().innerText(), /approach/i);
    assert.equal(await page.locator("#report-frontier th").count(), alternatives.filter((row) => row.frontier).length);
    await page.locator("#report-priority").selectOption("capital");
    assert.equal(await page.locator("#report-frontier th").first().innerText(), "Baseline");
    assert.deepEqual(await downloadReport(), report);
    await page.screenshot({path: path.join(output, engine + "-report.png"), fullPage: true});

    // S09: reversible snapshots and timed sequential playback.
    const final = await page.locator("#after-data").innerText();
    await page.getByRole("button", {name: "Previous"}).press("Enter");
    await page.locator("#after-title").filter({hasText: "Phase 1"}).waitFor();
    await page.getByRole("button", {name: "Previous"}).press("Enter");
    await page.locator("#after-title").filter({hasText: "Baseline"}).waitFor();
    assert.equal(await page.locator("#before-data").innerText(), await page.locator("#after-data").innerText());
    assert.match(await page.locator("#reachability-summary").innerText(), /No change in the number of reachable destinations/);
    assert.equal(await page.locator("#reachability-difference").innerText(), "The same destinations are reachable.");
    await page.getByRole("button", {name: "Play", exact: true}).press("Enter");
    await page.getByRole("button", {name: "Pause", exact: true}).waitFor();
    await page.locator("#after-title").filter({hasText: "Phase 2"}).waitFor();
    assert.equal(await page.locator("#after-data").innerText(), final);
    await page.getByRole("button", {name: "Play", exact: true}).waitFor();

    // Rejected comparison drafts cannot replace the committed query or leak a
    // previous run's maps beneath a second run's phase controls.
    await page.getByLabel("Last departure (s)").fill("120");
    await page.getByLabel("Travel-time limit (s)").fill("180");
    await page.getByRole("button", {name: "Update comparison"}).press("Enter");
    await page.locator("#results-status").filter({hasText: "last successful comparison"}).waitFor();
    assert.equal(await page.locator("#after-data").innerText(), final);
    await page.getByRole("button", {name: "Previous"}).press("Enter");
    await page.locator("#results-status").filter({hasText: "Phase 1 · Parking approach displayed for run 1"}).waitFor();
    await page.getByRole("checkbox", {name: /Clinic approach/}).press("Space");
    await page.getByRole("checkbox", {name: /Junction approach/}).press("Space");
    await page.getByRole("button", {name: "Check proposal"}).press("Enter");
    await page.locator("#plan-status").filter({hasText: "passed the fixture"}).waitFor();
    await page.route("**/api/e17/v1/comparison?id=2&*", (route) => route.abort(), {times: 1});
    await page.getByRole("button", {name: "Run comparison"}).press("Enter");
    await page.locator("#results-status").filter({hasText: "The stored report is available below."}).waitFor();
    assert.equal(await page.locator("#results-output").isVisible(), false);
    assert.equal(await page.getByRole("button", {name: "Download full report (JSON)"}).isVisible(), true);
    await page.getByRole("button", {name: "Update comparison"}).press("Enter");
    await page.locator("#results-status").filter({hasText: "Phase 2 · Junction approach displayed for run 2"}).waitFor();
    assert.notEqual(await page.locator("#after-data").innerText(), final);
    assert.equal(await after.locator("[data-phase-link='102']").count(), 1);
    assert.equal(await before.locator("[data-phase-link]").count(), 0);
    await page.getByLabel("Both map extents").selectOption("all");
    await page.screenshot({path: path.join(output, engine + "-desktop.png"), fullPage: true});

    // S08: same controls and results fit a narrow viewport without page overflow.
    await page.setViewportSize({width: 375, height: 780});
    const pageWidth = await page.evaluate(() => document.documentElement.scrollWidth);
    if (pageWidth !== 375) {
      const overflow = await page.locator("body *").evaluateAll((elements) => ({
        innerWidth, clientWidth: document.documentElement.clientWidth,
        elements: elements.filter((element) => element.getBoundingClientRect().right > 376 ||
            element.clientWidth > 0 && element.scrollWidth > element.clientWidth + 1)
          .map((element) => [element.tagName, element.id, String(element.className), element.clientWidth,
            element.scrollWidth, Math.round(element.getBoundingClientRect().right)]).slice(0, 30),
      }));
      assert.equal(pageWidth, 375, "Mobile overflow: " + JSON.stringify(overflow));
    }
    await page.getByRole("button", {name: "Previous"}).press("Enter");
    await page.locator("#after-title").filter({hasText: "Phase 1"}).waitFor();
    await page.screenshot({path: path.join(output, engine + "-mobile.png"), fullPage: true});
    // Force an accepted job to fail its budget and temporarily interrupt its
    // first status read. Retry the same real job and inspect the failed renderer.
    await page.route("**/api/e17/v1/runs?*", (route) => {
      const url = new URL(route.request().url());
      url.searchParams.set("capital", "1");
      return route.continue({url: url.toString().replaceAll("%2C", ",")});
    }, {times: 1});
    await page.route("**/api/e17/v1/run?id=3", (route) => route.abort(), {times: 1});
    await page.getByRole("button", {name: "Run comparison"}).press("Enter");
    await page.locator("#run-status").filter({hasText: "Could not check run 3"}).waitFor();
    await page.getByRole("button", {name: "Check run status"}).press("Enter");
    await page.locator("#run-status").filter({hasText: /Failed · run 3.*budget/}).waitFor();
    await page.locator("#check-plan:focus").waitFor();
    assert.match(await page.locator("#submitted-plan").innerText(), /budgets 1 USD capital/);
    assert.match(await page.locator("#result-plan").innerText(), /Run 2/);
    await page.keyboard.press("Enter");
    await page.locator("#plan-status").filter({hasText: "passed the fixture"}).waitFor();
    // Fresh mobile first visit by keyboard. Option+Tab in WebKit includes
    // controls outside its default keyboard navigation preference.
    const mobile = await browser.newPage({viewport: {width: 375, height: 780}, reducedMotion: "reduce"});
    mobile.on("pageerror", (error) => errors.push(error.message));
    await mobile.goto(base);
    const mobileParking = mobile.getByRole("checkbox", {name: /Parking approach/});
    await mobileParking.waitFor();
    assert.equal(await mobile.evaluate(() => document.documentElement.scrollWidth), 375);
    await mobile.screenshot({path: path.join(output, engine + "-first-mobile.png")});
    await mobileParking.focus();
    await mobile.keyboard.press("Space");
    assert.equal(await mobileParking.isChecked(), true);
    const tabKey = engine === "webkit" ? "Alt+Tab" : "Tab";
    for (let tab = 0; tab < 5 && await mobile.evaluate(() => document.activeElement.id) !== "check-plan"; tab++) {
      await mobile.keyboard.press(tabKey);
    }
    assert.equal(await mobile.evaluate(() => document.activeElement.id), "check-plan");
    await mobile.keyboard.press("Enter");
    await mobile.locator("#plan-status").filter({hasText: "passed the fixture"}).waitFor();
    assert.equal(await mobile.evaluate(() => document.activeElement.id), "check-plan");
    await mobile.keyboard.press(tabKey);
    assert.equal(await mobile.evaluate(() => document.activeElement.id), "run");
    await mobile.keyboard.press("Enter");
    await mobile.locator("#results-output").waitFor();
    assert.equal(await mobile.evaluate(() => document.activeElement.id), "results-heading");
    assert.equal(await mobile.evaluate(() => document.documentElement.scrollWidth), 375);
    assert.match(await mobile.locator("#reachability-summary").innerText(), /\d+ → \d+/);
    await mobile.screenshot({path: path.join(output, engine + "-first-mobile-results.png")});

    // Stub a loss at the comparison boundary. The built-in walkway example
    // adds links and supplies gains and equal counts for real endpoint checks.
    await mobile.route("**/api/e17/v1/comparison?id=4&*", async (route) => {
      const response = await route.fetch();
      const data = await response.json();
      data.after = data.before.map(([node, , , sampled]) => [node, null, 0, sampled]);
      await route.fulfill({response, json: data});
    }, {times: 1});
    await mobile.getByText("Comparison filters and map view", {exact: true}).press("Enter");
    await mobile.getByRole("button", {name: "Update comparison"}).press("Enter");
    await mobile.locator("#reachability-summary").filter({hasText: "1 fewer reachable destination (1 → 0)"}).waitFor();
    assert.match(await mobile.locator("#reachability-difference").innerText(), /No longer reachable/);
    assert.deepEqual(errors, []);
    console.log(engine + " E17-S01–S09 and E18-S01–S07 acceptance passed");
  } finally {
    try {
      await browser?.close();
    } finally {
      child.kill();
    }
  }
}

run().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
