use super::*;
use parquet::{
    file::reader::{FileReader, SerializedFileReader},
    record::{Field, RowAccessor},
};
use std::{fs, io::Write};
use tempfile::{Builder, TempDir};

const OWNER: &str = "0101010101010101010101010101010101010101010101010101010101010101";
const PROJECT: &str = "projects/1";
const SCENARIOS: &str = "projects/1/scenarios";
const REVISIONS: &str = "projects/1/scenarios/1/revisions";
const REVISION: &str = "projects/1/scenarios/1/revisions/1";

fn fresh() -> (TempDir, Workspace) {
    let dir = Builder::new()
        .permissions({
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::Permissions::from_mode(0o700)
            }
            #[cfg(not(unix))]
            {
                fs::metadata(".").unwrap().permissions()
            }
        })
        .tempdir()
        .unwrap();
    let mut owner = tempfile::NamedTempFile::new_in(dir.path()).unwrap();
    owner.write_all(OWNER.as_bytes()).unwrap();
    owner.persist(dir.path().join("owner")).unwrap();
    let workspace = Workspace::open(dir.path(), &dir.path().join("owner")).unwrap();
    (dir, workspace)
}

fn action(
    workspace: &mut Workspace,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Value,
) -> Response {
    let raw = if body.is_null() {
        Vec::new()
    } else {
        serde_json::to_vec(&body).unwrap()
    };
    let auth = token.map(|value| format!("Bearer {value}"));
    workspace.dispatch(method, path, auth.as_deref(), &raw)
}

fn check(response: Response, status: &str) -> Value {
    assert_eq!(
        response.status,
        status,
        "{}",
        String::from_utf8_lossy(&response.body)
    );
    store::exact_json(std::str::from_utf8(&response.body).unwrap()).unwrap()
}

fn fixture_plan() -> Value {
    json!({"projects":[0],"width":1900,"capital":200,"annual":30})
}

fn setup(workspace: &mut Workspace) {
    let project =
        json!({"title":"Private workspace title","area":"place:4159000","date":"2026-01-05"});
    check(
        action(workspace, "POST", "projects", Some(OWNER), project),
        "201 Created",
    );
    check(
        action(
            workspace,
            "POST",
            SCENARIOS,
            Some(OWNER),
            json!({"title":"Private scenario title","plan":fixture_plan()}),
        ),
        "201 Created",
    );
}

#[test]
fn e19_s01_exact_inputs_and_results_reopen_and_stale_blocks_replay() {
    let (dir, mut workspace) = fresh();
    setup(&mut workspace);
    let project = check(
        action(&mut workspace, "GET", PROJECT, Some(OWNER), Value::Null),
        "200 OK",
    );
    let snapshot = project["city"]["projectTsv"].as_str().unwrap();
    assert_eq!(
        project["inputs"]["city_snapshot_sha256"],
        store::digest(snapshot.as_bytes())
    );
    let city = mobility_ingest::study_area::CityProject::from_tsv(snapshot).unwrap();
    assert_eq!(city.to_tsv(), snapshot);
    assert!(project["stale"].is_null());
    assert!(Workspace::open(dir.path(), &dir.path().join("owner")).is_err());
    let path = format!("{REVISION}/jobs");
    let evaluated = check(
        action(&mut workspace, "POST", &path, Some(OWNER), Value::Null),
        "200 OK",
    );
    assert_eq!(evaluated["status"], "completed");
    let eval_path = format!("{REVISION}/evaluation");
    let response = action(&mut workspace, "GET", &eval_path, Some(OWNER), Value::Null);
    let expected_bytes = response.body.clone();
    let result = check(response, "200 OK");
    assert_eq!(result["manifest"]["seed"], 42);
    assert_eq!(result["manifest"]["inputs"], project["inputs"]);
    drop(workspace);

    let mut workspace = Workspace::open(dir.path(), &dir.path().join("owner")).unwrap();
    assert_eq!(
        check(
            action(&mut workspace, "GET", PROJECT, Some(OWNER), Value::Null),
            "200 OK"
        ),
        project
    );
    assert_eq!(
        check(
            action(&mut workspace, "GET", &eval_path, Some(OWNER), Value::Null),
            "200 OK"
        ),
        result
    );
    assert_eq!(
        check(
            action(&mut workspace, "POST", &path, Some(OWNER), Value::Null),
            "200 OK"
        ),
        evaluated
    );
    assert_eq!(
        action(&mut workspace, "GET", &eval_path, Some(OWNER), Value::Null).body,
        expected_bytes
    );
    workspace.executable = "changed".into();
    assert_eq!(
        check(
            action(&mut workspace, "GET", PROJECT, Some(OWNER), Value::Null),
            "200 OK"
        )["stale"],
        "evaluator_changed"
    );
    check(
        action(&mut workspace, "POST", &path, Some(OWNER), Value::Null),
        "409 Conflict",
    );
    assert_eq!(
        check(
            action(&mut workspace, "GET", &eval_path, Some(OWNER), Value::Null),
            "200 OK"
        ),
        result
    );
    workspace.store.ledger.projects[0].city["projectTsv"] = json!("changed");
    assert_eq!(
        check(
            action(&mut workspace, "GET", PROJECT, Some(OWNER), Value::Null),
            "200 OK"
        )["stale"],
        "saved_city_snapshot_changed"
    );
}

