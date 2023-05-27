use futures::stream::StreamExt;
use gcp_bigquery_client as bq;
use gcp_bigquery_client::model::table_data_insert_all_request::TableDataInsertAllRequest;
use jsonwebtoken as jwt;
use serde::{Deserialize, Serialize};
use signal_hook_tokio::Signals;
use std::collections::HashMap;
use std::env;
use warp::hyper::StatusCode;
use warp::Filter;

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct Event {
    event_id: String,
    target_name: String,
    event_type: String,
    timestamp: String,
    data: HashMap<String, serde_json::Value>,
}

#[derive(Deserialize, Clone, Debug)]
struct DTRequest {
    event: Event,
    labels: HashMap<String, String>,
}

#[derive(Serialize)]
struct BQRow<'a> {
    event_id: &'a str,
    target_name: &'a str,
    event_type: &'a str,
    timestamp: &'a str,
    data: &'a str,
    labels: &'a str,
}

#[derive(Deserialize)]
struct Claims {}

struct Config {
    dataset_id: String,
    project_id: String,
    table_id: String,
    jwt_key: String,
    bq_client: bq::Client,
}

#[tokio::main]
async fn main() {
    stackdriver_logger::init_with_cargo!();

    let bq_dataset = env::var("DATASET").expect("Missing DATASET");

    let bq_project = env::var("PROJECT_ID").expect("Missing PROJECT_ID");

    let bq_table = env::var("TABLE").expect("Missing TABLE");

    let signature_key = env::var("SIGNATURE").expect("Missing SIGNATURE env");

    let port: u16 = env::var("PORT")
        .expect("Missing PORT")
        .parse()
        .expect("PORT must be integer");

    // Initialize the BQ client
    let bq_client = bq::Client::from_application_default_credentials()
        .await
        .expect("Failed to create BQ client");

    let f = warp::path!("dtconn")
        .and(warp::post())
        .and(warp::body::json())
        .and(warp::header::<String>("x-dt-signature"))
        .and(warp::any().map(move || Config {
            dataset_id: bq_dataset.clone(),
            table_id: bq_table.clone(),
            project_id: bq_project.clone(),
            jwt_key: signature_key.clone(),
            bq_client: bq_client.clone(),
        }))
        .and_then(handler)
        .with(warp::log::custom(|info| {
            let mut level = log::Level::Info;
            if info.status().is_client_error() {
               level = log::Level::Warn; 
            } else if info.status().is_server_error() {
                level = log::Level::Error;
            }
            log::log!(level, "{} -> {} = {}", info.method(), info.path(), info.status())
        }));
    log::info!("Starting warp server");
    let server = warp::serve(f).run(([0, 0, 0, 0], port));
    let signals = Signals::new(&[signal_hook::consts::SIGINT]).expect("SIGINT should be supported");
    let handle = signals.handle();
    let mut signals = signals.fuse();

    tokio::select! {
        _ = server => {log::error!("Server error");}
        _ = signals.next() => {log::info!("SIGTERM");}
    }

    handle.close();
    log::info!("Shutting down, thanks for now! :)")
}

async fn handler(
    r: DTRequest,
    signature: String,
    config: Config,
) -> Result<impl warp::Reply, warp::Rejection> {
    if let Err(_) = jwt::decode::<Claims>(
        &signature,
        &jwt::DecodingKey::from_secret(config.jwt_key.as_bytes()),
        &jwt::Validation::new(jwt::Algorithm::HS256),
    ) {
        log::error!("Invalid signature");
        return Err(warp::reject());
    }

    let mut insert = TableDataInsertAllRequest::new();
    insert
        .add_row(
            Some(r.event.event_id.clone()),
            BQRow {
                event_id: &r.event.event_id,
                timestamp: &r.event.timestamp,
                target_name: &r.event.target_name,
                event_type: &r.event.event_type,
                data: &serde_json::to_string(&r.event.data).unwrap_or("{}".into()),
                labels: &serde_json::to_string(&r.labels).unwrap_or("{}".into()),
            },
        )
        .expect("Insert should never fail");
    let resp = config
        .bq_client
        .tabledata()
        .insert_all(
            &config.project_id,
            &config.dataset_id,
            &config.table_id,
            insert,
        )
        .await;

    resp.and(Ok(StatusCode::OK)).or(Err(warp::reject()))
}
