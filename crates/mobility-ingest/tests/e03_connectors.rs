use mobility_ingest::connectors::{inspect, Request};
use serde_json::{json, Value};
use std::{fs, path::Path};
use tempfile::TempDir;

fn write(temp: &TempDir, name: &str, bytes: impl AsRef<[u8]>) {
    fs::write(temp.path().join(name), bytes).unwrap();
}

fn request(input: Value) -> Request {
    serde_json::from_value(json!({
        "schema_version": 1,
        "source": {
            "id": "synthetic-e03", "version": "demo-1", "uri": "urn:fixture:e03",
            "attribution": "Synthetic test fixture", "license_terms": "Test fixture",
            "retrieved_on": "2026-01-29", "coverage": "Synthetic only",
            "geography": "Portland test area", "review": "pending"
        },
        "input": input
    }))
    .unwrap()
}

fn bundle(root: &Path, value: Value) -> Value {
    let result = inspect(&request(value), root, "2026-02-01").unwrap();
    assert!(
        result.report.errors.is_empty(),
        "{:?}",
        result.report.errors
    );
    result.data
}

#[test]
fn e03_s01_osm_preserves_segment_tags_and_turn_members() {
    let temp = TempDir::new().unwrap();
    write(
        &temp,
        "roads.osm",
        r#"<osm version="0.6">
        <node id="1" lat="45.0" lon="-122.1"/><node id="2" lat="45.1" lon="-122.1"><tag k="barrier" v="gate"/></node>
        <node id="3" lat="45.2" lon="-122.1"/>
        <way id="4"><nd ref="1"/><nd ref="2"/><tag k="highway" v="service"/><tag k="name" v="A &amp; B"/></way>
        <way id="5"><nd ref="2"/><nd ref="3"/><tag k="highway" v="cycleway"/><tag k="motor_vehicle" v="no"/></way>
        <relation id="9"><member type="way" ref="4" role="from"/><member type="node" ref="2" role="via"/><member type="way" ref="5" role="to"/><tag k="type" v="restriction"/><tag k="restriction" v="no_left_turn"/></relation>
        </osm>"#,
    );
    let input = json!({"kind":"osm","path":"roads.osm"});
    let data = bundle(temp.path(), input.clone());
    assert_eq!(data["ways"]["4"]["tags"]["name"], "A & B");
    assert_eq!(data["nodes"]["2"]["tags"]["barrier"], "gate");
    assert_eq!(data["ways"]["5"]["segments"][0]["to"]["id"], 3);
    assert_eq!(data["restrictions"]["9"]["members"][1]["role"], "via");
    let bad = fs::read_to_string(temp.path().join("roads.osm"))
        .unwrap()
        .replace("ref=\"3\"", "ref=\"99\"");
    write(&temp, "roads.osm", bad);
    let report = inspect(&request(input), temp.path(), "2026-02-01").unwrap();
    assert!(!report.report.errors.is_empty());
}

#[test]
fn e03_s03_census_keeps_leading_zeros_and_od_endpoints() {
    let temp = TempDir::new().unwrap();
    write(
        &temp,
        "population.json",
        r#"[["P1_001N","state","county","tract","block"],["12","01","003","010001","1000"]]"#,
    );
    let census = bundle(
        temp.path(),
        json!({"kind":"census_population","path":"population.json","geography_vintage":2020,"level":"block"}),
    );
    assert_eq!(census["rows"][0]["geoid"], "010030100011000");
    write(
        &temp,
        "od.csv",
        "w_geocode,h_geocode,S000,createdate\n010030100011001,010030100011000,5,20250101\n",
    );
    let od = bundle(
        temp.path(),
        json!({"kind":"lodes","path":"od.csv","geography_vintage":2020,"reference_year":2022,"job_type":"JT00","segment":"main"}),
    );
    assert_eq!(od["rows"][0]["home_geoid"], "010030100011000");
    assert_eq!(od["rows"][0]["work_geoid"], "010030100011001");
    assert_eq!(od["unit"], "jobs");
}

