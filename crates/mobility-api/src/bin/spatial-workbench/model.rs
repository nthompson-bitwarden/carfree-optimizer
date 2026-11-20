//! E17's versioned local adapter. Engines own all routing and scenario checks.
use interventions::{
    feasibility::{Bounds, Budget},
    scenario::Snapshot,
};
use mobility_graph::{
    network::{LinkKind, Network},
    Mode, NodeId,
};
use mobility_ingest::{
    study_area::{CatalogueImporter, CityProject, StudyArea},
    Catalogue, GeographyKind,
};
use mobility_routing::{
    planner::{departure_window, Options},
    Profile, Request,
};
use optimizer_env::{
    catalog::Catalog,
    reference::{Config, Environment, IndexedTransition},
    synthetic,
};
use std::fmt::{Display, Write};

#[path = "report.rs"]
mod report;
pub use report::Completed;

pub type Result<T> = std::result::Result<T, String>;

pub fn json(value: &str) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\0'..='\u{1f}' => write!(out, "\\u{:04x}", c as u32).unwrap(),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

pub fn array<T: Display>(items: impl IntoIterator<Item = T>) -> String {
    let entries: Vec<_> = items.into_iter().map(|entry| entry.to_string()).collect();
    format!("[{}]", entries.join(","))
}

fn catalogue() -> Result<Catalogue> {
    Catalogue::pilot().map_err(|e| e.to_string())
}

pub fn areas() -> Result<String> {
    Ok(array(catalogue()?.entries().iter().map(|area| {
        let name = match area.kind {
            GeographyKind::City => format!("{}, {}", area.name, area.states.join("–")),
            GeographyKind::MetropolitanArea => area.name.clone(),
        };
        format!("[{},{}]", json(&area.id), json(&name))
    })))
}

pub fn study(id: &str, date: &str, initialize: bool) -> Result<String> {
    let catalogue = catalogue()?;
    let geo = catalogue.get(id).ok_or("Choose a supported Census area.")?;
    let area = StudyArea::from_geography(geo).map_err(|e| e.to_string())?;
    let project = CityProject::initialize(
        &id.replace(':', "-"),
        area,
        date,
        &mut CatalogueImporter::new(&catalogue),
    )
    .map_err(|e| e.to_string())?;
    let rings = array(project.area().rings().iter().map(|ring| {
        array(
            ring.iter()
                .map(|p| format!("[{},{}]", p.longitude, p.latitude)),
        )
    }));
    let entries = array(project.coverage().entries.iter().map(|row| {
        format!(
            "[{},{},{},{},{}]",
            json(row.kind.as_str()),
            json(row.status.as_str()),
            json(row.source.as_deref().unwrap_or("Unknown")),
            json(row.version.as_deref().unwrap_or("Unknown")),
            json(&row.detail),
        )
    }));
    Ok(format!(
        r#"{{"rings":{rings},"coverage":{entries},"baseline":{},"missing":{},"projectTsv":{}}}"#,
        json(project.baseline_id()),
        array(
            project
                .coverage()
                .missing_required
                .iter()
                .map(|kind| json(kind.as_str()))
        ),
        if initialize {
            json(&project.to_tsv())
        } else {
            "null".into()
        },
    ))
}

fn fixture() -> Result<Config> {
    synthetic::configuration(0)
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Car => "road",
        Mode::Bicycle | Mode::ElectricBicycle => "bike",
        Mode::Walk => "walk",
        Mode::Bus => "bus",
        Mode::LightRail | Mode::Rail => "rail",
    }
}

fn links(network: &Network) -> String {
    array(network.data().links.iter().filter_map(|link| {
        let mode = match &link.kind {
            LinkKind::Street { mode, .. } | LinkKind::Transit { mode, .. } => *mode,
            _ => return None,
        };
        Some(format!(
            "[{},{},{},{},{}]",
            link.id.0,
            link.from.0,
            link.to.0,
            json(mode_name(mode)),
            json(&format!("{:?}", link.access)),
        ))
    }))
}

