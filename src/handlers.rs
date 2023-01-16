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
use serde_json::{json, Value};

//use crate::database::secret::SecretScrubbed;
use crate::database::metadata::MetaDataPublic;
use crate::database::users::CurrentUser;
use crate::error::Error as RestError;
use crate::helpers::tags_deserialize;
use crate::State;

// This is required in order to get the method from the request
#[derive(Debug)]
pub struct RequestMethod(pub hyper::Method);

#[derive(Deserialize)]
pub struct QueriesGet {
    key: Option<String>,
    id: Option<String>,
    pwd: Option<String>,
}

#[derive(Deserialize)]
pub struct RoleTagsCreate {
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(deserialize_with = "tags_deserialize")]
    tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
}

#[derive(Deserialize)]
pub struct Tags {
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(deserialize_with = "tags_deserialize")]
    tags: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct CreateUser {
    email: String,
    pwd: String,
}

#[derive(Deserialize)]
pub struct QueriesSet {
    pub filename: Option<String>,
    pub expires: Option<String>,
    pub reads: Option<i64>,
    pub pwd: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(deserialize_with = "tags_deserialize")]
    pub tags: Option<Vec<String>>,
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
    Json(payload): Json<CreateUser>,
) -> Result<Json<Value>, RestError> {
    match state.create_user(&payload.email, &payload.pwd).await {
        Ok(u) => {
            log::info!("{{\"method\": \"POST\", \"path\": \"/api/v1/user\", \"status\": 200}}",);
            Ok(Json(json!({ "created": true, "user id": u})))
        }
        Err(e) => Err(e),
    }
}

pub async fn get_user_id(
    Extension(state): Extension<State>,
    Json(payload): Json<CreateUser>,
) -> Result<Json<Value>, RestError> {
    match state.get_user_id(&payload.email, &payload.pwd).await {
        Ok(u) => {
            log::info!(
                "{{\"method\": \"POST\", \"path\": \"/api/v1/user/recover/id\", \"status\": 200}}",
            );
            Ok(Json(json!({ "email": &payload.email, "user id": u})))
        }
        Err(e) => Err(e),
    }
}

pub async fn create_api_key(
    Extension(state): Extension<State>,
    Extension(current_user): Extension<CurrentUser>,
    queries: Query<RoleTagsCreate>,
) -> Result<Json<Value>, RestError> {
    if current_user.id.is_some() && current_user.create() {
        match state
            .create_api_key(
                &current_user.id.unwrap(),
                queries.tags.clone(),
                queries.role.clone(),
            )
            .await
        {
            Ok(api_key) => {
                log::info!(
                    "{{\"method\": \"POST\", \"path\": \"/api/v1/user/apiKey\", \"status\": 200}}",
                );
                Ok(Json(json!({ "created": true, "data": api_key })))
            }
            Err(e) => Err(e),
        }
    } else {
        Err(RestError::Unauthorized)
    }
}

pub async fn get_doc(
    Extension(state): Extension<State>,
    Extension(current_user): Extension<CurrentUser>,
    Path(doc_id): Path<String>,
) -> Result<Json<MetaDataPublic>, RestError> {
    if current_user.id.is_some() && current_user.list() {
        match state
            .get_doc(current_user.id.as_ref().unwrap(), &doc_id)
            .await
        {
            Ok(upload) => {
                log::info!(
                    "{{\"method\": \"GET\", \"path\": \"/api/v1/user/uploads/{}\", \"status\": 200}}", &current_user.id.unwrap()
                );
                Ok(Json(upload))
            }
            Err(e) => Err(e),
        }
    } else {
        Err(RestError::Unauthorized)
    }
}

pub async fn delete_doc(
    Extension(state): Extension<State>,
    Extension(current_user): Extension<CurrentUser>,
    Path(doc_id): Path<String>,
) -> Result<Json<Value>, RestError> {
    if current_user.id.is_some() && current_user.delete() {
        match state
            .delete_doc(current_user.id.as_ref().unwrap(), &doc_id)
            .await
        {
            Ok(_) => {
                log::info!(
                    "{{\"method\": \"DELETE\", \"path\": \"/api/v1/user/uploads/{}\", \"status\": 200}}", &current_user.id.unwrap()
                );
                Ok(Json(json!({ "delete": true })))
            }
            Err(e) => Err(e),
        }
    } else {
        Err(RestError::Unauthorized)
    }
}