#[test]
fn e03_s04_destinations_dedup_conservatively_and_keep_trace() {
    let temp = TempDir::new().unwrap();
    let point = json!({"type":"Feature","id":"a","properties":{"amenity":"school","name":"School"},"geometry":{"type":"Point","coordinates":[-122,45]}});
    let polygon = json!({"type":"Feature","id":"b","properties":{"landuse":"residential"},"geometry":{"type":"Polygon","coordinates":[[[-122,45],[-121,45],[-121,46],[-122,45]]]}});
    write(
        &temp,
        "places.json",
        json!({"type":"FeatureCollection","features":[point,point,polygon]}).to_string(),
    );
    let data = bundle(temp.path(), json!({"kind":"places","path":"places.json"}));
    assert_eq!(data["destinations"][0]["category"], "education");
    assert_eq!(data["destinations"][0]["source_rows"], json!([1, 2]));
    assert_eq!(data["land_use"][0]["category"], "residential");
}

#[test]
fn e03_s07_inventory_records_review_and_deterministic_freshness() {
    let temp = TempDir::new().unwrap();
    write(&temp, "rows.csv", "name\nClinic\n");
    let input = json!({"kind":"custom","path":"rows.csv","format":"csv","fields":{"label":{"column":"name","type":"text","required":true}}});
    let result = inspect(&request(input), temp.path(), "2026-02-01").unwrap();
    let inventory = serde_json::to_value(&result.inventory).unwrap();
    assert_eq!(inventory["source"]["review"], "pending");
    assert_eq!(inventory["source"]["retrieved_on"], "2026-01-29");
    assert_eq!(inventory["age_days"], 3);
}

#[test]
fn e03_s02_gtfs_keeps_exception_only_service_after_midnight_and_realtime() {
    use gtfs_realtime::{FeedEntity, FeedHeader, FeedMessage, TripDescriptor, TripUpdate};
    use prost::Message;
    let temp = TempDir::new().unwrap();
    fs::create_dir(temp.path().join("feed")).unwrap();
    for (file, data) in [
        ("agency.txt", "agency_name,agency_url,agency_timezone\nTest Transit,https://example.test,America/Los_Angeles\n"),
        ("routes.txt", "route_id,route_type,route_short_name\nr,3,R\n"),
        ("stops.txt", "stop_id,stop_name,stop_lat,stop_lon,location_type\na,First,45,-122,0\nb,Second,45.1,-122.1,0\nsn,Station,45.1,-122.1,1\n"),
        ("trips.txt", "route_id,service_id,trip_id\nr,s,t\n"),
        ("calendar_dates.txt", "service_id,date,exception_type\ns,20260201,1\n"),
        ("stop_times.txt", "trip_id,arrival_time,departure_time,stop_id,stop_sequence\nt,25:01:00,25:02:00,a,1\nt,25:10:00,25:10:00,b,2\n"),
        ("transfers.txt", "from_stop_id,to_stop_id,transfer_type,min_transfer_time\na,b,2,120\nsn,b,,\n"),
        ("frequencies.txt", "trip_id,start_time,end_time,headway_secs\nt,25:00:00,26:00:00,600\n"),
    ] {
        write(&temp, &format!("feed/{file}"), data);
    }
    let input = json!({"kind":"gtfs","directory":"feed"});
    let data = bundle(temp.path(), input.clone());
    assert!(data["realtime"].is_null());
    assert_eq!(
        data["schedule"]["calendar_dates.txt"][0]["fields"]["exception_type"],
        "1"
    );
    assert_eq!(
        data["schedule"]["stop_times.txt"][0]["fields"]["departure_time"],
        "25:02:00"
    );
    assert_eq!(
        data["schedule"]["transfers.txt"][0]["fields"]["min_transfer_time"],
        "120"
    );
    assert_eq!(
        data["schedule"]["frequencies.txt"][0]["fields"]["headway_secs"],
        "600"
    );
    let snapshot = FeedMessage {
        header: FeedHeader {
            gtfs_realtime_version: "2.0".into(),
            timestamp: Some(1_769_904_000),
            ..Default::default()
        },
        entity: vec![FeedEntity {
            id: "update".into(),
            trip_update: Some(TripUpdate {
                trip: TripDescriptor {
                    trip_id: Some("t".into()),
                    ..Default::default()
                },
                delay: Some(60),
                ..Default::default()
            }),
            ..Default::default()
        }],
    };
    write(&temp, "feed.pb", snapshot.encode_to_vec());
    let mut req = request(input);
    let mut source = req.source.clone();
    source.id = "realtime-test".into();
    if let mobility_ingest::connectors::Input::Gtfs { realtime, .. } = &mut req.input {
        *realtime = Some(mobility_ingest::connectors::RealtimeInput {
            path: "feed.pb".into(),
            source,
        });
    }
    let candidate = inspect(&req, temp.path(), "2026-02-01").unwrap();
    assert!(candidate.is_valid(), "{:?}", candidate.report);
    assert_eq!(
        candidate.data["realtime"]["entity"][0]["trip_update"]["delay"],
        60
    );
    assert_eq!(
        candidate.realtime_inventory.unwrap().source.id,
        "realtime-test"
    );

    write(&temp, "feed/stop_times.txt", "trip_id,arrival_time,departure_time,stop_id,stop_sequence\nt,25:01:00,25:02:00,sn,1\nt,25:10:00,25:10:00,b,2\n");
    write(&temp, "feed.pb", b"\xff\xff");
    write(
        &temp,
        "feed/transfers.txt",
        "from_stop_id,to_stop_id,transfer_type\na,missing,0\n",
    );
    let invalid = inspect(&req, temp.path(), "2026-02-01").unwrap();
    assert!(invalid.data.is_null());
    assert!(invalid.report.errors.iter().any(
        |e| e.input.ends_with("stop_times.txt") && e.message.contains("invalid location_type")
    ));
    assert!(invalid
        .report
        .errors
        .iter()
        .any(|e| e.input.ends_with("transfers.txt") && e.row == Some(2)));
    assert!(invalid
        .report
        .errors
        .iter()
        .any(|e| e.message.contains("malformed GTFS-Realtime")));
}