pub fn network() -> Result<String> {
    let config = fixture()?;
    let net = config.history.current().network();
    let projects = array(config.catalog.projects().iter().map(|entry| {
        let [from, to] = entry.payload.anchors.map(|node| node.unwrap().0);
        format!("[{},{from},{to}]", json(&entry.id))
    }));
    Ok(format!(
        r#"{{"links":{},"nodes":{},"projects":{projects},"source":{},"version":{},"geography":{},"date":{},"license":{},"uncertainty":{}}}"#,
        links(net),
        array(net.data().nodes.iter().map(|node| node.id.0)),
        json(&net.data().provenance.source),
        json(&net.data().provenance.source_version),
        json(&net.data().provenance.geography),
        json(&net.data().provenance.date),
        json(&net.data().provenance.license),
        json(&net.data().provenance.uncertainty),
    ))
}

#[derive(Clone)]
pub struct Plan {
    projects: Vec<usize>,
    width_mm: u32,
    capital: u32,
    annual: u32,
}

impl Plan {
    pub fn parse(projects: &str, width: &str, capital: &str, annual: &str) -> Result<Self> {
        let projects: Vec<_> = projects
            .split(',')
            .map(str::parse)
            .collect::<std::result::Result<_, _>>()
            .map_err(|_| "Select one or two candidate links.")?;
        if projects.is_empty()
            || projects.len() > 2
            || projects.iter().any(|&p| p >= 3)
            || (projects.len() == 2 && projects[0] == projects[1])
        {
            return Err("Select one or two distinct candidate links.".into());
        }
        Ok(Self {
            projects,
            width_mm: integer(width, 1500, 8000, "width (mm)")?,
            capital: integer(capital, 0, 10000, "capital budget (2026 USD)")?,
            annual: integer(annual, 0, 10000, "annual budget (2026 USD/year)")?,
        })
    }

    pub fn view(&self) -> String {
        format!(
            r#"{{"projects":{},"width":{},"capital":{},"annual":{}}}"#,
            array(self.projects.iter()),
            self.width_mm,
            self.capital,
            self.annual,
        )
    }

    fn configure(&self, mut config: Config) -> Result<Config> {
        let mut projects = config.catalog.projects().to_vec();
        if let [a, b] = self.projects.as_slice() {
            if !projects[*a]
                .payload
                .anchors
                .iter()
                .any(|node| projects[*b].payload.anchors.contains(node))
            {
                return Err("The selected candidate links do not share a node. Select a connected corridor.".into());
            }
        }
        for &index in &self.projects {
            for segment in &mut projects[index].payload.segments {
                segment.other_width_mm = self.width_mm;
            }
        }
        config.catalog = Catalog::new(config.catalog.revision().to_owned(), projects)
            .map_err(|e| e.to_string())?;
        config.opening.budget.capital = f64::from(self.capital);
        config.opening.budget.annual = f64::from(self.annual);
        Ok(config)
    }
}

fn integer(s: &str, min: u32, max: u32, label: &str) -> Result<u32> {
    s.parse::<u32>()
        .ok()
        .filter(|number| (min..=max).contains(number))
        .ok_or_else(|| format!("Enter a whole number from {min} to {max} for {label}."))
}

fn money(bounds: Bounds) -> String {
    format!("[{},{},{}]", bounds.lower, bounds.central, bounds.upper)
}

fn estimates(config: &Config, projects: &[usize], budget: &Budget) -> Result<String> {
    let mut entries = Vec::new();
    for &index in projects {
        let p = &config.catalog.projects()[index];
        let (capital, annual) = p
            .payload
            .incremental_costs(budget)
            .map_err(|e| e.to_string())?;
        entries.push(format!(
            "[{},{},{}]",
            json(&p.id),
            money(capital),
            money(annual)
        ));
    }
    Ok(array(entries))
}

