//! Loopback-only HTTP/1.1 surface for the bundled workbench.
use crate::{
    model::{self, Completed, MapQuery, Plan},
    workspace::{self, Workspace},
};
use std::{
    collections::BTreeMap,
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        mpsc::{self, SyncSender},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

const ROOT: &str = "/api/e17/v1/";
const INDEX: &str = include_str!("../../../../../web/index.html");
const STYLE: &str = include_str!("../../../../../web/workbench.css");
const SCRIPT: &str = include_str!("../../../../../web/workbench.js");
const MAX_HEADER_BYTES: usize = 8192;
const MAX_BODY_BYTES: usize = 16 * 1024;
const MAX_RETAINED: usize = 64;

#[derive(Clone)]
enum State {
    Queued,
    Running,
    Failed(String),
    Completed(Arc<Completed>),
}

impl State {
    fn view(&self) -> String {
        match self {
            Self::Queued => r#"{"status":"queued"}"#.into(),
            Self::Running => r#"{"status":"running"}"#.into(),
            Self::Failed(error) => {
                format!(r#"{{"status":"failed","error":{}}}"#, model::json(error))
            }
            Self::Completed(result) => format!(
                r#"{{"status":"completed","phases":{},"report":{}}}"#,
                model::array(result.phases.iter().map(model::Phase::view)),
                result.report,
            ),
        }
    }
}

#[derive(Clone)]
struct Job {
    plan: Plan,
    state: State,
}

impl Job {
    fn view(&self) -> String {
        format!(
            r#"{{"plan":{},"state":{}}}"#,
            self.plan.view(),
            self.state.view()
        )
    }
}

#[derive(Default)]
struct Ledger {
    jobs: BTreeMap<u64, Job>,
    last: u64,
}

struct Jobs {
    ledger: Arc<Mutex<Ledger>>,
    queue: SyncSender<(u64, Plan)>,
}

impl Jobs {
    fn start(evaluate: impl Fn(Plan) -> model::Result<Completed> + Send + 'static) -> Self {
        let ledger = Arc::new(Mutex::new(Ledger::default()));
        let worker_ledger = Arc::clone(&ledger);
        let (queue, entries) = mpsc::sync_channel::<(u64, Plan)>(4);
        std::thread::spawn(move || {
            for (id, plan) in entries {
                worker_ledger
                    .lock()
                    .unwrap()
                    .jobs
                    .get_mut(&id)
                    .unwrap()
                    .state = State::Running;
                let result = match evaluate(plan) {
                    Ok(phases) => State::Completed(Arc::new(phases)),
                    Err(error) => State::Failed(error),
                };
                worker_ledger
                    .lock()
                    .unwrap()
                    .jobs
                    .get_mut(&id)
                    .unwrap()
                    .state = result;
            }
        });
        Self { ledger, queue }
    }

    fn submit(&self, plan: Plan) -> model::Result<u64> {
        let mut ledger = self.ledger.lock().unwrap();
        let id = ledger
            .last
            .checked_add(1)
            .ok_or("Restart the workbench to submit new jobs.")?;
        self.queue
            .try_send((id, plan.clone()))
            .map_err(|_| "The local queue is full or unavailable. Try again shortly.")?;
        if ledger.jobs.len() >= MAX_RETAINED {
            // At most one job is running and four queued.
            let oldest = ledger
                .jobs
                .iter()
                .find(|(_, state)| matches!(state.state, State::Failed(_) | State::Completed(_)))
                .map(|(&key, _)| key)
                .unwrap();
            ledger.jobs.remove(&oldest);
        }
        ledger.last = id;
        ledger.jobs.insert(
            id,
            Job {
                plan,
                state: State::Queued,
            },
        );
        Ok(id)
    }

    fn get(&self, id: &str) -> model::Result<Job> {
        let id: u64 = id.parse().map_err(|_| "Invalid run ID.")?;
        self.ledger
            .lock()
            .unwrap()
            .jobs
            .get(&id)
            .cloned()
            .ok_or_else(|| {
                "Run unavailable. The workbench retains recent runs until it restarts.".into()
            })
    }
}

pub fn serve(
    port: u16,
    mut workspace: Option<Workspace>,
) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    let host = listener.local_addr()?.to_string();
    let origin = format!("http://{host}");
    let jobs = Jobs::start(|plan| model::run(&plan));
    println!("Spatial workbench: {origin}");
    io::stdout().flush()?;
    for connection in listener.incoming() {
        let mut stream = connection?;
        if let Err(e) = handle(&mut stream, &host, &origin, &jobs, workspace.as_mut()) {
            if !matches!(
                e.kind(),
                io::ErrorKind::TimedOut
                    | io::ErrorKind::WouldBlock
                    | io::ErrorKind::UnexpectedEof
                    | io::ErrorKind::ConnectionReset
                    | io::ErrorKind::BrokenPipe
            ) {
                eprintln!("Local HTTP request failed: {e}");
            }
        }
    }
    Ok(())
}

fn handle(
    stream: &mut TcpStream,
    host: &str,
    origin: &str,
    jobs: &Jobs,
    workspace: Option<&mut Workspace>,
) -> io::Result<()> {
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut bytes = [0; MAX_HEADER_BYTES];
    let mut used = 0;
    let deadline = Instant::now() + Duration::from_secs(2);
    let end = loop {
        used += read_before(stream, &mut bytes[used..], deadline)?;
        if let Some(end) = bytes[..used]
            .windows(4)
            .position(|part| part == b"\r\n\r\n")
        {
            break end;
        }
        if used == MAX_HEADER_BYTES {
            return Response::error(
                "431 Request Header Fields Too Large",
                "Headers exceed 8 KiB.",
            )
            .write(stream);
        }
    };
    let Ok(head) = std::str::from_utf8(&bytes[..end]) else {
        return Response::error("400 Bad Request", "Invalid HTTP headers.").write(stream);
    };
    let request = match Request::parse(head, host, origin) {
        Ok(request) => request,
        Err(response) => return response.write(stream),
    };
    let initial = &bytes[end + 4..used];
    if initial.len() > request.length {
        return Response::error("400 Bad Request", "Unexpected bytes after the request.")
            .write(stream);
    }
    let mut body = vec![0; request.length];
    body[..initial.len()].copy_from_slice(initial);
    let mut used = initial.len();
    while used < body.len() {
        used += read_before(stream, &mut body[used..], deadline)?;
    }
    dispatch(request, &body, jobs, workspace).write(stream)
}

fn read_before(stream: &mut TcpStream, bytes: &mut [u8], deadline: Instant) -> io::Result<usize> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(io::ErrorKind::TimedOut.into());
    }
    stream.set_read_timeout(Some(remaining))?;
    match stream.read(bytes)? {
        0 => Err(io::ErrorKind::UnexpectedEof.into()),
        count => Ok(count),
    }
}