#[test]
fn e19_s02_documented_api_validation_and_durable_interruption() {
    let (dir, mut workspace) = fresh();
    setup(&mut workspace);
    let schema = check(
        action(&mut workspace, "GET", "openapi.json", None, Value::Null),
        "200 OK",
    );
    let spec = schema["paths"].as_object().unwrap();
    assert_eq!(spec.len(), 21);
    assert_eq!(schema["openapi"], "3.1.0");
    let method_count: usize = spec
        .values()
        .map(|path| path.as_object().unwrap().len() - 1)
        .sum();
    assert_eq!(method_count, 25);
    assert_eq!(
        check(
            action(&mut workspace, "GET", "cities", None, Value::Null),
            "200 OK"
        )
        .as_array()
        .unwrap()
        .len(),
        3
    );
    check(
        action(
            &mut workspace,
            "GET",
            "cities/place:4159000/2026-02-31",
            None,
            Value::Null,
        ),
        "400 Bad Request",
    );
    check(
        action(
            &mut workspace,
            "GET",
            "cities/place:4159000/2026-01-05",
            None,
            Value::Null,
        ),
        "200 OK",
    );
    let invalid = check(
        action(
            &mut workspace,
            "POST",
            REVISIONS,
            Some(OWNER),
            json!({"projects":[0,0],"width":1900,"capital":200,"annual":30}),
        ),
        "400 Bad Request",
    );
    assert_eq!(invalid["code"], "invalid_request");
    let job = format!("{REVISION}/jobs");
    check(
        action(&mut workspace, "GET", &job, Some(OWNER), Value::Null),
        "404 Not Found",
    );
    check(
        action(&mut workspace, "POST", &job, Some(OWNER), json!({})),
        "400 Bad Request",
    );
    workspace
        .store
        .change(|ledger| {
            ledger.projects[0].scenarios[0].revisions[0].job = Some(Job::Running);
            Ok(())
        })
        .unwrap_or_else(|error| panic!("{}", error.2));
    drop(workspace);

    let mut workspace = Workspace::open(dir.path(), &dir.path().join("owner")).unwrap();
    assert_eq!(
        check(
            action(&mut workspace, "GET", &job, Some(OWNER), Value::Null),
            "200 OK"
        )["status"],
        "interrupted"
    );
    assert_eq!(
        check(
            action(&mut workspace, "POST", &job, Some(OWNER), Value::Null),
            "200 OK"
        )["status"],
        "completed"
    );

    let mut infeasible = fixture_plan();
    infeasible["capital"] = json!(0);
    check(
        action(&mut workspace, "POST", REVISIONS, Some(OWNER), infeasible),
        "201 Created",
    );
    let path = format!("{REVISIONS}/2/jobs");
    assert_eq!(
        check(
            action(&mut workspace, "POST", &path, Some(OWNER), Value::Null),
            "200 OK"
        )["status"],
        "failed"
    );
    assert!(check(
        action(&mut workspace, "GET", &path, Some(OWNER), Value::Null),
        "200 OK"
    )["error"]
        .is_string());
    check(
        action(
            &mut workspace,
            "GET",
            &format!("{REVISIONS}/2/evaluation"),
            Some(OWNER),
            Value::Null,
        ),
        "409 Conflict",
    );
}

