use axum::{
    body::Bytes,
    extract::{OriginalUri, Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use clap::{crate_description, crate_name, crate_version};
use hyper::HeaderMap;
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
    pwd: Option<String>,
}

#[derive(Deserialize)]
pub struct QueriesSet {
    filename: Option<String>,
    expires: Option<i64>,
    reads: Option<i64>,
    pwd: Option<String>,
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

    match state
        .get(&id_override, &queries.key, queries.pwd.as_ref())
        .await
    {
        Ok((s, c)) => {
            log::info!(
                "{{\"method\": \"GET\", \"path\": \"/download/{}\", \"status\": 200}}",
                &id_override
            );
            let mut headers = HeaderMap::new();
            headers.insert("content-type", c.parse().unwrap());
            Ok((StatusCode::OK, headers, s).into_response())
        }
        Err(e) => {
            log::info!(
                "{{\"method\": \"GET\", \"path\": \"/download/{}\", \"status\": 401}}",
                &id_override
            );
            Err(e)
        }
    }
}

pub async fn cache_set(
    Extension(mut state): Extension<State>,
    queries: Query<QueriesSet>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, RestError> {
    let results = state
        .set(
            body,
            queries.reads,
            queries.expires,
            queries.pwd.as_ref(),
            headers,
        )
        .await?;
    log::info!(
        "{{\"method\": \"POST\", \"path\": \"/upload\", \"id\": \"{}\", \"status\": 201}}",
        &results.id
    );

    // If client specified a desired filename, include that in url
    let url = match &queries.filename {
        Some(filename) => format!(
            "{}/download/{}?key={}&id={}",
            state.url, filename, results.key, results.id
        ),
        None => format!("{}/download/{}?key={}", state.url, results.id, results.key),
    };

    let json = json!({"message": "Saved", "url": url, "data": { "id": results.id, "key": results.key, "expires_in": results.expire_seconds, "max_reads": results.expire_reads, "password_protected": results.pwd}});
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
