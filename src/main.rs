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

#![feature(lazy_cell)]
#![feature(adt_const_params)]
#![allow(incomplete_features)]
extern crate core;

use std::env;
use std::net::SocketAddr;
use std::time::Duration;
use std::str::FromStr;

use anyhow::Context as _;
use hmac::digest::KeyInit;
use hmac::Hmac;
use hyper::client::HttpConnector;
use hyper::service::{make_service_fn, service_fn};
use hyper_rustls::{HttpsConnector, ConfigBuilderExt};
use hyper::{Body, Client, Request, Response, Server};
use rustls::ClientConfig;
use rustls::client::Resumption;

use crate::hypixel::Rule;
use crate::meta::respond_to_meta;
use crate::util::Obscure;

pub mod hypixel;
pub mod meta;
pub mod mojang;
pub mod util;

#[derive(Debug)]
pub struct RequestContext {
    redis_client: Obscure<redis::aio::ConnectionManager, "ConnectionManager">,
    request: Request<Body>,
}

#[derive(Debug)]
pub struct GlobalApplicationContext {
    client: Client<HttpsConnector<HttpConnector>>,
    hypixel_token: Obscure<String>,
    host: SocketAddr,
    rules: Vec<Rule>,
    allow_anonymous: bool,
    // Use sha384 to prevent against length extension attacks
    key: Hmac<sha2::Sha384>,
    redis_url: Obscure<String>,
    default_token_duration: Duration,
    rate_limit_lifespan: Duration,
    rate_limit_bucket: u64,
    git_url: String,
}

fn make_error(status_code: u16, error_text: &str) -> anyhow::Result<Response<Body>> {
    Ok(Response::builder()
        .status(status_code)
        .body(format!("{} {}", status_code, error_text).into())?)
}

async fn respond_to(mut context: RequestContext) -> anyhow::Result<Response<Body>> {
    let path = &context.request.uri().path().to_owned();
    if path == "/" {
        return Ok(Response::builder()
            .status(302)
            .header("Location", &global_application_config.git_url)
            .body(Body::empty())?);
    }
    if let Some(meta_path) = path.strip_prefix("/_meta/") {
        return respond_to_meta(context, meta_path).await;
    }

    if let Some(hypixel_path) = path.strip_prefix("/v1/hypixel/") {
        let (save, principal) = require_login!(context);
        if let Some(resp) = hypixel::respond_to(&mut context, hypixel_path, principal).await? {
            return save.save_to(resp);
        }
    }
    return make_error(404, format!("Unknown request path {}", path).as_str());
}

async fn wrap_error(context: RequestContext) -> anyhow::Result<Response<Body>> {
    let start = std::time::Instant::now();
    let resp = respond_to(context).await;
    let end = std::time::Instant::now();
    let time_passed = end - start;
    let mut final_resp = match resp {
        Ok(x) => x,
        Err(e) => {
            let error_id = uuid::Uuid::new_v4();
            eprintln!("Error id: {error_id}:");
            eprintln!("{e:#?}");
            Response::builder()
                .status(500)
                .body(format!("500 Internal Error\n\nError id: {}", error_id).into())?
        }
    };
    final_resp.headers_mut().insert(
        "x-ursa-timings",
        format!("{}ns", time_passed.as_nanos()).try_into()?,
    );
    return Ok(final_resp);
}

fn config_var(name: &str) -> anyhow::Result<String> {
    env::var(format!("URSA_{}", name)).with_context(|| {
        format!(
            "Could not find {} expected to be found in the environment at URSA_{}",
            name, name
        )
    })
}

#[allow(non_upper_case_globals)]
static global_application_config: std::sync::LazyLock<GlobalApplicationContext> =
    std::sync::LazyLock::new(|| init_config().unwrap());

fn init_config() -> anyhow::Result<GlobalApplicationContext> {
    if let Ok(path) = dotenv::dotenv() {
        println!("Loaded dotenv from {}", path.to_str().unwrap_or("?"));
    }
    let hypixel_token = config_var("HYPIXEL_TOKEN")?;
    let allow_anonymous = config_var("ANONYMOUS").unwrap_or("false".to_owned()) == "true";
    let rules = config_var("RULES")?
        .split(':')
        .map(|it| {
            std::fs::read(it)
                .map_err(anyhow::Error::from)
                .and_then(|it| {
                    serde_json::from_slice::<hypixel::Rule>(&it).map_err(anyhow::Error::from)
                })
        })
        .collect::<Result<Vec<Rule>, _>>()?;
    let host = SocketAddr::from_str(&config_var("HOST")?)
        .with_context(|| "Could not parse host at URSA_HOST")?;
    let token_lifespan = config_var("TOKEN_LIFESPAN")
        .unwrap_or("3600".to_owned())
        .parse::<u64>()
        .with_context(|| "Could not parse token lifespan at URSA_TOKEN_LIFESPAN")?;
    let secret = config_var("SECRET")?;
    let https = hyper_rustls::HttpsConnectorBuilder::new()
        .with_tls_config({
            let mut cfg = ClientConfig::builder()
                .with_safe_defaults()
                .with_native_roots()
                .with_no_client_auth();
            cfg.resumption = Resumption::disabled();
            cfg
        })
        .https_only()
        .enable_http1()
        .build();

    let client = Client::builder().build::<_, Body>(https);
    let redis_url = config_var("REDIS_URL")?;
    let rate_limit_lifespan =
        Duration::from_secs(config_var("RATE_LIMIT_TIMEOUT")?.parse::<u64>()?);
    let rate_limit_bucket = config_var("RATE_LIMIT_BUCKET")?.parse::<u64>()?;
    let git_url = config_var("GIT_URL")?;
    Ok(GlobalApplicationContext {
        client,
        host,
        hypixel_token: Obscure(hypixel_token),
        rules,
        allow_anonymous,
        key: Hmac::new_from_slice(secret.as_bytes())?,
        redis_url: Obscure(redis_url),
        default_token_duration: Duration::from_secs(token_lifespan),
        rate_limit_lifespan,
        rate_limit_bucket,
        git_url
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if env::args().any(|it| &it == "--version") {
        println!("{}", meta::debug_string());
        return Ok(());
    }
    println!("Ursa minor rises above the sky!");
    println!(
        "Launching with configuration: {:#?}",
        *global_application_config
    );
    let redis_client = redis::Client::open(global_application_config.redis_url.clone())?;
    let managed = redis::aio::ConnectionManager::new(redis_client).await?;
    let service = make_service_fn(|_conn| {
        let client = managed.clone();
        async {
            Ok::<_, anyhow::Error>(service_fn(move |req| {
                wrap_error(RequestContext {
                    redis_client: Obscure(client.clone()),
                    request: req,
                })
            }))
        }
    });
    let server = Server::bind(&global_application_config.host).serve(service);
    println!("Now listening at {}", global_application_config.host);
    let mut s = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! {
        _ = s.recv() => {
            println!("Terminated! It's time to say goodbye.");
        }
        it = tokio::signal::ctrl_c() => {
            it?;
            println!("Interrupted! It's time to say goodbye.");
        }
        it = server =>{
            it?;
        }
    }
    Ok(())
}
