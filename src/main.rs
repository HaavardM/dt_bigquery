use futures::stream::StreamExt;
use gcp_bigquery_client as bq;
use gcp_bigquery_client::model::table_data_insert_all_request::TableDataInsertAllRequest;
use jsonwebtoken as jwt;
use serde::{Deserialize, Serialize};
use signal_hook_tokio::Signals;
use std::collections::HashMap;
use warp::hyper::StatusCode;
use warp::Filter;

use envconfig::Envconfig;

#[derive(Envconfig, Clone)]
struct Config {
    #[envconfig(from = "DATASET")]
    pub bq_dataset: String,

    #[envconfig(from = "PROJECT_ID")]
    pub bq_project: String,

    #[envconfig(from = "TABLE")]
    pub bq_table: String,

    #[envconfig(from = "SIGNATURE")]
    pub signature_secret: String,

    #[envconfig(from = "PORT", default = "8080")]
    pub port: u16,
}

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

#[tokio::main]
async fn main() {
    stackdriver_logger::init_with_cargo!();

    let config = Config::init_from_env().unwrap();
    let port = config.port;

    // Initialize the BQ client
    let bq_client = bq::Client::from_application_default_credentials()
        .await
        .expect("Failed to create BQ client");
    // Configure the HTTP server to accept events from the DT data-connector
    let f = warp::path!("dtconn")
        .and(warp::post())
        .and(warp::body::json())
        .and(warp::header::<String>("x-dt-signature"))
        .and(warp::any().map(move || bq_client.clone()))
        .and(warp::any().map(move || config.clone()))
        .and_then(handler)
        .with(warp::log::custom(|info| {
            let mut level = log::Level::Info;
            if info.status().is_client_error() {
                level = log::Level::Warn;
            } else if info.status().is_server_error() {
                level = log::Level::Error;
            }
            log::log!(
                level,
                "{} -> {} = {}",
                info.method(),
                info.path(),
                info.status()
            )
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
    client: bq::Client,
    config: Config,
) -> Result<impl warp::Reply, warp::Rejection> {
    if let Err(_) = jwt::decode::<Claims>(
        &signature,
        &jwt::DecodingKey::from_secret(config.signature_secret.as_bytes()),
        &jwt::Validation::new(jwt::Algorithm::HS256),
    ) {
        log::error!("invalid signature");
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
    let resp = client
        .tabledata()
        .insert_all(
            &config.bq_project,
            &config.bq_dataset,
            &config.bq_table,
            insert,
        )
        .await;

    resp.and(Ok(StatusCode::OK)).or(Err(warp::reject()))
}
