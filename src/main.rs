use axum::extract::DefaultBodyLimit;
use axum::{
    extract::Extension,
    middleware,
    routing::{delete, get, post, put},
    Router,
};
use azure_storage::StorageCredentials;
use azure_storage_blobs::prelude::*;
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
use utoipa_swagger_ui::SwaggerUi;
use utoipa::{
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
    Modify, OpenApi,
};

#[cfg(not(target_env = "msvc"))]
use jemallocator::Jemalloc;

mod data;
mod database;
mod error;
mod handlers;
mod helpers;
mod metrics;
mod state;
mod storage;

use crate::metrics::{setup_metrics_recorder, track_metrics};
use crate::handlers::{CreateUser};
use crate::storage::azure_blob::AzureBlobClient;
use crate::storage::gcs::GcsClient;
use crate::storage::trait_storage::StorageClient;
use handlers::{
    add_doc_tags, add_link, download, upload, create_api_key, create_user, delete_api_key,
    delete_doc, delete_doc_tags, delete_link, get_doc, get_doc_tags, get_links, get_user_id,
    handler_404, health, list_api_keys, list_uploads, root,
};
use state::State;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[derive(OpenApi)]
#[openapi(
    paths(
        handlers::health,
        handlers::root,
        handlers::create_user,
        handlers::get_user_id,
        handlers::create_api_key,
        handlers::get_doc,
        handlers::delete_doc,
        handlers::delete_link,
        handlers::get_links,
        handlers::add_link,
        handlers::delete_api_key,
        handlers::list_api_keys,
        handlers::list_uploads,
        handlers::add_doc_tags,
        handlers::delete_doc_tags,
        handlers::get_doc_tags,
        handlers::download,
        handlers::upload,
    ),
    modifiers(&SecurityAddon),
    components(schemas(CreateUser))
)]
struct ApiDoc;
struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "basic",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Basic)
                        .build()
                )
            );
        }
    }
}


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
            Arg::new("retention")
                .short('r')
                .long("retention")
                .help("Set the default retention ms")
                .env("TACKD_RETENTION_MS")
                .default_value("3600")
                .takes_value(true),
        )
        .arg(
            Arg::new("reads")
                .short('R')
                .long("reads")
                .help("Set the default read count")
                .env("TACKD_READS")
                .default_value("-1")
                .takes_value(true),
        )
        .arg(
            Arg::new("keys")
                .short('k')
                .long("keys")
                .help("Set encryption keys")
                .env("TACKD_KEYS")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::new("azure_storage_account")
                .short('A')
                .long("azure_storage_account")
                .help("Set Azure Storage Account")
                .env("AZURE_STORAGE_ACCOUNT")
                .required(false)
                .takes_value(true),
        )
        .arg(
            Arg::new("azure_storage_access_key")
                .short('s')
                .long("azure_storage_access_key")
                .help("Set Azure Storage Access Key")
                .env("AZURE_STORAGE_ACCESS_KEY")
                .required(false)
                .takes_value(true),
        )
        .arg(
            Arg::new("ignore_link_key")
                .short('i')
                .long("ignore_link_key")
                .help("Ignore link keys, useful for private deployments")
                .env("TACKD_IGNORE_LINK_KEY")
                .required(false)
                .takes_value(false),
        )
        .arg(
            Arg::new("encrypt_data")
                .short('e')
                .long("encrypt_data")
                .help("Encrypt data before committing to object storage")
                .env("TACKD_ENCRYPT_DATA")
                .required(false)
                .takes_value(false),
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

    // Ensure that we can talk to storage
    let storage_client = if std::env::var("SERVICE_ACCOUNT_JSON").is_ok()
        || std::env::var("GOOGLE_APPLICATION_CREDENTIALS").is_ok()
    {
        let gcs_client = cloud_storage::Client::default();
        StorageClient::GcsClient(GcsClient::new(opts.value_of("bucket").unwrap(), gcs_client))
    } else if opts.value_of("azure_storage_account").is_some()
        && opts.value_of("azure_storage_access_key").is_some()
    {
        let account = opts
            .value_of("azure_storage_account")
            .expect("Set env variable AZURE_STORAGE_ACCOUNT first");
        let access_key = opts
            .value_of("azure_storage_access_key")
            .expect("Set env variable AZURE_STORAGE_ACCESS_KEY first!");
        let storage_credentials =
            StorageCredentials::Key(account.to_string(), access_key.to_string());
        let service_client = BlobServiceClient::new(account, storage_credentials);
        StorageClient::AzureBlobClient(AzureBlobClient::new(
            opts.value_of("bucket").unwrap(),
            service_client,
        ))
    } else {
        panic!("No storage credentials found!");
    };

    // This takes too long to startup
    //    gcs_client
    //        .bucket()
    //        .read(opts.value_of("bucket").unwrap())
    //        .await?;

    // Create state for axum
    let mut state = State::new(opts.clone(), mongo_client, storage_client).await?;
    state.init().await?;

    // Create prometheus handle
    let recorder_handle = setup_metrics_recorder();

    // These should be authenticated through api keys
    let authenticated = Router::new()
        .route("/", get(root))
        .route(
            "/api/v1/user/apiKeys",
            get(list_api_keys).post(create_api_key),
        )
        .route("/api/v1/user/apiKeys/:key", delete(delete_api_key))
        .route("/api/v1/uploads", get(list_uploads))
        .route("/api/v1/uploads/:id", get(get_doc).delete(delete_doc))
        .route(
            "/api/v1/uploads/:id/tags",
            put(add_doc_tags).delete(delete_doc_tags).get(get_doc_tags),
        )
        .route("/api/v1/uploads/:id/links", put(add_link).get(get_links))
        .route("/api/v1/uploads/:id/links/:link", delete(delete_link))
        .route("/health", get(health))
        .route("/upload", post(upload));

    // These should NOT be authenticated through api keys
    let not_authenticated = Router::new()
        .route("/download/:id", get(download))
        .route("/api/v1/user", post(create_user))
        .route("/api/v1/user/recover/id", post(get_user_id))
        .route("/metrics", get(move || ready(recorder_handle.render())));

    let app = Router::new()
        .merge(authenticated)
        .route_layer(middleware::from_fn(database::auth::auth))
        .merge(not_authenticated)
        .merge(SwaggerUi::new("/swagger-ui")
        .url("/api-doc/openapi.json", ApiDoc::openapi()))
        .layer(TraceLayer::new_for_http())
        .route_layer(middleware::from_fn(track_metrics))
        .layer(DefaultBodyLimit::disable())
        .layer(RequestBodyLimitLayer::new(limit))
        .fallback(handler_404)
        .layer(Extension(state));

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    log::info!("\"Listening on {}\"", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}