#[test]
fn e03_s05_dot_preserves_count_and_speed_units_offset_and_quality() {
    let temp = TempDir::new().unwrap();
    write(
        &temp,
        "dot.csv",
        concat!(
            "id,x,y,time,window,metric,value,u,flags\n",
            "station-1,-122,45,2026-01-01T08:15:00-08:00,900,count,125,vehicles,complete\n",
            "station-1,-122,45,2026-01-01T08:15:00-08:00,900,speed,32.5,mph,provisional\n",
        ),
    );
    let input = json!({"kind":"dot","path":"dot.csv","columns":{
        "station":"id","longitude":"x","latitude":"y","timestamp":"time","duration_seconds":"window",
        "metric":"metric","value":"value","unit":"u","quality":"flags"
    }});
    let data = bundle(temp.path(), input);
    assert_eq!(data["rows"][0]["value"], 125);
    assert_eq!(data["rows"][0]["duration_seconds"], 900);
    assert_eq!(data["rows"][1]["coordinates"], json!([-122.0, 45.0]));
    assert_eq!(data["rows"][1]["timestamp"], "2026-01-01T08:15:00-08:00");
    assert_eq!(data["rows"][1]["value"], 32.5);
    assert_eq!(data["rows"][1]["unit"], "mph");
    assert_eq!(data["rows"][1]["quality"], "provisional");
}

