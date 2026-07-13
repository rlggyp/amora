use crate::AppState;

use std::{collections::{HashMap, HashSet}, sync::Arc};
use axum::{extract::{Request, State}, http::{HeaderMap, HeaderValue, StatusCode, header}, middleware::Next, response::{IntoResponse, Response}};
use base64::{engine::general_purpose, Engine};
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct Credential {
    pub encoded: String,
    pub decoded: String,
}

#[derive(Clone)]
pub struct BasicAuth {
    pub credentials: HashMap<String, String>,
    pub hashed_credential_cache: Arc<RwLock<HashSet<String>>>,
    pub cache_key: [u8; 32],
}

impl BasicAuth {
    pub fn new(credentials: HashMap<String, String>) -> Self {
        let mut key = [0u8; 32];
        getrandom::fill(&mut key).expect("Failed to generate random key");
        
        Self {
            credentials,
            hashed_credential_cache: Arc::new(RwLock::new(HashSet::new())),
            cache_key: key,
        }
    }

    fn hash_credential(key: &[u8; 32], credential: &str) -> String {
        let hash = blake3::keyed_hash(key, credential.as_bytes());
        hash.to_hex().to_string()
    }

    fn is_valid_basic_auth_header(auth_header: &str) -> Option<Credential> {
        log::debug!("Checking if Authorization header is valid Basic Auth: {}", auth_header);

        if !(auth_header.starts_with("Basic") && auth_header.len() > 6) {
            log::debug!("Auth header does not start with `Basic` or is too short");
            return None;
        }

        let encoded= auth_header[6..].to_string();

        let decoded = match general_purpose::STANDARD.decode(&encoded) {
            Ok(bytes) =>  match String::from_utf8(bytes) {
                Ok(s) => s,
                Err(_) => {
                    log::debug!("Failed to decode base64 to UTF-8");
                    return None;
                },
            },
            Err(_) => {
                log::debug!("Failed to decode base64 from auth header");
                return None;
            },
        };

        let credential = Credential {
            encoded,
            decoded,
        };

        Some(credential)
    }

    fn parse_user_pass(&self, credential: &String) -> Option<(String, String)> {
        let parts: Vec<&str> = credential.splitn(2,':').collect();

        if parts.len() != 2 {
            log::debug!("Credential does not contain a valid `user:pass` format");
            return None;
        }

        let username = parts[0].to_string();
        let password = parts[1].to_string();

        log::debug!("Parsed username: {}, password: [REDACTED]", username);
        Some((username, password))
    }

    async fn verify(&self, credential: Credential) -> bool {
        let Some((username, password)) = self.parse_user_pass(&credential.decoded) else {
            return false;
        };

        match self.credentials.get(&username) {
            Some(hash) => {
                let verified = bcrypt::verify(password, &hash).unwrap_or(false);

                if verified {
                    let hashed_credential = BasicAuth::hash_credential(&self.cache_key, &credential.encoded);

                    let is_cached = self.hashed_credential_cache.read().await.contains(&hashed_credential);
                    if !is_cached {
                        let mut cache = self.hashed_credential_cache.write().await;
                        cache.insert(hashed_credential);
                    }
                }

                log::debug!("Password verification for user {}: {}", username, verified);
                verified
            },
            None => {
                log::debug!("User {} not found in credentials", username);
                false
            }
        }
    }
}

pub async fn basic_auth(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next
) -> Result<Response, impl IntoResponse> {
    let mut headers = HeaderMap::new();
    headers.insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Basic realm=\"PDU Exporter\""));

    let is_valid_userpass;

    {
        if state.basic_auth.credentials.is_empty() {
            log::debug!("No credentials configured, skipping auth");
            return Ok(next.run(request).await);
        }

        let auth_header = request.headers()
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|header| header.to_str().ok());
    
        let Some(auth_header) = auth_header else {
            log::debug!("No Authorization header found");
            return Err((StatusCode::UNAUTHORIZED, headers))
        };

        let Some(credential) = BasicAuth::is_valid_basic_auth_header(auth_header) else {
            return Err((StatusCode::UNAUTHORIZED, headers))
        };

        {
            let hashed_credential = BasicAuth::hash_credential(&state.basic_auth.cache_key, &credential.encoded);

            let is_cached = state.basic_auth.hashed_credential_cache.read().await.contains(&hashed_credential);
            if is_cached {
                log::debug!("Authorization header found in cache, skipping further verification");
                return Ok(next.run(request).await);
            }
        }

        is_valid_userpass = state.basic_auth.verify(credential).await;
    }

    if is_valid_userpass {
        log::debug!("Authorization successful");
        Ok(next.run(request).await)
    } else {
        log::debug!("Authorization failed");
        Err((StatusCode::UNAUTHORIZED, headers))
    }
}