pub(crate) struct Response {
    pub(super) status: &'static str,
    pub(super) mime: &'static str,
    pub(super) body: Vec<u8>,
}

impl Response {
    pub(crate) fn new(status: &'static str, mime: &'static str, body: impl Into<String>) -> Self {
        Self::bytes(status, mime, body.into().into_bytes())
    }

    pub(crate) fn bytes(status: &'static str, mime: &'static str, body: Vec<u8>) -> Self {
        Self { status, mime, body }
    }

    pub(crate) fn json(status: &'static str, body: String) -> Self {
        Self::new(status, "application/json", body)
    }

    fn error(status: &'static str, error: &str) -> Self {
        Self::json(status, format!(r#"{{"error":{}}}"#, model::json(error)))
    }

    fn write(self, stream: &mut TcpStream) -> io::Result<()> {
        write!(
            stream,
            concat!(
                "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n",
                "Connection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n",
                "Referrer-Policy: no-referrer\r\n{}",
                "Content-Security-Policy: default-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'\r\n\r\n"
            ),
            self.status,
            self.mime,
            self.body.len(),
            if self.status == "401 Unauthorized" {
                "WWW-Authenticate: Bearer\r\n"
            } else {
                ""
            },
        )?;
        stream.write_all(&self.body)
    }
}

struct Request<'a> {
    method: &'a str,
    target: &'a str,
    authorization: Option<&'a str>,
    length: usize,
}

impl<'a> Request<'a> {
    fn parse(head: &'a str, host: &str, origin: &str) -> Result<Self, Response> {
        let mut lines = head.split("\r\n");
        let mut start = lines.next().unwrap_or("").split(' ');
        let (Some(method), Some(target), Some("HTTP/1.1"), None) =
            (start.next(), start.next(), start.next(), start.next())
        else {
            return Err(Response::error(
                "400 Bad Request",
                "Use HTTP/1.1 with an origin-form target.",
            ));
        };
        let mut headers = BTreeMap::new();
        for line in lines {
            let Some((key, value)) = line.split_once(':') else {
                return Err(Response::error("400 Bad Request", "Malformed HTTP header."));
            };
            if key.is_empty()
                || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                || headers
                    .insert(key.to_ascii_lowercase(), value.trim())
                    .is_some()
            {
                return Err(Response::error(
                    "400 Bad Request",
                    "Invalid or duplicate HTTP header.",
                ));
            }
        }
        if headers.get("host") != Some(&host) {
            return Err(Response::error(
                "403 Forbidden",
                "Open the exact loopback URL printed by the workbench.",
            ));
        }
        let workspace_route = target.starts_with(workspace::ROOT);
        let length = headers.get("content-length").copied().unwrap_or("0");
        if headers.contains_key("transfer-encoding")
            || length.is_empty()
            || !length.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(Response::error(
                "400 Bad Request",
                "Invalid request framing.",
            ));
        }
        let length: usize = length
            .parse()
            .map_err(|_| Response::error("413 Content Too Large", "Body exceeds 16 KiB."))?;
        if length > MAX_BODY_BYTES {
            return Err(Response::error(
                "413 Content Too Large",
                "Body exceeds 16 KiB.",
            ));
        }
        if length > 0
            && !(workspace_route
                && matches!(method, "POST" | "PUT")
                && headers.get("content-type") == Some(&"application/json"))
        {
            return Err(Response::error(
                "400 Bad Request",
                "Only workspace JSON writes accept a body.",
            ));
        }
        if headers.get("origin").is_some_and(|value| *value != origin)
            || !workspace_route
                && method == "POST"
                && (headers.get("origin") != Some(&origin)
                    || headers.get("x-carfree-workbench") != Some(&"1"))
        {
            return Err(Response::error(
                "403 Forbidden",
                "This operation requires the workbench origin.",
            ));
        }
        Ok(Self {
            method,
            target,
            authorization: headers.get("authorization").copied(),
            length,
        })
    }
}

