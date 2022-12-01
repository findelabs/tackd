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
use crate::auth::CurrentUser;
use crate::State;
use crate::secret::SecretScrubbed;

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
pub struct CreateUser {
    email: String,
    pwd: String
}

#[derive(Deserialize)]
pub struct QueriesSet {
    pub filename: Option<String>,
    pub expires: Option<i64>,
    pub reads: Option<i64>,
    pub pwd: Option<String>,
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

pub async fn create_user(
    Extension(state): Extension<State>,
    Json(payload): Json<CreateUser>
) -> Result<Json<Value>, RestError> {

    match state.create_user(&payload.email, &payload.pwd).await {
        Ok(u) => {
            log::info!(
                "{{\"method\": \"POST\", \"path\": \"/api/v1/user\", \"status\": 200}}",
            );
            Ok(Json(json!({ "created": true, "user id": u})))
        },
        Err(e) => Err(e)
    }
}

pub async fn get_user_id(
    Extension(state): Extension<State>,
    Json(payload): Json<CreateUser>
) -> Result<Json<Value>, RestError> {

    match state.get_user_id(&payload.email, &payload.pwd).await {
        Ok(u) => {
            log::info!(
                "{{\"method\": \"POST\", \"path\": \"/api/v1/user/recover/id\", \"status\": 200}}",
            );
            Ok(Json(json!({ "email": &payload.email, "user id": u})))
        },
        Err(e) => Err(e)
    }
}

pub async fn create_api_key(
    Extension(state): Extension<State>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<Value>, RestError> {
    if let Some(id) = &current_user.id {
        match state.create_api_key(id).await {
            Ok(api_key) => {
                log::info!(
                    "{{\"method\": \"POST\", \"path\": \"/api/v1/user/apiKey\", \"status\": 200}}",
                );
                Ok(Json(json!({ "created": true, "data": api_key })))
            },
            Err(e) => Err(e)
        }
    } else {
        Err(RestError::Unauthorized)
    }
}

pub async fn add_link(
    Extension(state): Extension<State>,
    Extension(current_user): Extension<CurrentUser>,
    Path(doc_id): Path<String>,
) -> Result<Json<Value>, RestError> {
    if let Some(user_id) = &current_user.id {
        match state.add_link(user_id, &doc_id).await {
            Ok(new_link) => {
                log::info!(
                    "{{\"method\": \"POST\", \"path\": \"/api/v1/uploads/{}/links\", \"status\": 200}}",
                    doc_id
                );
                let url = format!(
                    "{}/download/{}?key={}",
                    state.configs.url, new_link.link.id, new_link.key.as_ref().unwrap()
                );

                Ok(Json(json!({ "created": true, "url": url, "data": new_link.to_json() })))
            },
            Err(e) => Err(e)
        }
    } else {
        Err(RestError::Unauthorized)
    }
}

pub async fn delete_api_key(
    Extension(state): Extension<State>,
    Extension(current_user): Extension<CurrentUser>,
    Path(key): Path<String>,
) -> Result<Json<Value>, RestError> {
    if let Some(id) = &current_user.id {
        match state.delete_api_key(id, &key).await {
            Ok(success) => {
                log::info!(
                    "{{\"method\": \"DELETE\", \"path\": \"/api/v1/user/apiKey/{}\", \"status\": 200}}",
                    &key
                );
                Ok(Json(json!({ "deleted": success})))
            },
            Err(e) => Err(e)
        }
    } else {
        Err(RestError::Unauthorized)
    }
}

pub async fn list_api_keys(
    Extension(state): Extension<State>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<Value>, RestError> {
    if let Some(id) = &current_user.id {
        match state.list_api_keys(id).await {
            Ok(api_keys) => {
                log::info!(
                    "{{\"method\": \"GET\", \"path\": \"/api/v1/user/apiKey\", \"status\": 200}}",
                );
                Ok(Json(json!(api_keys)))
            },
            Err(e) => Err(e)
        }
    } else {
        Err(RestError::Unauthorized)
    }
}

pub async fn list_uploads(
    Extension(state): Extension<State>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<Vec<SecretScrubbed>>, RestError> {
    if let Some(id) = &current_user.id {
        match state.uploads_owned(&id).await {
            Ok(uploads) => {
                log::info!(
                    "{{\"method\": \"GET\", \"path\": \"/api/v1/user/uploads\", \"status\": 200}}",
                );
                Ok(Json(uploads))
            },
            Err(e) => Err(e)
        }
    } else {
        Err(RestError::Unauthorized)
    }
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
    Extension(current_user): Extension<CurrentUser>,
    queries: Query<QueriesSet>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, RestError> {
    let results = state
        .set(
            body,
            &queries,
            headers,
            current_user
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
            state.configs.url, filename, results.key, results.id
        ),
        None => format!("{}/download/{}?key={}", state.configs.url, results.id, results.key),
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