pub fn preview(plan: &Plan) -> Result<String> {
    let config = plan.configure(fixture()?)?;
    let costs = estimates(&config, &plan.projects, &config.opening.budget)?;
    let (warning, total) = match evaluate(config, plan) {
        Ok(phases) => (String::from("null"), phases.last().unwrap().budget.clone()),
        Err(error) => (json(&error), String::from("null")),
    };
    Ok(format!(
        r#"{{"costs":{costs},"warning":{warning},"total":{total}}}"#
    ))
}

#[derive(Debug)]
pub struct Phase {
    network: Network,
    name: String,
    budget: String,
    metrics: String,
    report: report::PhaseEvidence,
}

impl Phase {
    pub fn view(&self) -> String {
        format!(
            r#"{{"name":{},"revision":{},"links":{},"budget":{},"metrics":{}}}"#,
            json(&self.name),
            json(&self.network.data().revision),
            links(&self.network),
            self.budget,
            self.metrics,
        )
    }
}

fn record(
    snapshot: &Snapshot,
    projects: &[String],
    transition: &IndexedTransition,
    catalog: &str,
) -> Phase {
    let assignment = &transition.evidence.trips.assignment;
    let essential = &assignment.essential;
    let reference = &transition.reward.after;
    Phase {
        network: snapshot.network().clone(),
        name: projects
            .last()
            .cloned()
            .unwrap_or_else(|| "Baseline".into()),
        report: report::PhaseEvidence::new(snapshot, projects, transition, catalog),
        budget: format!(
            "[{},{}]",
            money(transition.budget.capital),
            money(transition.budget.annual),
        ),
        metrics: format!(
            r#"{{"essentialTotal":{},"essentialUnserved":{},"essentialReferenceTotal":{},"essentialNoMode":{},"essentialCarRequired":{},"noncyclingUnserved":{},"stepFreeUnserved":{},"assignment":{}}}"#,
            essential.total,
            essential.unserved,
            reference.essential.total,
            reference.essential.no_mode,
            reference.essential.car_required,
            reference.noncycling.unserved,
            reference.step_free.unserved,
            json(&format!("{:?}", reference.status)),
        ),
    }
}

fn evaluate(config: Config, plan: &Plan) -> Result<Vec<Phase>> {
    let mut env = Environment::new(config).map_err(|e| e.to_string())?;
    let opening = env.reset(42).map_err(|e| e.to_string())?.clone();
    let mut projects = Vec::new();
    let catalog = env.catalog().revision().to_owned();
    let mut phases = vec![record(
        env.snapshot().unwrap(),
        &projects,
        &opening,
        &catalog,
    )];
    let mut state = opening.state;
    for &index in &plan.projects {
        let name = env.catalog().projects()[index].id.clone();
        let next = env
            .step(index, state)
            .map_err(|e| format!("{name}: {e}"))?
            .clone();
        state = next.state;
        projects.push(name);
        phases.push(record(env.snapshot().unwrap(), &projects, &next, &catalog));
    }
    Ok(phases)
}

pub fn run(plan: &Plan) -> Result<Completed> {
    report::build(plan)
}

pub struct MapQuery {
    origin: NodeId,
    bus: bool,
    step_free: bool,
    start: i64,
    end: i64,
    seconds: u32,
}

impl MapQuery {
    pub fn parse(
        origin: &str,
        modes: &str,
        profile: &str,
        start: &str,
        end: &str,
        seconds: &str,
    ) -> Result<Self> {
        let origin = NodeId(integer(origin, 1, 5, "origin node")?);
        let bus = match modes {
            "walk" => false,
            "walk-bus" => true,
            _ => return Err("Choose walking or walking and bus.".into()),
        };
        let step_free = match profile {
            "step-free" => true,
            "standard" => false,
            _ => return Err("Choose a supported mobility profile.".into()),
        };
        let start = integer(start, 0, 120, "start (s)")?;
        let end = integer(end, start, 120, "end (s)")?;
        let seconds = integer(seconds, 1, 180 - end, "travel-time limit (s)")?;
        Ok(Self {
            origin,
            bus,
            step_free,
            start: i64::from(start),
            end: i64::from(end),
            seconds,
        })
    }