#[test]
fn e03_s06_crash_severity_and_emissions_basis_remain_separate() {
    let temp = TempDir::new().unwrap();
    write(
        &temp,
        "crashes.csv",
        concat!(
            "case,x,y,date,status,uncertainty\n",
            "a,-122,45,2026-01-01,F,Location approximate\n",
            "b,-122,45,2026-01-01,N,Unknown\n",
            "c,-122,45,2026-01-02,U,Pending report\n",
        ),
    );
    let crashes = bundle(
        temp.path(),
        json!({"kind":"crashes","path":"crashes.csv","columns":{
            "event_id":"case","longitude":"x","latitude":"y","date":"date","severity":"status",
            "uncertainty":"uncertainty","severity_codes":{"fatal":["F"],"nonfatal":["N"],"unknown":["U"]}
        }}),
    );
    assert_eq!(crashes["rows"][0]["severity"], "fatal");
    assert_eq!(crashes["rows"][1]["severity"], "nonfatal");
    assert_eq!(crashes["rows"][2]["severity"], "unknown");
    assert_eq!(crashes["rows"][0]["source_severity"], "F");
    assert_eq!(crashes["rows"][0]["uncertainty"], "Location approximate");
    write(&temp, "emissions.csv", "area,date,pollutant,value,unit,basis,uncertainty\nx,2026-01-01,NOx,32,kg,modeled,95% interval 25–39 kg\n");
    let emissions = bundle(
        temp.path(),
        json!({"kind":"emissions","path":"emissions.csv","columns":{
            "area_id":"area","date":"date","pollutant":"pollutant","value":"value","unit":"unit",
            "basis":"basis","uncertainty":"uncertainty"
        }}),
    );
    assert_eq!(emissions["rows"][0]["basis"], "modeled");
    assert_eq!(emissions["rows"][0]["uncertainty"], "95% interval 25–39 kg");
}

fn custom(format: &str, path: &str) -> Value {
    json!({"kind":"custom","path":path,"format":format,"fields":{
        "count":{"column":"n","type":"integer","required":true}
    }})
}

#[test]
fn e03_s08_parquet_decodes_real_typed_compressed_columns() {
    use parquet::{
        basic::Compression,
        data_type::Int64Type,
        file::{properties::WriterProperties, writer::SerializedFileWriter},
        schema::parser::parse_message_type,
    };
    use std::sync::Arc;
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("counts.parquet");
    let schema = Arc::new(parse_message_type("message counts { REQUIRED INT64 n; }").unwrap());
    let properties = Arc::new(
        WriterProperties::builder()
            .set_compression(Compression::SNAPPY)
            .build(),
    );
    let mut writer =
        SerializedFileWriter::new(fs::File::create(path).unwrap(), schema, properties).unwrap();
    let mut group = writer.next_row_group().unwrap();
    let mut column = group.next_column().unwrap().unwrap();
    let count = 9_007_199_254_740_993_i64;
    column
        .typed::<Int64Type>()
        .write_batch(&[count, 2], None, None)
        .unwrap();
    column.close().unwrap();
    group.close().unwrap();
    writer.close().unwrap();
    let data = bundle(temp.path(), custom("parquet", "counts.parquet"));
    assert_eq!(data["rows"][0]["fields"]["count"], count);
    assert_eq!(data["rows"][1]["fields"]["count"], 2);
    let path = temp.path().join("nulls.parquet");
    let schema = Arc::new(parse_message_type("message counts { OPTIONAL INT64 n; }").unwrap());
    let mut writer =
        SerializedFileWriter::new(fs::File::create(path).unwrap(), schema, Default::default())
            .unwrap();
    let mut group = writer.next_row_group().unwrap();
    let mut column = group.next_column().unwrap().unwrap();
    column
        .typed::<Int64Type>()
        .write_batch(&[], Some(&[0]), None)
        .unwrap();
    column.close().unwrap();
    group.close().unwrap();
    writer.close().unwrap();
    let mut incompatible = custom("parquet", "nulls.parquet");
    incompatible["fields"]["count"] = json!({"column":"n","type":"text","required":false});
    let invalid = inspect(&request(incompatible), temp.path(), "2026-02-01").unwrap();
    assert!(invalid.report.errors[0].message.contains("incompatible"));
    write(&temp, "fake.parquet", "n\n2\n");
    let invalid = inspect(
        &request(custom("parquet", "fake.parquet")),
        temp.path(),
        "2026-02-01",
    )
    .unwrap();
    assert!(!invalid.is_valid());
}