pub async fn delete_link(
    Extension(state): Extension<State>,
    Extension(current_user): Extension<CurrentUser>,
    Path((doc_id, link_id)): Path<(String, String)>,
) -> Result<Json<Value>, RestError> {
    if current_user.id.is_some() && current_user.delete() {
        match state
            .delete_link(current_user.id.as_ref().unwrap(), &doc_id, &link_id)
            .await
        {
            Ok(_) => {
                log::info!(
                    "{{\"method\": \"DELETE\", \"path\": \"/api/v1/user/uploads/{}/links/{}\", \"status\": 200}}", doc_id, link_id
                );
                Ok(Json(json!({ "delete": true })))
            }
            Err(e) => Err(e),
        }
    } else {
        Err(RestError::Unauthorized)
    }
}

pub async fn get_links(
    Extension(state): Extension<State>,
    Extension(current_user): Extension<CurrentUser>,
    Path(doc_id): Path<String>,
) -> Result<Json<Value>, RestError> {
    if current_user.id.is_some() && current_user.list() {
        match state
            .get_links(current_user.id.as_ref().unwrap(), &doc_id)
            .await
        {
            Ok(secret) => {
                log::info!(
                    "{{\"method\": \"GET\", \"path\": \"/api/v1/uploads/{}/links\", \"status\": 200}}",
                    doc_id
                );
                Ok(Json(json! {secret}))
            }
            Err(e) => Err(e),
        }
    } else {
        Err(RestError::Unauthorized)
    }
}

