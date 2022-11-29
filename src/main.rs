use axum::extract::DefaultBodyLimit;
use axum::{
    extract::Extension,
    handler::Handler,
    middleware,
    routing::{get, post, delete},
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
mod users;
mod auth;

use crate::metrics::{setup_metrics_recorder, track_metrics};
use handlers::{cache_get, cache_set, handler_404, health, root, create_user, create_api_key, delete_api_key, list_api_keys, list_uploads, get_user_id};
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
            Arg::new("users")
                .short('U')
                .long("users")
                .help("MongoDB Users Collection")
                .env("TACKD_MONGODB_USERS_COLLECTION")
                .default_value("users")
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
        .arg(
            Arg::new("keys")
                .short('k')
                .long("keys")
                .help("Set encryption keys")
                .env("TACKD_KEYS")
                .required(true)
                .takes_value(true)
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

    // Ensure that we can talk to GCS
    let gcs_client = cloud_storage::Client::default();
    gcs_client
        .bucket()
        .read(opts.value_of("bucket").unwrap())
        .await?;

    // Create state for axum
    let mut state = State::new(opts.clone(), mongo_client, gcs_client).await?;
    state.init().await?;

    // Create prometheus handle
    let recorder_handle = setup_metrics_recorder();

    // These should be authenticated through api keys
    let authenticated = Router::new().
        route("/", get(root))
        .route("/api/v1/user/apiKeys", get(list_api_keys).post(create_api_key))
        .route("/api/v1/user/apiKeys/:key", delete(delete_api_key))
        .route("/api/v1/uploads", get(list_uploads))
        .route("/health", get(health))
        .route("/upload", post(cache_set));

    // These should NOT be authenticated through api keys
    let not_authenticated = Router::new()
        .route("/download/:id", get(cache_get))
        .route("/api/v1/user", post(create_user))
        .route("/api/v1/user/recover/id", post(get_user_id))
        .route("/metrics", get(move || ready(recorder_handle.render())));

    let app = Router::new()
        .merge(authenticated)
        .route_layer(middleware::from_fn(auth::auth))
        .merge(not_authenticated)
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