    fn score(&self, network: &Network) -> Result<String> {
        let departures: Vec<_> = (self.start..=self.end).collect();
        let mut modes = vec![Mode::Walk];
        if self.bus {
            modes.push(Mode::Bus);
        }
        let mut destinations = Vec::new();
        for node in &network.data().nodes {
            if self.origin == node.id {
                continue;
            }
            let request = Request {
                from: self.origin,
                to: node.id,
                departure_utc: self.start,
                deadline_utc: self.start + i64::from(self.seconds),
                initial_mode: Mode::Walk,
                vehicle: None,
                profile: Profile {
                    allowed_modes: modes.clone(),
                    step_free: self.step_free,
                    mobility_aid: self.step_free,
                    allow_unknown_access: false,
                    protected_cycling_only: true,
                    maximum_cycling_stress: Some(1),
                },
                max_states: 10_000,
            };
            let window = departure_window(
                network,
                &request,
                &Options::default(),
                &departures,
                self.seconds,
            )
            .map_err(|e| format!("Routing failed for destination {}: {e}", node.id.0))?;
            destinations.push(format!(
                "[{},{},{},{}]",
                node.id.0,
                window
                    .p50_seconds
                    .map_or("null".into(), |p50| p50.to_string()),
                window.feasible,
                departures.len(),
            ));
        }
        Ok(array(destinations))
    }
}

pub fn compare(phases: &[Phase], index: usize, query: &MapQuery) -> Result<String> {
    let after = phases.get(index).ok_or("Choose an available phase.")?;
    Ok(format!(
        r#"{{"before":{},"after":{},"beforeTrace":{},"afterTrace":{}}}"#,
        query.score(&phases[0].network)?,
        query.score(&after.network)?,
        phases[0].report.trace(),
        after.report.trace(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn e17_s01_real_boundaries_and_gaps() {
        assert!(areas().unwrap().contains("cbsa:38900"));
        let study = study("place:4159000", "2026-01-05", true).unwrap();
        assert!(study.contains(r#""missing":["Roads","Transit","Population","Demand"]"#));
        assert!(study.contains(r#""projectTsv":"carfree-study-project-v1"#));
        assert!(study.contains("-122."));
    }

    #[test]
    fn e17_s03_s04_screen_changes_and_costs_with_rust() {
        let disconnected = Plan::parse("1,2", "1900", "200", "30").unwrap();
        assert!(preview(&disconnected).is_err());
        let too_wide = Plan::parse("0", "8000", "200", "30").unwrap();
        assert!(preview(&too_wide).unwrap().contains("right-of-way"));
        assert!(run(&too_wide).is_err());
        let bad_budget = Plan::parse("0", "1900", "1", "30").unwrap();
        assert!(preview(&bad_budget).unwrap().contains("budget"));
        assert!(Plan::parse("0", "2.5", "100", "20").is_err());
        let plan = Plan::parse("0,1", "1900", "200", "30").unwrap();
        assert!(preview(&plan).unwrap().contains(r#""warning":null"#));
    }

    #[test]
    fn e17_s06_s07_s09_routed_matched_reversible_phases() {
        let plan = Plan::parse("0,1", "1900", "200", "30").unwrap();
        let completed = run(&plan).unwrap();
        let phases = &completed.phases;
        assert_eq!(phases.len(), 3);
        let query = MapQuery::parse("1", "walk", "step-free", "0", "2", "25").unwrap();
        let baseline = compare(phases, 0, &query).unwrap();
        assert_ne!(
            query.score(&phases[0].network).unwrap(),
            query.score(&phases[1].network).unwrap(),
        );
        assert_eq!(compare(phases, 0, &query).unwrap(), baseline);
        assert_ne!(phases[0].view(), phases[2].view());
        assert!(MapQuery::parse("1", "walk-bus", "step-free", "8", "14", "180").is_err());
    }
}