pub async fn add_link(
    Extension(state): Extension<State>,
    Extension(current_user): Extension<CurrentUser>,
    Path(doc_id): Path<String>,
    queries: Query<RoleTagsCreate>,
) -> Result<Json<Value>, RestError> {
    if current_user.id.is_some() && current_user.create() {
        match state
            .add_link(
                current_user.id.as_ref().unwrap(),
                &doc_id,
                queries.tags.clone(),
            )
            .await
        {
            Ok((new_link, filename, ignore_link_key)) => {
                log::info!(
                    "{{\"method\": \"POST\", \"path\": \"/api/v1/uploads/{}/links\", \"status\": 200}}",
                    doc_id
                );
                // If client specified a desired filename, include that in url
                let url = match filename {
                    Some(filename) => {
                        if ignore_link_key || !&current_user.id.is_some() {
                            format!(
                                "{}/download/{}?id={}",
                                state.configs.url, filename, new_link.link.id
                            )
                        } else {
                            format!(
                                "{}/download/{}?id={}&key={}",
                                state.configs.url,
                                filename,
                                new_link.link.id,
                                new_link.key.as_ref().unwrap()
                            )
                        }
                    }
                    None => {
                        if ignore_link_key || !&current_user.id.is_some() {
                            format!("{}/download/{}", state.configs.url, new_link.link.id)
                        } else {
                            format!(
                                "{}/download/{}?key={}",
                                state.configs.url,
                                new_link.link.id,
                                new_link.key.as_ref().unwrap()
                            )
                        }
                    }
                };

                Ok(Json(
                    json!({ "created": true, "url": url, "data": new_link.to_json() }),
                ))
            }
            Err(e) => Err(e),
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
    if current_user.id.is_some() && current_user.delete() {
        match state
            .delete_api_key(current_user.id.as_ref().unwrap(), &key)
            .await
        {
            Ok(success) => {
                log::info!(
                    "{{\"method\": \"DELETE\", \"path\": \"/api/v1/user/apiKey/{}\", \"status\": 200}}",
                    &key
                );
                Ok(Json(json!({ "deleted": success })))
            }
            Err(e) => Err(e),
        }
    } else {
        Err(RestError::Unauthorized)
    }
}

pub async fn list_api_keys(
    Extension(state): Extension<State>,
    Extension(current_user): Extension<CurrentUser>,
) -> Result<Json<Value>, RestError> {
    if current_user.id.is_some() && current_user.list() {
        match state.list_api_keys(current_user.id.as_ref().unwrap()).await {
            Ok(api_keys) => {
                log::info!(
                    "{{\"method\": \"GET\", \"path\": \"/api/v1/user/apiKey\", \"status\": 200}}",
                );
                Ok(Json(json!(api_keys)))
            }
            Err(e) => Err(e),
        }
    } else {
        Err(RestError::Unauthorized)
    }
}

pub async fn list_uploads(
    Extension(state): Extension<State>,
    Extension(current_user): Extension<CurrentUser>,
    queries: Query<Tags>,
) -> Result<Json<Vec<MetaDataPublic>>, RestError> {
    if current_user.id.is_some() && current_user.list() {
        match state
            .uploads_owned(current_user.id.as_ref().unwrap(), queries.tags.clone())
            .await
        {
            Ok(uploads) => {
                log::info!(
                    "{{\"method\": \"GET\", \"path\": \"/api/v1/user/uploads\", \"status\": 200}}",
                );
                Ok(Json(uploads))
            }
            Err(e) => Err(e),
        }
    } else {
        Err(RestError::Unauthorized)
    }
}

pub async fn add_doc_tags(
    Extension(state): Extension<State>,
    Extension(current_user): Extension<CurrentUser>,
    Path(doc_id): Path<String>,
    queries: Query<Tags>,
) -> Result<Json<Vec<String>>, RestError> {
    if current_user.id.is_some() && current_user.create() {
        match state
            .add_doc_tags(
                current_user.id.as_ref().unwrap(),
                &doc_id,
                queries.tags.clone(),
            )
            .await
        {
            Ok(tags) => {
                log::info!(
                    "{{\"method\": \"POST\", \"path\": \"/api/v1/user/uploads/{}/tags\", \"status\": 200}}",
                    doc_id
                );
                Ok(Json(tags))
            }
            Err(e) => Err(e),
        }
    } else {
        Err(RestError::Unauthorized)
    }
}

pub async fn delete_doc_tags(
    Extension(state): Extension<State>,
    Extension(current_user): Extension<CurrentUser>,
    Path(doc_id): Path<String>,
    queries: Query<Tags>,
) -> Result<Json<Vec<String>>, RestError> {
    if current_user.id.is_some() && current_user.delete() {
        match state
            .delete_doc_tags(
                current_user.id.as_ref().unwrap(),
                &doc_id,
                queries.tags.clone(),
            )
            .await
        {
            Ok(tags) => {
                log::info!(
                    "{{\"method\": \"DELETE\", \"path\": \"/api/v1/user/uploads/{}/tags\", \"status\": 200}}",
                    doc_id
                );
                Ok(Json(tags))
            }
            Err(e) => Err(e),
        }
    } else {
        Err(RestError::Unauthorized)
    }
}

pub async fn get_doc_tags(
    Extension(state): Extension<State>,
    Extension(current_user): Extension<CurrentUser>,
    Path(doc_id): Path<String>,
) -> Result<Json<Vec<String>>, RestError> {
    if current_user.id.is_some() && current_user.list() {
        match state
            .get_doc_tags(current_user.id.as_ref().unwrap(), &doc_id)
            .await
        {
            Ok(tags) => {
                log::info!(
                    "{{\"method\": \"GET\", \"path\": \"/api/v1/user/uploads/{}/tags\", \"status\": 200}}",
                    doc_id
                );
                Ok(Json(tags))
            }
            Err(e) => Err(e),
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
        .get(&id_override, queries.key.as_ref(), queries.pwd.as_ref())
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
        .set(body, &queries, headers, current_user.clone())
        .await?;
    log::info!(
        "{{\"method\": \"POST\", \"path\": \"/upload\", \"id\": \"{}\", \"status\": 201}}",
        &results.data.id
    );

    let json = json!({"message": "Saved", "message": results });
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
