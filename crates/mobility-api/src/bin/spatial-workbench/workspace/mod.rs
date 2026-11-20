//! E19 versioned workspace routes. Authorization precedes access to project data.
mod export;
mod store;

use super::{model, server::Response};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, io, path::Path};

use store::{
    Comment, Grant, Inputs, Job, PlanInput, Project, Projection, Review, Revision, Role, Scenario,
    Share, Store,
};

pub(super) const ROOT: &str = "/api/e19/v1/";
const OPENAPI: &str = include_str!("../../../../../../docs/workspaces/openapi.json");
const EXTENSIONS: &str = include_str!("../../../../../../docs/workspaces/extensions.json");

type Result<T> = std::result::Result<T, Error>;

struct Error(&'static str, &'static str, String);

impl Error {
    fn invalid(message: impl Into<String>) -> Self {
        Self("400 Bad Request", "invalid_request", message.into())
    }

    fn unauthenticated() -> Self {
        Self(
            "401 Unauthorized",
            "unauthenticated",
            "A workspace bearer credential is required.".into(),
        )
    }

    fn missing() -> Self {
        Self(
            "404 Not Found",
            "not_found",
            "Workspace resource unavailable.".into(),
        )
    }

    fn forbidden() -> Self {
        Self(
            "403 Forbidden",
            "forbidden",
            "The workspace role cannot perform this action.".into(),
        )
    }

    fn conflict(message: impl Into<String>) -> Self {
        Self("409 Conflict", "conflict", message.into())
    }

    fn internal(message: impl Into<String>) -> Self {
        Self("500 Internal Server Error", "internal", message.into())
    }

    fn response(self) -> Response {
        Response::json(self.0, json!({"code":self.1,"message":self.2}).to_string())
    }
}

fn parse<T: DeserializeOwned>(body: &[u8]) -> Result<T> {
    serde_json::from_slice(body).map_err(|error| Error::invalid(error.to_string()))
}

fn built_json(value: model::Result<String>) -> Result<Value> {
    let text = value.map_err(Error::invalid)?;
    store::exact_json(&text).map_err(|_| Error::internal("Invalid model output."))
}

fn ok(value: Value) -> Response {
    Response::json("200 OK", value.to_string())
}

fn created(value: Value) -> Response {
    Response::json("201 Created", value.to_string())
}

fn id(text: &str) -> Result<usize> {
    let number = text.parse::<usize>().map_err(|_| Error::missing())?;
    if number == 0 || number.to_string() != text {
        return Err(Error::missing());
    }
    Ok(number - 1)
}

fn text(value: &str, limit: usize) -> Result<()> {
    if value.trim().is_empty() || value.len() > limit || value.chars().any(char::is_control) {
        return Err(Error::invalid(format!(
            "Supply 1 to {limit} UTF-8 bytes without control characters."
        )));
    }
    Ok(())
}

