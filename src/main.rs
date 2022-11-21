use axum::extract::DefaultBodyLimit;
use axum::{
    extract::Extension,
    handler::Handler,
    middleware,
    routing::{get, post},
    Router,
};
use chrono::Local;
use clap::{crate_name, crate_version, Arg, Command};
use env_logger::{Builder, Target};
use log::LevelFilter;
use mongodb::options::ClientOptions;
use mongodb::Client;
use std::future::ready;
use std::io::Write;
use std::net::SocketAddr;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;

mod error;
mod handlers;
mod metrics;
mod secret;
mod state;

use crate::metrics::{setup_metrics_recorder, track_metrics};
use handlers::{cache_get, cache_set, handler_404, health, root};
use state::State;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let opts = Command::new(crate_name!())
        .version(crate_version!())
        .author("")
        .about(crate_name!())
        .arg(
            Arg::new("port")
                .short('p')
                .long("port")
                .help("Set port to listen on")
                .env("TACKD_PORT")
                .default_value("8080")
                .takes_value(true),
        )
        .arg(
            Arg::new("url")
                .short('u')
                .long("url")
                .help("Declare url")
                .env("TACKD_EXTERNAL_URL")
                .default_value("http://localhost:8080")
                .takes_value(true),
        )
        .arg(
            Arg::new("database")
                .short('d')
                .long("database")
                .help("MongoDB Database")
                .env("TACKD_MONGODB_DATABASE")
                .default_value("tackd")
                .takes_value(true),
        )
        .arg(
            Arg::new("collection")
                .short('c')
                .long("collection")
                .help("MongoDB Metadata Collection")
                .env("TACKD_MONGODB_COLLECTION")
                .default_value("uploads")
                .takes_value(true),
        )
        .arg(
            Arg::new("admin")
                .short('a')
                .long("admin")
                .help("MongoDB Admin Collection")
                .env("TACKD_MONGODB_ADMIN_COLLECTION")
                .default_value("admin")
                .takes_value(true),
        )
        .arg(
            Arg::new("mongo")
                .short('m')
                .long("mongo")
                .help("MongoDB connection url")
                .env("TACKD_MONGODB_URL")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::new("bucket")
                .short('b')
                .long("bucket")
                .help("Bucket name")
                .env("TACKD_BUCKET")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::new("limit")
                .short('l')
                .long("limit")
                .help("Set the max payload size in bytes")
                .env("TACKD_UPLOAD_LIMIT")
                .default_value("10485760")
                .takes_value(true),
        )
        .get_matches();

    // Initialize log Builder
    Builder::new()
        .format(|buf, record| {
            writeln!(
                buf,
                "{{\"date\": \"{}\", \"level\": \"{}\", \"log\": {}}}",
                Local::now().format("%Y-%m-%dT%H:%M:%S:%f"),
                record.level(),
                record.args()
            )
        })
        .target(Target::Stdout)
        .filter_level(LevelFilter::Info)
        .parse_default_env()
        .init();

    // Set port
    let port: u16 = opts.value_of("port").unwrap().parse().unwrap_or_else(|_| {
        eprintln!("specified port isn't in a valid range, setting to 8080");
        8080
    });

    // Set limit
    let limit: usize = opts.value_of("limit").unwrap().parse().unwrap_or_else(|_| {
        eprintln!("specified limit isn't in a valid range, setting to 10MB");
        10485760
    });

    // Create mongo client
    let client_options = ClientOptions::parse(opts.value_of("mongo").unwrap()).await?;
    let mongo_client = Client::with_options(client_options)?;
    if let Err(e) = mongo_client.list_database_names(None, None).await {
        panic!("{}", e);
    };

    let gcs_client = cloud_storage::Client::default();
    gcs_client
        .bucket()
        .read(opts.value_of("bucket").unwrap())
        .await?;

    // Create state for axum
    let mut state = State::new(opts.clone(), mongo_client, gcs_client).await?;
    state.create_indexes().await?;
    state.init().await?;

    // Create prometheus handle
    let recorder_handle = setup_metrics_recorder();

    // These should be authenticated
    let base = Router::new().route("/", get(root));

    // These should NOT be authenticated
    let standard = Router::new()
        .route("/health", get(health))
        .route("/upload", post(cache_set))
        .route("/download/:id", get(cache_get))
        .route("/metrics", get(move || ready(recorder_handle.render())));

    let app = Router::new()
        .merge(base)
        .merge(standard)
        .layer(TraceLayer::new_for_http())
        .route_layer(middleware::from_fn(track_metrics))
        .layer(DefaultBodyLimit::disable())
        .layer(RequestBodyLimitLayer::new(limit))
        .layer(Extension(state));

    // add a fallback service for handling routes to unknown paths
    let app = app.fallback(handler_404.into_service());

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    log::info!("\"Listening on {}\"", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}
