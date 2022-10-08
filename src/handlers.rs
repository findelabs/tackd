use axum::{
    body::Bytes,
    extract::{OriginalUri, Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use clap::{crate_description, crate_name, crate_version};
use serde::Deserialize;
use serde_json::json;
use serde_json::Value;

use crate::error::Error as RestError;
use crate::State;

// This is required in order to get the method from the request
#[derive(Debug)]
pub struct RequestMethod(pub hyper::Method);

#[derive(Deserialize)]
pub struct QueriesGet {
    key: String,
    id: Option<String>,
}

#[derive(Deserialize)]
pub struct QueriesSet {
    filename: Option<String>,
    expires_in: Option<u64>,
    reads: Option<u64>,
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

pub async fn cache_get(
    Extension(mut state): Extension<State>,
    queries: Query<QueriesGet>,
    Path(id): Path<String>,
) -> Result<Response, RestError> {
    let id_override = match &queries.id {
        Some(i) => i.clone(),
        None => id,
    };

    match state.lock.get(&id_override, &queries.key).await {
        Ok(s) => {
            log::info!(
                "{{\"method\": \"GET\", \"path\": \"/cache/{}\", \"status\": 200}}",
                &id_override
            );
            Ok((StatusCode::OK, s).into_response())
        }
        Err(e) => {
            log::info!(
                "{{\"method\": \"GET\", \"path\": \"/cache/{}\", \"status\": 401}}",
                &id_override
            );
            Err(e)
        }
    }
}

pub async fn cache_set(
    Extension(mut state): Extension<State>,
    queries: Query<QueriesSet>,
    body: Bytes,
) -> Result<Response, RestError> {
    let results = state
        .lock
        .set(body, queries.reads, queries.expires_in)
        .await?;
    log::info!(
        "{{\"method\": \"POST\", \"path\": \"/cache\", \"id\": \"{}\", \"status\": 201}}",
        &results.id
    );

    // If client specified a desired filename, include that in url
    let url = match &queries.filename {
        Some(filename) => format!(
            "{}/tack/{}?key={}&id={}",
            state.url, filename, results.key, results.id
        ),
        None => format!("{}/tack/{}?key={}", state.url, results.id, results.key),
    };

    let json = json!({"message": "Saved", "url": url, "data": { "id": results.id, "key": results.key, "expires": results.expires, "max reads": results.reads}});
    Ok((StatusCode::CREATED, json.to_string()).into_response())
}

pub async fn handler_404(OriginalUri(original_uri): OriginalUri) -> impl IntoResponse {
    let parts = original_uri.into_parts();
    let path_and_query = parts.path_and_query.expect("Missing post path and query");
    log::info!(
        "{{\"fn\": \"handler_404\", \"method\":\"get\", \"path\":\"{}\"}}",
        path_and_query
    );
    RestError::NotFound
}
