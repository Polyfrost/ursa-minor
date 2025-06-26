// Ursa Minor - A Hypixel API proxy
// Copyright (C) 2023 Linnea Gräf
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use hyper::{Body, Response};
use prometheus::{Encoder, TextEncoder};

use crate::{global_application_config, make_error, require_login, RequestContext};

pub const BUILD_VERSION: &str = env!("GIT_HASH");

async fn respond_to_metrics(req: RequestContext) -> anyhow::Result<Response<Body>> {
    // Validate authorization if a token was added to the config
    match &global_application_config.metrics_bearer_token {
        Some(required_token) => match req.request.headers().get("Authorization") {
            Some(auth_header)
                if auth_header.as_bytes().len() > 7
                    && &auth_header.as_bytes()[0..7] == b"Bearer "
                    && &auth_header.as_bytes()[7..] == required_token.as_bytes() =>
            {
                ()
            }
            Some(_) => {
                return make_error(401, "Incorrect metrics bearer token authorization passed")
            }
            None => return make_error(401, "Missing metrics bearer token"),
        },
        None => (),
    }

    let encoder = TextEncoder::new();
    let metrics = global_application_config.prometheus.registry.gather();

    return Ok(Response::builder()
        .header("content-type", encoder.format_type())
        .body(encoder.encode_to_string(&metrics)?.into())?);
}

pub async fn respond_to_meta(
    req: RequestContext,
    meta_path: &str,
) -> anyhow::Result<Response<Body>> {
    if meta_path == "version" {
        return Ok(Response::builder()
            .status(200)
            .body(debug_string().into())?);
    } else if meta_path == "metrics" {
        return respond_to_metrics(req).await;
    }
    let (save, principal) = require_login!(req);
    let response = if meta_path == "principal" {
        Response::builder()
            .status(200)
            .body(format!("{principal:#?}").into())?
    } else {
        make_error(404, format!("Unknown meta request {meta_path}").as_str())?
    };
    save.save_to(response)
}

pub fn debug_string() -> String {
    format!(
        "ursa-minor {} https://github.com/NotEnoughUpdates/ursa-minor/\nfeatures: {}",
        BUILD_VERSION,
        crate::built_info::FEATURES_STR
    )
}
