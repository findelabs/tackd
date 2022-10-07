use axum::{
    Extension,
    extract::{OriginalUri, Query},
    http::StatusCode,
    response::{Response, IntoResponse},
    Json,
};
use clap::{crate_description, crate_name, crate_version};
use serde_json::json;
use serde_json::Value;
use serde::Deserialize;


use crate::error::Error as RestError;
use crate::State;

// This is required in order to get the method from the request
#[derive(Debug)]
pub struct RequestMethod(pub hyper::Method);

#[derive(Deserialize)]
pub struct QueriesGet {
    key: String,
    secret: String,
}

#[derive(Deserialize)]
pub struct QueriesSet {
    expires_in: Option<u64>,
    reads: Option<u64>
}

pub async fn health() -> Json<Value> {
    log::info!("{{\"fn\": \"health\", \"method\":\"get\"}}");
    Json(json!({ "msg": "Healthy"}))
}

pub async fn root() -> Json<Value> {
    log::info!("{{\"fn\": \"root\", \"method\":\"get\"}}");
    Json(
        json!({ "version": crate_version!(), "name": crate_name!(), "description": crate_description!()}),
    )
}

pub async fn cache_get(queries: Query<QueriesGet>) -> Json<Value> {
    log::info!("{{\"fn\": \"cache_get\", \"method\":\"get\"}}");
    Json(json!({"key": "value"}))
}

pub async fn cache_set(Extension(mut state): Extension<State>, queries: Query<QueriesSet>, body: String) -> Result<Response, RestError> {
    log::info!("{{\"fn\": \"cache_set\", \"method\":\"post\"}}");
    let results = state.lock.set(body, queries.reads, queries.expires_in).await?;
    let json = json!({"id": results.id, "key": results.key});
    Ok((StatusCode::OK, json.to_string()).into_response())
}

pub async fn help() -> Json<Value> {
    log::info!("{{\"fn\": \"help\", \"method\":\"get\"}}");
    let payload = json!({"paths": {
            "/health": "Get the health of the api",
            "/reload": "Reload the api's config",
            "/help": "Show this help message"
        }
    });
    Json(payload)
}

pub async fn handler_404(OriginalUri(original_uri): OriginalUri) -> impl IntoResponse {
    let parts = original_uri.into_parts();
    let path_and_query = parts.path_and_query.expect("Missing post path and query");
    log::info!(
        "{{\"fn\": \"handler_404\", \"method\":\"get\", \"path\":\"{}\"}}",
        path_and_query
    );
    (
        StatusCode::NOT_FOUND,
        "{\"error_code\": 404, \"message\": \"HTTP 404 Not Found\"}",
    )
}