#[test]
fn e03_s08_cli_previews_every_row_and_rolls_back_without_editing_e02_project() {
    use mobility_ingest::{
        study_area::{CatalogueImporter, CityProject, StudyArea},
        Catalogue,
    };
    use std::process::Command;
    let temp = TempDir::new().unwrap();
    let catalogue = Catalogue::pilot().unwrap();
    let area = StudyArea::from_geography(catalogue.get("place:4159000").unwrap()).unwrap();
    let snapshot = CityProject::initialize(
        "portland",
        area,
        "2026-02-01",
        &mut CatalogueImporter::new(&catalogue),
    )
    .unwrap();
    let original = snapshot.to_tsv();
    let project = temp.path().join("project.tsv");
    let sidecar = temp.path().join("project.tsv.upload.json");
    write(&temp, "project.tsv", &original);
    write(
        &temp,
        "request.json",
        serde_json::to_vec(&request(custom("csv", "counts.csv"))).unwrap(),
    );
    let invoke = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_ingest-data"))
            .current_dir(temp.path())
            .args(args)
            .output()
            .unwrap()
    };
    write(&temp, "counts.csv", "n\n1\n2\n3\n4\n5\ninvalid\n");
    let preview = invoke(&["preview", "request.json", "2026-02-01"]);
    assert_eq!(preview.status.code(), Some(2));
    let report: Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert_eq!(report["report"]["errors"][0]["row"], 7);
    assert!(report["data"].is_null());
    let apply = invoke(&["apply", "request.json", "2026-02-01", "project.tsv"]);
    assert_eq!(apply.status.code(), Some(2));
    assert!(!sidecar.exists());
    assert_eq!(fs::read(&project).unwrap(), original.as_bytes());

    write(
        &temp,
        "counts.csv",
        "n\r\n\r\n1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n",
    );
    let preview = invoke(&["preview", "request.json", "2026-02-01"]);
    assert!(preview.status.success());
    let report: Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert_eq!(report["data"]["rows"]["count"], 6);
    assert_eq!(report["data"]["rows"]["sample"][0]["source_row"], 3);
    assert_eq!(
        report["data"]["rows"]["sample"].as_array().unwrap().len(),
        5
    );
    assert!(!sidecar.exists());
    let apply = invoke(&["apply", "request.json", "2026-02-01", "project.tsv"]);
    assert!(
        apply.status.success(),
        "{}",
        String::from_utf8_lossy(&apply.stderr)
    );
    assert_eq!(fs::read(&project).unwrap(), original.as_bytes());
    assert!(sidecar.exists());
    let installed = fs::read(&sidecar).unwrap();
    assert_ne!(
        invoke(&["apply", "request.json", "2026-02-01", "project.tsv"])
            .status
            .code(),
        Some(0)
    );
    assert_eq!(installed, fs::read(&sidecar).unwrap());
    assert!(invoke(&["active", "project.tsv"]).status.success());
    let mut altered: Value = serde_json::from_slice(&installed).unwrap();
    altered["candidate"]["data"]["rows"][0]["fields"]["count"] = json!("wrong type");
    write(&temp, "project.tsv.upload.json", altered.to_string());
    let invalid = invoke(&["active", "project.tsv"]);
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("differs from its schema"));
    assert!(!invoke(&["rollback", "project.tsv"]).status.success());
    write(&temp, "project.tsv.upload.json", installed);
    assert!(invoke(&["rollback", "project.tsv"]).status.success());
    assert!(invoke(&["rollback", "project.tsv"]).status.success());
    assert!(!sidecar.exists());
    assert_eq!(fs::read(&project).unwrap(), original.as_bytes());

    write(
        &temp,
        "project.tsv.upload.json",
        "unrelated preexisting file",
    );
    assert!(!invoke(&["rollback", "project.tsv"]).status.success());
    assert_eq!(
        fs::read_to_string(&sidecar).unwrap(),
        "unrelated preexisting file"
    );
}

