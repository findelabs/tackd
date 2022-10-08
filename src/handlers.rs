use axum::{
    Extension,
    extract::{OriginalUri, Query, Path},
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
    key: String
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

pub async fn cache_get(Extension(mut state): Extension<State>, queries: Query<QueriesGet>, Path(id): Path<String>) -> Result<Response, RestError> {
    match state.lock.get(&id, &queries.key).await {
        Ok(s) => {
            log::info!("{{\"method\": \"GET\", \"path\": \"/cache/{}\", \"status\": 200}}", &id);
            Ok((StatusCode::OK, s).into_response())
        },
        Err(e) => {
            log::info!("{{\"method\": \"GET\", \"path\": \"/cache/{}\", \"status\": 401}}", &id);
            Err(e)
        }
    }
}

pub async fn cache_set(Extension(mut state): Extension<State>, queries: Query<QueriesSet>, body: String) -> Result<Response, RestError> {
    let results = state.lock.set(body, queries.reads, queries.expires_in).await?;
    log::info!("{{\"method\": \"POST\", \"path\": \"/cache\", \"id\": \"{}\", \"status\": 201}}", &results.id);
    let url = format!("{}/tack/{}?key={}", state.url, results.id, results.key);
    let json = json!({"message": "Saved", "url": url, "data": { "id": results.id, "key": results.key, "expires": results.expires, "max reads": results.reads}});
    Ok((StatusCode::CREATED, json.to_string()).into_response())
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
