use axum::{
    body::Bytes,
    extract::{OriginalUri, Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use clap::{crate_description, crate_name, crate_version};
use hyper::header::CONTENT_TYPE;
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
    password: Option<String>,
}

#[derive(Deserialize)]
pub struct QueriesSet {
    filename: Option<String>,
    expires: Option<i64>,
    reads: Option<i64>,
    password: Option<String>,
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
        .get(&id_override, &queries.key, queries.password.as_ref())
        .await
    {
        Ok((s, c)) => {
            log::info!(
                "{{\"method\": \"GET\", \"path\": \"/note/{}\", \"status\": 200}}",
                &id_override
            );
            let mut headers = HeaderMap::new();
            headers.insert("content-type", c.parse().unwrap());
            Ok((StatusCode::OK, headers, s).into_response())
        }
        Err(e) => {
            log::info!(
                "{{\"method\": \"GET\", \"path\": \"/note/{}\", \"status\": 401}}",
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
    let content_type = match headers.get(CONTENT_TYPE) {
        Some(h) => match h.to_str() {
            Ok(s) => s.to_string(),
            Err(_) => "application/octet-stream".to_string(),
        },
        None => "application/octet-stream".to_string(),
    };

    let results = state
        .set(
            body,
            queries.reads,
            queries.expires,
            queries.password.as_ref(),
            content_type,
        )
        .await?;
    log::info!(
        "{{\"method\": \"POST\", \"path\": \"/upload\", \"id\": \"{}\", \"status\": 201}}",
        &results.id
    );

    // If client specified a desired filename, include that in url
    let url = match &queries.filename {
        Some(filename) => format!(
            "{}/note/{}?key={}&id={}",
            state.url, filename, results.key, results.id
        ),
        None => format!("{}/note/{}?key={}", state.url, results.id, results.key),
    };

    let json = json!({"message": "Saved", "url": url, "data": { "id": results.id, "key": results.key, "expires in": results.expire_seconds, "max reads": results.expire_reads}});
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
