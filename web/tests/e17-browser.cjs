// E17-S01–S09 executable Chromium/WebKit acceptance against the Rust binary.
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
    await page.goto(base);

    // S01/S08: real Census boundary, visible gaps, keyboard initialization, saved E02 file.
    await page.locator("#boundary path").waitFor();
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
    await page.getByLabel("Walkway width (mm)").fill("8000");
    await page.getByRole("button", {name: "Preview and screen"}).press("Enter");
    await page.locator("#plan-status").filter({hasText: "right-of-way"}).waitFor();
    assert.equal(await page.getByRole("button", {name: "Run scenario"}).isDisabled(), true);
    assert.match(await page.locator("#costs").innerText(), /Modeled incremental costs/);
    await page.getByLabel("Walkway width (mm)").fill("1900");
    await page.getByRole("button", {name: "Preview and screen"}).press("Enter");
    await page.locator("#plan-status").filter({hasText: "passed the fixture"}).waitFor();

    // S05: browser submits an actual job. Queue/running/failed are deterministic Rust tests.
    await page.getByRole("button", {name: "Run scenario"}).press("Enter");
    await page.locator("#run-status").filter({hasText: "Completed · run"}).waitFor();
    assert.match(await page.locator("#submitted-plan").innerText(), /Parking approach → Clinic approach · 1900 mm/);
    await page.getByLabel("Walkway width (mm)").fill("2100");
    assert.match(await page.locator("#submitted-plan").innerText(), /1900 mm/);
    await page.getByLabel("Walkway width (mm)").fill("1900");
    await page.getByRole("button", {name: "Preview and screen"}).press("Enter");
    await page.locator("#plan-status").filter({hasText: "passed the fixture"}).waitFor();

    // S06/S07: real matched map updates and a shared extent and legend.
    await page.locator("#results-content").waitFor();
    await page.getByLabel("Mode set").selectOption("walk-bus");
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

    // S09: reversible snapshots and timed sequential playback.
    const final = await page.locator("#after-data").innerText();
    await page.getByRole("button", {name: "Previous"}).press("Enter");
    await page.locator("#after-title").filter({hasText: "Phase 1"}).waitFor();
    await page.getByRole("button", {name: "Previous"}).press("Enter");
    await page.locator("#after-title").filter({hasText: "Baseline"}).waitFor();
    assert.equal(await page.locator("#before-data").innerText(), await page.locator("#after-data").innerText());
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
    await page.getByRole("button", {name: "Preview and screen"}).press("Enter");
    await page.locator("#plan-status").filter({hasText: "passed the fixture"}).waitFor();
    await page.getByRole("button", {name: "Run scenario"}).press("Enter");
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
    await page.getByRole("button", {name: "Run scenario"}).press("Enter");
    await page.locator("#run-status").filter({hasText: "Could not check run 3"}).waitFor();
    await page.getByRole("button", {name: "Check run status"}).press("Enter");
    await page.locator("#run-status").filter({hasText: /Failed · run 3.*budget/}).waitFor();
    assert.match(await page.locator("#submitted-plan").innerText(), /budgets 1 USD capital/);
    assert.match(await page.locator("#result-plan").innerText(), /Run 2/);
    assert.deepEqual(errors, []);
    console.log(engine + " E17-S01–S09 acceptance passed");
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