#[test]
fn e19_s03_s06_parquet_matches_json_and_standalone_exports_show_limits() {
    let (_dir, mut workspace) = fresh();
    setup(&mut workspace);
    check(
        action(
            &mut workspace,
            "POST",
            &format!("{REVISION}/jobs"),
            Some(OWNER),
            Value::Null,
        ),
        "200 OK",
    );
    let mut download = |name| {
        action(
            &mut workspace,
            "GET",
            &format!("{REVISION}/exports/{name}"),
            Some(OWNER),
            Value::Null,
        )
    };
    let result = check(download("scenario.json"), "200 OK");
    let manifest = check(download("manifest.json"), "200 OK");
    assert_eq!(result["manifest"], manifest);
    for key in ["scenario", "report"] {
        assert_eq!(
            manifest[format!("{key}_sha256")],
            store::digest(result[key].to_string().as_bytes())
        );
    }

    let response = download("metrics.parquet");
    assert_eq!(response.mime, "application/vnd.apache.parquet");
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(&response.body).unwrap();
    let reader = SerializedFileReader::new(file).unwrap();
    let rows = reader
        .get_row_iter(None)
        .unwrap()
        .collect::<parquet::errors::Result<Vec<_>>>()
        .unwrap();
    let phases = result["report"]["phases"].as_array().unwrap();
    let metrics = result["report"]["metrics"].as_array().unwrap();
    assert_eq!(rows.len(), phases.len() * metrics.len());
    for (index, row) in rows.iter().enumerate() {
        let phase = index / metrics.len();
        let metric = index % metrics.len();
        assert_eq!(row.get_int(0).unwrap(), phase as i32);
        assert_eq!(*row.get_string(1).unwrap(), metrics[metric][0]);
        assert_eq!(*row.get_string(2).unwrap(), metrics[metric][1]);
        for (column, key) in ["values", "incremental", "cumulative"].iter().enumerate() {
            match phases[phase][key][metric].as_f64() {
                Some(value) => assert_eq!(row.get_double(column + 3).unwrap(), value),
                None => assert_eq!(
                    row.get_column_iter().nth(column + 3).unwrap().1,
                    &Field::Null
                ),
            }
        }
    }
    let metadata = reader
        .metadata()
        .file_metadata()
        .key_value_metadata()
        .unwrap();
    assert_eq!(
        metadata[0].value.as_deref(),
        Some(manifest.to_string().as_str())
    );
    assert_eq!(
        metadata[1].value.as_deref(),
        Some(fixture_plan().to_string().as_str())
    );
    for (name, mime, parts) in [
        (
            "map.svg",
            "image/svg+xml",
            ["<title", "access unknown", "no distance scale"],
        ),
        (
            "report.html",
            "text/html; charset=utf-8",
            ["<caption>", "2026 USD/year", "Change from baseline"],
        ),
    ] {
        let response = download(name);
        assert_eq!(response.status, "200 OK");
        assert_eq!(response.mime, mime);
        let text = String::from_utf8(response.body).unwrap();
        let trace = &phases[0]["trace"];
        for part in parts.into_iter().chain([
            "Uncalibrated synthetic fixture",
            trace["date"].as_str().unwrap(),
            trace["source"].as_str().unwrap(),
            trace["license"].as_str().unwrap(),
        ]) {
            assert!(text.contains(part), "Missing {part} in {name}");
        }
        assert!(!text.contains("<script"));
    }
}