#[test]
fn e03_s08_geojson_maps_native_properties_and_geometry() {
    let temp = TempDir::new().unwrap();
    let geometry = json!({"type":"Point","coordinates":[-122,45]});
    write(
        &temp,
        "sample.geojson",
        json!({"type":"FeatureCollection","features":[{
            "type":"Feature","geometry":geometry,"properties":{"n":4}
        }]})
        .to_string(),
    );
    let mut input = custom("geojson", "sample.geojson");
    input["fields"]["location"] = json!({"column":"@geometry","type":"json","required":true});
    let result = bundle(temp.path(), input);
    assert_eq!(result["rows"][0]["fields"]["count"], 4);
    assert_eq!(result["rows"][0]["fields"]["location"], geometry);
}

#[test]
fn e03_s08_rejects_input_path_traversal() {
    let temp = TempDir::new().unwrap();
    let result = inspect(
        &request(custom("csv", "../rows.csv")),
        temp.path(),
        "2026-02-01",
    )
    .unwrap();
    assert!(result.report.errors[0]
        .message
        .contains("without traversal"));
    assert!(result.data.is_null());
}

#[test]
fn e03_s07_inventory_rejects_conflicting_source_versions() {
    use mobility_ingest::connectors::inventory;
    let original = request(custom("csv", "sample.csv"));
    let mut duplicate = original.clone();
    assert_eq!(
        inventory(&[original.clone(), duplicate.clone()], "2026-02-01")
            .unwrap()
            .len(),
        1
    );
    duplicate.source.license_terms = "Different terms".into();
    assert!(inventory(&[original.clone(), duplicate], "2026-02-01")
        .unwrap_err()
        .to_string()
        .contains("conflicting"));
    assert!(inventory(&[original], "2026-01-28")
        .unwrap_err()
        .to_string()
        .contains("after"));
}

#[test]
fn e03_s08_validates_csv_quotes_without_losing_quoted_line_breaks() {
    let temp = TempDir::new().unwrap();
    let input = json!({"kind":"custom","path":"sample.csv","format":"csv","fields":{
        "label":{"column":"name","type":"text","required":true}
    }});
    write(
        &temp,
        "sample.csv",
        "name\r\n\"Clinic, East \"\"Annex\"\"\r\n2nd floor\"\r\n",
    );
    let data = bundle(temp.path(), input.clone());
    assert_eq!(data["rows"][0]["source_row"], 2);
    assert_eq!(
        data["rows"][0]["fields"]["label"],
        "Clinic, East \"Annex\"\r\n2nd floor"
    );
    write(&temp, "sample.csv", "name\n\"Incomplete\n");
    let result = inspect(&request(input), temp.path(), "2026-02-01").unwrap();
    assert!(!result.is_valid());
    assert!(result.report.errors[0]
        .message
        .contains("unterminated quoted CSV field"));
    write(&temp, "sample.csv", "name\rClinic\rLibrary\r");
    let result = inspect(
        &request(
            json!({"kind":"custom","path":"sample.csv","format":"csv","fields":{
                "label":{"column":"name","type":"text","required":true}
            }}),
        ),
        temp.path(),
        "2026-02-01",
    )
    .unwrap();
    assert!(!result.is_valid());
    assert!(result.report.errors[0].message.contains("bare CR"));
}

#[test]
fn e03_s08_preview_rejects_json_too_deep_for_a_staged_upload() {
    let temp = TempDir::new().unwrap();
    write(
        &temp,
        "rows.csv",
        format!("raw\n{}0{}\n", "[".repeat(33), "]".repeat(33)),
    );
    let req = request(
        json!({"kind":"custom","path":"rows.csv","format":"csv","fields":{
            "nested":{"column":"raw","type":"json","required":true}
        }}),
    );
    let result = inspect(&req, temp.path(), "2026-02-01").unwrap();
    assert!(!result.is_valid());
    assert!(result.report.errors[0].message.contains("nesting limit"));
}