fn dispatch(
    request: Request<'_>,
    body: &[u8],
    jobs: &Jobs,
    workspace: Option<&mut Workspace>,
) -> Response {
    let Request {
        method,
        target,
        authorization,
        ..
    } = request;
    if let Some(path) = target.strip_prefix(workspace::ROOT) {
        return match workspace {
            Some(workspace) if !path.contains('?') && !path.contains('#') => workspace.dispatch(method, path, authorization, body),
            Some(_) => Response::json("400 Bad Request", r#"{"code":"invalid_request","message":"Use a versioned path without a query or fragment."}"#.into()),
            None => Response::json("404 Not Found", r#"{"code":"not_found","message":"Workspace mode is disabled."}"#.into()),
        };
    }
    match (method, target) {
        ("GET", "/") => return Response::new("200 OK", "text/html; charset=utf-8", INDEX),
        ("GET", "/workbench.css") => {
            return Response::new("200 OK", "text/css; charset=utf-8", STYLE)
        }
        ("GET", "/workbench.js") => {
            return Response::new("200 OK", "text/javascript; charset=utf-8", SCRIPT)
        }
        _ => {}
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let Some(endpoint) = path.strip_prefix(ROOT) else {
        return Response::error("404 Not Found", "Unknown workbench route.");
    };
    let result = handle_api(method, endpoint, query, jobs);
    match result {
        Ok((status, body)) => Response::json(status, body),
        Err(error) => Response::error("400 Bad Request", &error),
    }
}

fn fields<'a>(query: &'a str, expected: &[&str]) -> model::Result<BTreeMap<&'a str, &'a str>> {
    let mut values = BTreeMap::new();
    for pair in query.split('&').filter(|_| !query.is_empty()) {
        let (key, value) = pair.split_once('=').ok_or("Malformed query.")?;
        if !expected.contains(&key)
            || values.insert(key, value).is_some()
            || !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b":,-".contains(&b))
        {
            return Err("Unexpected query field or value.".into());
        }
    }
    if values.len() != expected.len() {
        return Err("Required query fields are missing.".into());
    }
    Ok(values)
}

fn handle_api(
    method: &str,
    endpoint: &str,
    query: &str,
    jobs: &Jobs,
) -> model::Result<(&'static str, String)> {
    let body = match (method, endpoint) {
        ("GET", "areas") | ("GET", "network") => {
            fields(query, &[])?;
            if endpoint == "areas" {
                model::areas()?
            } else {
                model::network()?
            }
        }
        ("GET", "study") | ("POST", "initialize") => {
            let v = fields(query, &["area", "date"])?;
            model::study(v["area"], v["date"], method == "POST")?
        }
        ("GET", "preview") | ("POST", "runs") => {
            let v = fields(query, &["projects", "width", "capital", "annual"])?;
            let plan = Plan::parse(v["projects"], v["width"], v["capital"], v["annual"])?;
            if method == "POST" {
                return Ok((
                    "202 Accepted",
                    format!(r#"{{"id":{},"status":"queued"}}"#, jobs.submit(plan)?),
                ));
            }
            model::preview(&plan)?
        }
        ("GET", "run") => {
            let v = fields(query, &["id"])?;
            jobs.get(v["id"])?.view()
        }
        ("GET", "comparison") => {
            let v = fields(
                query,
                &[
                    "id", "phase", "origin", "modes", "profile", "start", "end", "seconds",
                ],
            )?;
            let query = MapQuery::parse(
                v["origin"],
                v["modes"],
                v["profile"],
                v["start"],
                v["end"],
                v["seconds"],
            )?;
            let phase = v["phase"].parse().map_err(|_| "Invalid phase number.")?;
            let State::Completed(phases) = jobs.get(v["id"])?.state else {
                return Err("Wait for the run to complete before comparing maps.".into());
            };
            model::compare(&phases.phases, phase, &query)?
        }
        _ => {
            return Ok((
                "404 Not Found",
                r#"{"error":"Unknown workbench operation."}"#.into(),
            ))
        }
    };
    Ok(("200 OK", body))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait_for(jobs: &Jobs, id: &str, matches: impl Fn(State) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !matches(jobs.get(id).unwrap().state) {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
    }

    #[test]
    fn e17_s05_bounded_queue_reports_real_lifecycle_and_recovers() {
        let (release, gate) = mpsc::channel();
        let jobs = Jobs::start(move |plan| {
            gate.recv().unwrap();
            model::run(&plan)
        });
        let failing = Plan::parse("0", "2500", "1", "30").unwrap();
        let valid = Plan::parse("0", "2500", "200", "30").unwrap();
        assert_eq!(jobs.submit(failing).unwrap(), 1);
        wait_for(&jobs, "1", |state| matches!(state, State::Running));
        for id in 2..=5 {
            assert_eq!(jobs.submit(valid.clone()).unwrap(), id);
            assert!(matches!(
                jobs.get(&id.to_string()).unwrap().state,
                State::Queued
            ));
        }
        assert!(jobs.submit(valid).is_err());
        release.send(()).unwrap();
        wait_for(&jobs, "1", |state| matches!(state, State::Failed(_)));
        for _ in 0..4 {
            release.send(()).unwrap();
        }
        wait_for(&jobs, "5", |state| matches!(state, State::Completed(_)));
    }

    #[test]
    fn e17_http_rejects_foreign_writes_and_invalid_framing() {
        let host = "127.0.0.1:8047";
        let origin = "http://127.0.0.1:8047";
        let jobs = Jobs::start(|plan| model::run(&plan));
        let request = |header: &str| {
            let head = format!("POST /api/e17/v1/runs?projects=0&width=2500&capital=200&annual=30 HTTP/1.1\r\nHost: {host}\r\n{header}");
            match Request::parse(&head, host, origin) {
                Ok(request) => dispatch(request, &[], &jobs, None),
                Err(response) => response,
            }
            .status
        };
        for header in [
            "Origin: http://evil.example\r\nX-Carfree-Workbench: 1",
            "X-Carfree-Workbench: 1",
        ] {
            assert_eq!(request(header), "403 Forbidden");
        }
        assert_eq!(request("Transfer-Encoding: chunked"), "400 Bad Request");
        assert_eq!(request("Host: other.example"), "400 Bad Request");
        assert_eq!(
            request(&format!("Origin: {origin}\r\nX-Carfree-Workbench: 1")),
            "202 Accepted"
        );
        assert!(fields("date=1&date=2", &["date"]).is_err());
        assert!(fields("area=../../home", &["area"]).is_err());
    }
}