#[test]
fn e19_s04_s05_s08_role_matrix_revisions_and_revocable_explicit_publication() {
    let (dir, mut workspace) = fresh();
    setup(&mut workspace);
    let members = format!("{PROJECT}/members");
    let editor = check(
        action(
            &mut workspace,
            "POST",
            &members,
            Some(OWNER),
            json!({"role":"editor"}),
        ),
        "201 Created",
    );
    let viewer = check(
        action(
            &mut workspace,
            "POST",
            &members,
            Some(OWNER),
            json!({"role":"viewer"}),
        ),
        "201 Created",
    );
    let editor = editor["token"].as_str().unwrap();
    let viewer_token = viewer["token"].as_str().unwrap();

    let second = json!({"title":"Other workspace","area":"place:4159000","date":"2026-01-05"});
    check(
        action(&mut workspace, "POST", "projects", Some(OWNER), second),
        "201 Created",
    );
    assert_eq!(
        check(
            action(
                &mut workspace,
                "GET",
                "projects",
                Some(viewer_token),
                Value::Null
            ),
            "200 OK"
        )["projects"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    check(
        action(
            &mut workspace,
            "GET",
            "projects/2",
            Some(viewer_token),
            Value::Null,
        ),
        "404 Not Found",
    );
    check(
        action(&mut workspace, "GET", REVISION, None, Value::Null),
        "401 Unauthorized",
    );
    check(
        action(
            &mut workspace,
            "GET",
            REVISION,
            Some(viewer_token),
            Value::Null,
        ),
        "200 OK",
    );
    check(
        action(&mut workspace, "GET", &members, Some(editor), Value::Null),
        "403 Forbidden",
    );
    check(
        action(
            &mut workspace,
            "POST",
            REVISIONS,
            Some(viewer_token),
            fixture_plan(),
        ),
        "403 Forbidden",
    );

    let comment = format!("{REVISION}/comments");
    let private = json!({"text":"Private comment <script> about the original revision."});
    check(
        action(
            &mut workspace,
            "POST",
            &comment,
            Some(viewer_token),
            private.clone(),
        ),
        "403 Forbidden",
    );
    check(
        action(
            &mut workspace,
            "POST",
            &comment,
            Some(editor),
            private.clone(),
        ),
        "201 Created",
    );
    let review = format!("{REVISION}/review");
    check(
        action(
            &mut workspace,
            "PUT",
            &review,
            Some(editor),
            json!({"state":"in_review"}),
        ),
        "200 OK",
    );
    check(
        action(
            &mut workspace,
            "PUT",
            &review,
            Some(editor),
            json!({"state":"approved"}),
        ),
        "403 Forbidden",
    );
    check(
        action(
            &mut workspace,
            "PUT",
            &review,
            Some(OWNER),
            json!({"state":"approved"}),
        ),
        "200 OK",
    );

    let publish = format!("{REVISION}/shares");
    check(
        action(
            &mut workspace,
            "POST",
            &publish,
            Some(editor),
            json!({"projection":"scenario"}),
        ),
        "403 Forbidden",
    );
    check(
        action(
            &mut workspace,
            "POST",
            &publish,
            Some(OWNER),
            json!({"projection":"report"}),
        ),
        "409 Conflict",
    );
    let shared = check(
        action(
            &mut workspace,
            "POST",
            &publish,
            Some(OWNER),
            json!({"projection":"scenario"}),
        ),
        "201 Created",
    );
    let link = shared["path"].as_str().unwrap().strip_prefix(ROOT).unwrap();
    let public = check(
        action(&mut workspace, "GET", link, None, Value::Null),
        "200 OK",
    );
    assert_eq!(public.as_object().unwrap().len(), 3);
    assert_eq!(public["plan"], fixture_plan());
    check(
        action(&mut workspace, "POST", link, None, Value::Null),
        "404 Not Found",
    );
    check(
        action(
            &mut workspace,
            "DELETE",
            &format!("projects/2/shares/{}", shared["id"].as_str().unwrap()),
            Some(OWNER),
            Value::Null,
        ),
        "404 Not Found",
    );

    let mut revised = fixture_plan();
    revised["width"] = json!(2100);
    check(
        action(&mut workspace, "POST", REVISIONS, Some(editor), revised),
        "201 Created",
    );
    drop(workspace);
    let mut workspace = Workspace::open(dir.path(), &dir.path().join("owner")).unwrap();
    assert_eq!(
        check(
            action(&mut workspace, "GET", link, None, Value::Null),
            "200 OK"
        ),
        public
    );
    let previous = check(
        action(
            &mut workspace,
            "GET",
            REVISION,
            Some(viewer_token),
            Value::Null,
        ),
        "200 OK",
    );
    assert_eq!(previous["review"], "approved");
    assert_eq!(previous["comments"][0]["text"], private["text"]);
    let next = check(
        action(
            &mut workspace,
            "GET",
            &format!("{REVISIONS}/2"),
            Some(viewer_token),
            Value::Null,
        ),
        "200 OK",
    );
    assert_eq!(next["review"], "draft");
    assert_eq!(next["comments"], json!([]));
    check(
        action(
            &mut workspace,
            "DELETE",
            &format!("{PROJECT}/shares/{}", shared["id"].as_str().unwrap()),
            Some(OWNER),
            Value::Null,
        ),
        "200 OK",
    );
    check(
        action(&mut workspace, "GET", link, None, Value::Null),
        "404 Not Found",
    );
    check(
        action(
            &mut workspace,
            "DELETE",
            &format!("{members}/{}", viewer["id"].as_str().unwrap()),
            Some(OWNER),
            Value::Null,
        ),
        "200 OK",
    );
    check(
        action(
            &mut workspace,
            "GET",
            REVISION,
            Some(viewer_token),
            Value::Null,
        ),
        "404 Not Found",
    );
    let disk = fs::read_to_string(dir.path().join("workspace-v1.json")).unwrap();
    assert!(!disk.contains(editor));
    assert!(!disk.contains(OWNER));
}

#[test]
fn e19_s07_published_extension_examples_and_compatibility() {
    let (_dir, mut workspace) = fresh();
    let contracts = check(
        action(&mut workspace, "GET", "extensions", None, Value::Null),
        "200 OK",
    );
    for key in ["dataset-upload-v1", "optimizer-plan-v1"] {
        let example = contracts["contracts"][key]["example"].clone();
        check(
            action(
                &mut workspace,
                "POST",
                "extensions/validate",
                None,
                example.clone(),
            ),
            "200 OK",
        );
        let mut future = example.clone();
        future["version"] = json!(2);
        check(
            action(&mut workspace, "POST", "extensions/validate", None, future),
            "400 Bad Request",
        );
        let mut wrong = example;
        wrong["sample"] = json!({});
        check(
            action(&mut workspace, "POST", "extensions/validate", None, wrong),
            "400 Bad Request",
        );
    }
    let mut example = contracts["contracts"]["optimizer-plan-v1"]["example"].clone();
    example["sample"]["capital"] = json!(0);
    check(
        action(&mut workspace, "POST", "extensions/validate", None, example),
        "400 Bad Request",
    );
    let mut example = contracts["contracts"]["dataset-upload-v1"]["example"].clone();
    example["sample"]["input"]["path"] = json!("../counts.csv");
    check(
        action(&mut workspace, "POST", "extensions/validate", None, example),
        "400 Bad Request",
    );
}

#[test]
fn e19_s01_s03_changed_saved_metric_fails_integrity_on_reopen() {
    let (dir, mut workspace) = fresh();
    setup(&mut workspace);
    let job = format!("{REVISION}/jobs");
    assert_eq!(
        check(
            action(&mut workspace, "POST", &job, Some(OWNER), Value::Null),
            "200 OK"
        )["status"],
        "completed"
    );
    drop(workspace);

    let path = dir.path().join("workspace-v1.json");
    let mut ledger = store::exact_json(&fs::read_to_string(&path).unwrap()).unwrap();
    ledger["projects"][0]["scenarios"][0]["revisions"][0]["job"]["completed"]["result"]["report"]
        ["phases"][0]["values"][0] = json!(91.125);
    fs::write(path, ledger.to_string()).unwrap();
    let error = Workspace::open(dir.path(), &dir.path().join("owner"))
        .err()
        .unwrap();
    assert!(error.to_string().contains("integrity check failed"));
}

#[test]
fn e19_s02_full_store_reports_a_finished_job_save_failure() {
    let (dir, mut workspace) = fresh();
    setup(&mut workspace);
    let existing = fs::metadata(dir.path().join("workspace-v1.json"))
        .unwrap()
        .len();
    let comment = Comment {
        role: Role::Owner,
        member: "owner".into(),
        text: "x".repeat(2000),
    };
    let bytes_per_comment = serde_json::to_vec(&comment).unwrap().len() as u64 + 1;
    let count = ((store::MAX_BYTES - existing - 1024) / bytes_per_comment) as usize;
    workspace
        .store
        .change(|ledger| {
            ledger.projects[0].scenarios[0].revisions[0].comments = vec![comment; count];
            Ok(())
        })
        .unwrap_or_else(|error| panic!("{}", error.2));

    let path = format!("{REVISION}/jobs");
    check(
        action(&mut workspace, "POST", &path, Some(OWNER), Value::Null),
        "409 Conflict",
    );
    let failed = check(
        action(&mut workspace, "GET", &path, Some(OWNER), Value::Null),
        "200 OK",
    );
    assert_eq!(failed["status"], "failed");
    assert!(failed["error"]
        .as_str()
        .unwrap()
        .contains("result could not be saved"));
    drop(workspace);
    let mut reopened = Workspace::open(dir.path(), &dir.path().join("owner")).unwrap();
    assert_eq!(
        check(
            action(&mut reopened, "GET", &path, Some(OWNER), Value::Null),
            "200 OK"
        ),
        failed
    );
}

#[test]
fn e19_s01_decimal_decoding_does_not_change_existing_ingest_parser() {
    let text = "51.248178375505404";
    let expected = text.parse::<f64>().unwrap();
    assert_eq!(store::exact_json(text).unwrap().as_f64(), Some(expected));
    let legacy: Value = serde_json::from_str(text).unwrap();
    assert_ne!(legacy.as_f64(), Some(expected));
}