fn plan(input: &PlanInput) -> Result<model::Plan> {
    if !matches!(input.projects.len(), 1 | 2) {
        return Err(Error::invalid("Select one or two candidate links."));
    }
    let projects = input
        .projects
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(",");
    model::Plan::parse(
        &projects,
        &input.width.to_string(),
        &input.capital.to_string(),
        &input.annual.to_string(),
    )
    .map_err(Error::invalid)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewProject {
    title: String,
    area: String,
    date: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewScenario {
    title: String,
    plan: PlanInput,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewGrant {
    role: Role,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewShare {
    projection: Projection,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewComment {
    text: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewReview {
    state: Review,
}

pub(super) struct Workspace {
    owner: String,
    executable: String,
    store: Store,
}

impl Workspace {
    pub fn open(directory: &Path, owner_file: &Path) -> io::Result<Self> {
        Ok(Self {
            owner: store::read_owner(owner_file)?,
            executable: store::executable_digest()?,
            store: Store::open(directory)?,
        })
    }

    pub fn dispatch(
        &mut self,
        method: &str,
        path: &str,
        authorization: Option<&str>,
        body: &[u8],
    ) -> Response {
        self.route(method, path, authorization, body)
            .unwrap_or_else(Error::response)
    }

    fn stale(&self, project: &Project) -> Option<&'static str> {
        let snapshot = project.city["projectTsv"].as_str().unwrap_or_default();
        if store::digest(snapshot.as_bytes()) != project.inputs.city_snapshot_sha256
            || store::digest(project.city.to_string().as_bytes()) != project.inputs.city_view_sha256
        {
            return Some("saved_city_snapshot_changed");
        }
        if project.inputs.executable_sha256 != self.executable {
            return Some("evaluator_changed");
        }
        None
    }

    fn credential(&self, authorization: Option<&str>) -> Result<String> {
        let secret = authorization
            .and_then(|value| value.strip_prefix("Bearer "))
            .filter(|value| store::valid_token(value))
            .ok_or_else(Error::unauthenticated)?;
        Ok(store::digest(secret.as_bytes()))
    }

    fn owner(&self, credential: &str) -> bool {
        constant_time_equal(&self.owner, credential)
    }

    fn route(
        &mut self,
        method: &str,
        path: &str,
        authorization: Option<&str>,
        body: &[u8],
    ) -> Result<Response> {
        let parts: Vec<_> = path.split('/').collect();
        match (method, parts.as_slice()) {
            ("GET", ["openapi.json"]) => return Ok(Response::json("200 OK", OPENAPI.into())),
            ("GET", ["extensions"]) => return Ok(Response::json("200 OK", EXTENSIONS.into())),
            ("POST", ["extensions", "validate"]) => return validate_extension(body),
            ("GET", ["shares", token]) if store::valid_token(token) => {
                let share = self
                    .store
                    .ledger
                    .shares
                    .get(&store::digest(token.as_bytes()))
                    .ok_or_else(Error::missing)?;
                return export::projection(share.projection, &share.payload);
            }
            ("GET", ["cities"]) => return Ok(ok(built_json(model::areas())?)),
            ("GET", ["cities", area, date]) => {
                return Ok(ok(built_json(model::study(area, date, false))?))
            }
            (_, ["projects", ..]) => {}
            _ => return Err(Error::missing()),
        }

        let credential = self.credential(authorization)?;
        let owner = self.owner(&credential);
        let ["projects", project_id, tail @ ..] = parts.as_slice() else {
            return match method {
                "GET" => {
                    let projects: Vec<_> = self
                        .store
                        .ledger
                        .projects
                        .iter()
                        .enumerate()
                        .filter_map(|(index, project)| {
                            let (_, role) = access(project, owner, &credential)?;
                            Some(
                                json!({"id":index+1,"title":project.title,"area":project.area,
                                "date":project.date,"role":role,"stale":self.stale(project)}),
                            )
                        })
                        .collect();
                    if projects.is_empty() && !owner {
                        return Err(Error::unauthenticated());
                    }
                    Ok(ok(json!({"projects":projects})))
                }
                "POST" if owner => self.create_project(parse(body)?),
                "POST" => Err(Error::forbidden()),
                _ => Err(Error::missing()),
            };
        };

        let p = id(project_id)?;
        let project = self
            .store
            .ledger
            .projects
            .get(p)
            .ok_or_else(Error::missing)?;
        let (member, role) = access(project, owner, &credential).ok_or_else(Error::missing)?;
        let member = member.to_owned();
        let stale = self.stale(project);
        match (method, tail) {
            ("GET", []) => {
                let scenarios: Vec<_> = project.scenarios.iter().enumerate().map(|(index, scenario)|
                    json!({"id":index+1,"title":scenario.title,"revisions":scenario.revisions.len()})).collect();
                return Ok(ok(
                    json!({"id":p+1,"title":project.title,"area":project.area,"date":project.date,
                    "role":role,"inputs":project.inputs,"stale":stale,"city":project.city,"scenarios":scenarios}),
                ));
            }
            ("GET", ["members"]) => {
                require_owner(role)?;
                let members: Vec<_> = project
                    .grants
                    .iter()
                    .map(|(id, grant)| json!({"id":id,"role":grant.role}))
                    .collect();
                return Ok(ok(json!({"members":members})));
            }
            ("POST", ["members"]) => {
                require_owner(role)?;
                let request: NewGrant = parse(body)?;
                if request.role == Role::Owner {
                    return Err(Error::invalid("Grant an editor or viewer role."));
                }
                let token = store::token()?;
                let member_id = store::token()?;
                return self.store.change(|ledger| {
                    ledger.projects[p].grants.insert(
                        member_id.clone(),
                        Grant {
                            role: request.role,
                            credential_sha256: store::digest(token.as_bytes()),
                        },
                    );
                    Ok(created(
                        json!({"id":member_id,"role":request.role,"token":token}),
                    ))
                });
            }
            ("DELETE", ["members", member_id]) => {
                require_owner(role)?;
                return self.store.change(|ledger| {
                    ledger.projects[p]
                        .grants
                        .remove(*member_id)
                        .ok_or_else(Error::missing)?;
                    Ok(ok(json!({"revoked":true})))
                });
            }
            ("POST", ["scenarios"]) => {
                require_writer(role)?;
                require_current(stale)?;
                let request: NewScenario = parse(body)?;
                text(&request.title, 120)?;
                return self.store.change(|ledger| {
                    let scenarios = &mut ledger.projects[p].scenarios;
                    scenarios.push(Scenario {
                        title: request.title,
                        revisions: vec![revision(request.plan)?],
                    });
                    Ok(created(json!({"id":scenarios.len(),"revision":1})))
                });
            }
            ("DELETE", ["shares", share_id]) => {
                require_owner(role)?;
                return self.store.change(|ledger| {
                    if !ledger
                        .shares
                        .get(*share_id)
                        .is_some_and(|share| share.project == p)
                    {
                        return Err(Error::missing());
                    }
                    ledger.shares.remove(*share_id);
                    Ok(ok(json!({"revoked":true})))
                });
            }
            ("GET", ["shares"]) => {
                require_owner(role)?;
                let shares: Vec<_> = self
                    .store
                    .ledger
                    .shares
                    .iter()
                    .filter(|(_, share)| share.project == p)
                    .map(|(key, share)| json!({"id":key,"projection":share.projection}))
                    .collect();
                return Ok(ok(json!({"shares":shares})));
            }
            _ => {}
        }
        let ["scenarios", scenario_id, "revisions", rest @ ..] = tail else {
            return Err(Error::missing());
        };
        let s = id(scenario_id)?;
        let scenario = project.scenarios.get(s).ok_or_else(Error::missing)?;
        if rest.is_empty() {
            return match method {
                "GET" => Ok(ok(
                    json!({"title":scenario.title,"revisions":scenario.revisions.iter().enumerate()
                    .map(|(index, rev)| json!({"id":index+1,"plan":rev.plan,"review":rev.review})).collect::<Vec<_>>()}),
                )),
                "POST" => {
                    require_writer(role)?;
                    require_current(stale)?;
                    let entry = revision(parse(body)?)?;
                    self.store.change(|ledger| {
                        let revisions = &mut ledger.projects[p].scenarios[s].revisions;
                        revisions.push(entry);
                        Ok(created(json!({"id":revisions.len()})))
                    })
                }
                _ => Err(Error::missing()),
            };
        }
        let [revision_id, action @ ..] = rest else {
            unreachable!()
        };
        let r = id(revision_id)?;
        let revision = scenario.revisions.get(r).ok_or_else(Error::missing)?;
        let revision_location =
            format!("{ROOT}projects/{project_id}/scenarios/{scenario_id}/revisions/{revision_id}");
        match (method, action) {
            ("GET", []) => Ok(ok(
                json!({"id":r+1,"plan":revision.plan,"review":revision.review,
                "comments":revision.comments,"job": revision.job.as_ref().map(|_| format!("{revision_location}/jobs")),"stale":stale}),
            )),
            ("GET", ["jobs"]) => match &revision.job {
                Some(Job::Completed { .. }) => Ok(ok(
                    json!({"status":"completed","evaluation":format!("{revision_location}/evaluation")}),
                )),
                Some(Job::Running) => Ok(ok(json!({"status":"running"}))),
                Some(Job::Interrupted) => Ok(ok(json!({"status":"interrupted"}))),
                Some(Job::Failed { error }) => Ok(ok(json!({"status":"failed","error":error}))),
                None => Err(Error::missing()),
            },
            ("POST", ["jobs"]) => {
                require_writer(role)?;
                require_current(stale)?;
                if !body.is_empty() {
                    return Err(Error::invalid("Job submission takes no body."));
                }
                self.run(p, s, r, revision_location)
            }
            ("GET", ["evaluation"]) => Ok(ok(completed(revision)?.clone())),
            ("GET", ["exports", name]) => export::download(name, completed(revision)?),
            ("POST", ["comments"]) => {
                require_writer(role)?;
                let request: NewComment = parse(body)?;
                text(&request.text, 2000)?;
                self.store.change(|ledger| {
                    let comments = &mut ledger.projects[p].scenarios[s].revisions[r].comments;
                    comments.push(Comment {
                        role,
                        member,
                        text: request.text,
                    });
                    Ok(created(json!({"id":comments.len(),"revision":r+1})))
                })
            }
            ("PUT", ["review"]) => {
                require_writer(role)?;
                let request: NewReview = parse(body)?;
                if matches!(request.state, Review::Approved | Review::ChangesRequested)
                    || matches!(revision.review, Review::Approved | Review::ChangesRequested)
                {
                    require_owner(role)?;
                }
                self.store.change(|ledger| {
                    ledger.projects[p].scenarios[s].revisions[r].review = request.state;
                    Ok(ok(json!({"state":request.state,"revision":r+1})))
                })
            }
            ("POST", ["shares"]) => {
                require_owner(role)?;
                let request: NewShare = parse(body)?;
                let payload = match request.projection {
                    Projection::Scenario => {
                        json!({"schema":"carfree-public-scenario-v1","plan":revision.plan,
                        "caveat":"Synthetic schematic scenario. No observed calibration or city network correspondence."})
                    }
                    Projection::Report | Projection::Map => {
                        export::public_payload(request.projection, completed(revision)?)?
                    }
                };
                let token = store::token()?;
                let key = store::digest(token.as_bytes());
                let payload_sha256 = store::digest(payload.to_string().as_bytes());
                self.store.change(|ledger| {
                    ledger.shares.insert(key.clone(), Share { project: p, projection:request.projection, payload, payload_sha256 });
                    Ok(created(json!({"id":key,"path":format!("{ROOT}shares/{token}"),"projection":request.projection})))
                })
            }
            _ => Err(Error::missing()),
        }
    }

    fn create_project(&mut self, request: NewProject) -> Result<Response> {
        text(&request.title, 120)?;
        let city = built_json(model::study(&request.area, &request.date, true))?;
        let snapshot = city["projectTsv"]
            .as_str()
            .ok_or_else(|| Error::internal("City snapshot missing."))?;
        let project = Project {
            title: request.title,
            area: request.area,
            date: request.date,
            inputs: Inputs {
                executable_sha256: self.executable.clone(),
                city_snapshot_sha256: store::digest(snapshot.as_bytes()),
                city_view_sha256: store::digest(city.to_string().as_bytes()),
            },
            city,
            grants: BTreeMap::new(),
            scenarios: Vec::new(),
        };
        self.store.change(|ledger| {
            ledger.projects.push(project);
            Ok(created(json!({"id":ledger.projects.len()})))
        })
    }

    fn run(&mut self, p: usize, s: usize, r: usize, location: String) -> Result<Response> {
        let project = &self.store.ledger.projects[p];
        let revision = &project.scenarios[s].revisions[r];
        if matches!(revision.job, Some(Job::Completed { .. })) {
            return Ok(ok(
                json!({"status":"completed","job":format!("{location}/jobs")}),
            ));
        }
        let input = revision.plan.clone();
        let resolved = plan(&input)?;
        let inputs = project.inputs.clone();
        self.store.change(|ledger| {
            ledger.projects[p].scenarios[s].revisions[r].job = Some(Job::Running);
            Ok(())
        })?;
        let job = match model::run(&resolved) {
            Ok(result) => match export::evaluation(&inputs, &input, result) {
                Ok(result) => Job::Completed { result },
                Err(error) => Job::Failed { error: error.2 },
            },
            Err(error) => Job::Failed { error },
        };
        let status = match job {
            Job::Completed { .. } => "completed",
            Job::Failed { .. } => "failed",
            _ => unreachable!(),
        };
        let saved = self.store.change(|ledger| {
            ledger.projects[p].scenarios[s].revisions[r].job = Some(job);
            Ok(ok(
                json!({"status":status,"job":format!("{location}/jobs")}),
            ))
        });
        if let Err(error) = &saved {
            let diagnostic = format!(
                "Evaluation finished but the result could not be saved. {}",
                error.2
            );
            let failed = Job::Failed { error: diagnostic };
            if self
                .store
                .change(|ledger| {
                    ledger.projects[p].scenarios[s].revisions[r].job = Some(failed.clone());
                    Ok(())
                })
                .is_err()
            {
                self.store.ledger.projects[p].scenarios[s].revisions[r].job = Some(failed);
            }
        }
        saved
    }
}

fn constant_time_equal(first: &str, second: &str) -> bool {
    first.len() == second.len()
        && first
            .bytes()
            .zip(second.bytes())
            .fold(0, |difference, (a, b)| difference | (a ^ b))
            == 0
}

fn access<'a>(project: &'a Project, owner: bool, credential: &str) -> Option<(&'a str, Role)> {
    if owner {
        return Some(("owner", Role::Owner));
    }
    project.grants.iter().find_map(|(id, grant)| {
        constant_time_equal(&grant.credential_sha256, credential)
            .then_some((id.as_str(), grant.role))
    })
}

fn require_owner(role: Role) -> Result<()> {
    if role != Role::Owner {
        return Err(Error::forbidden());
    }
    Ok(())
}

fn require_writer(role: Role) -> Result<()> {
    if role == Role::Viewer {
        return Err(Error::forbidden());
    }
    Ok(())
}

fn require_current(stale: Option<&str>) -> Result<()> {
    if let Some(reason) = stale {
        return Err(Error::conflict(format!(
            "Project inputs are stale ({reason}). Reopen with the original build and saved inputs."
        )));
    }
    Ok(())
}

fn revision(input: PlanInput) -> Result<Revision> {
    let resolved = plan(&input)?;
    model::preview(&resolved).map_err(Error::invalid)?;
    Ok(Revision {
        plan: input,
        review: Review::Draft,
        comments: Vec::new(),
        job: None,
    })
}

fn completed(revision: &Revision) -> Result<&Value> {
    match &revision.job {
        Some(Job::Completed { result }) => Ok(result),
        _ => Err(Error::conflict("A completed evaluation is required.")),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Extension {
    id: String,
    version: u32,
    contract: String,
    sample: Value,
}

fn validate_extension(body: &[u8]) -> Result<Response> {
    let request: Extension = parse(body)?;
    text(&request.id, 80)?;
    if request.version != 1 {
        return Err(Error::invalid("Unsupported extension manifest version."));
    }
    match request.contract.as_str() {
        "optimizer-plan-v1" => {
            let entry: PlanInput = serde_json::from_value(request.sample)
                .map_err(|e| Error::invalid(e.to_string()))?;
            let resolved = plan(&entry)?;
            let screen = built_json(model::preview(&resolved))?;
            if !screen["warning"].is_null() {
                return Err(Error::invalid(
                    "Optimizer candidate did not pass workbench screening.",
                ));
            }
        }
        "dataset-upload-v1" => {
            use mobility_ingest::connectors::{Input, Request};
            let sample: Request = serde_json::from_value(request.sample)
                .map_err(|e| Error::invalid(e.to_string()))?;
            let Input::Custom { path, fields, .. } = &sample.input else {
                return Err(Error::invalid("Expected a mapped upload request."));
            };
            if path.as_os_str().is_empty()
                || path
                    .components()
                    .any(|part| !matches!(part, std::path::Component::Normal(_)))
                || !(1..=128).contains(&fields.len())
                || fields.iter().any(|(name, field)| {
                    name.trim().is_empty()
                        || name.chars().any(char::is_control)
                        || field.column.trim().is_empty()
                })
            {
                return Err(Error::invalid(
                    "Provide a relative sample path and 1–128 named mappings.",
                ));
            }
            let date = sample.source.retrieved_on.clone();
            mobility_ingest::connectors::inventory(&[sample], &date)
                .map_err(|error| Error::invalid(error.to_string()))?;
        }
        _ => return Err(Error::invalid("Unsupported extension contract.")),
    }
    Ok(ok(
        json!({"id":request.id,"version":1,"contract":request.contract,"compatible":true}),
    ))
}

#[cfg(test)]
mod tests;
