//! Fixed-field operator events. Never serialize inputs, request headers or job errors.
use serde_json::json;
use std::{
    io::{self, Write},
    time::Instant,
};

#[derive(Clone, Copy)]
pub(crate) enum State {
    Running,
    Completed,
    EvaluationFailed,
    SaveFailed,
}

fn record(scope: &'static str, job: &str, state: State, started: Instant) -> serde_json::Value {
    let (status, action) = match state {
        State::Running => ("running", None),
        State::Completed => ("completed", None),
        State::EvaluationFailed => ("evaluation_failed", Some("inspect the job status endpoint")),
        State::SaveFailed => (
            "save_failed",
            Some("check workspace storage and job status"),
        ),
    };
    json!({
        "schema": "carfree-job-event-v1",
        "component": scope,
        "job": job,
        "status": status,
        "action": action,
        "elapsed_ms": started.elapsed().as_millis() as u64,
    })
}

pub(crate) fn workbench(run: u64, state: State, started: Instant) {
    let _ = writeln!(
        io::stderr().lock(),
        "{}",
        record("workbench", &run.to_string(), state, started)
    );
}

pub(crate) fn workspace(
    project: usize,
    scenario: usize,
    revision: usize,
    state: State,
    started: Instant,
) {
    let job = format!("{}/{}/{}", project + 1, scenario + 1, revision + 1);
    let _ = writeln!(
        io::stderr().lock(),
        "{}",
        record("workspace", &job, state, started)
    );
}
