use crate::{config, Error};

use axum::http::HeaderValue;
use http::{method::Method, HeaderName};
use std::str::FromStr;
use tower_http::cors::CorsLayer;

#[derive(Debug)]
pub struct Cors;

impl Cors {
    pub fn new(config: config::Cors) -> Result<CorsLayer, Error> {
        let mut cors = CorsLayer::new();

        if config.allow_credentials {
            cors = cors.allow_credentials(true);
        }

        if !config.allow_origins.is_empty() {
            let origins = Cors::get_allow_origins(&config.allow_origins)?;
            cors = cors.allow_origin(origins);
        }

        if !config.allow_methods.is_empty() {
            let methods = Cors::get_allow_methods(&config.allow_methods)?;
            cors = cors.allow_methods(methods);
        }

        if !config.allow_headers.is_empty() {
            let headers = Cors::get_allow_headers(&config.allow_headers)?;
            cors = cors.allow_headers(headers);
        }

        Ok(cors)
    }

    fn get_allow_origins(origins: &[String]) -> Result<Vec<HeaderValue>, Error> {
        origins
            .iter()
            .map(|origin| {
                origin.parse::<HeaderValue>().map_err(|e| {
                    let err_msg = format!("failed to parse origin {}: {}", origin, e);
                    log::error!("{}", err_msg);
                    Error::from(err_msg)
                })
            })
            .collect()
    }

    fn get_allow_methods(methods: &[String]) -> Result<Vec<Method>, Error> {
        methods
            .iter()
            .map(|method| {
                Method::from_str(method).map_err(|e| {
                    let err_msg = format!("failed to parse method {}: {}", method, e);
                    log::error!("{}", err_msg);
                    Error::from(err_msg)
                })
            })
            .collect()
    }

    fn get_allow_headers(headers: &[String]) -> Result<Vec<HeaderName>, Error> {
        headers
            .iter()
            .map(|header| {
                HeaderName::from_str(header).map_err(|e| {
                    let err_msg = format!("failed to parse header {}: {}", header, e);
                    log::error!("{}", err_msg);
                    Error::from(err_msg)
                })
            })
            .collect()
    }
}