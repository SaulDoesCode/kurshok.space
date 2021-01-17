use actix_web::{
    client,
    http::{Cookie, HeaderName, HeaderValue},
    get, post, web, HttpRequest, HttpResponse,
};
use serde::{Deserialize, Serialize};
use rayon::prelude::*;

use std::{
    collections::{HashMap, BTreeMap},
    process::Command,
};

use crate::{
    orchestrator::{ORC},
    expirable_data::{ExpirableData},
    responses,
};

use super::TEMPLATES;


pub fn watch_and_update_files() -> bool {
    Command::new("node")
        .current_dir("./scripts")
        .arg("index.js")
        .spawn().is_ok()
}


#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct RemoteHttpResponse {
    pub status: u16,
    pub content_type: String,
    pub headers: HashMap<String, String>,
    pub body: String,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct RemoteHttpRequest {
    pub method: String,
    pub url: String,
    pub content_type: Option<String>,
    pub cookies: Option<Vec<String>>,
    pub bearer_token: Option<String>,
    pub headers: Option<HashMap<String, String>>,
    pub body: Option<String>,
}

impl RemoteHttpRequest {
    pub async fn run(&self) -> Option<RemoteHttpResponse> {
        let client = client::Client::default();
        let mut builder = match self.method.to_lowercase().as_str() {
            "get" => client.get(&self.url),
            "post" => client.post(&self.url),
            "put" => client.put(&self.url),
            "patch" => client.patch(&self.url),
            "head" => client.head(&self.url),
            "options" => client.options(&self.url),
            "delete" => client.delete(&self.url),
            _ => {
                println!("remote-http: invalid method");
                return None;
            }
        };

        if let Some(headers) = &self.headers {
            for (key, value) in headers {
                if let Ok(k) = HeaderName::from_bytes(key.as_bytes()) {
                    if let Ok(v) = HeaderValue::from_str(value.as_str()) {
                        builder = builder.header(k, v);
                    } else {
                        print!("header value fucky");
                    }
                } else {
                    print!("header name fucky");
                }
            }
        }

        if let Some(cookies) = &self.cookies {
            for cookie in cookies {
                match Cookie::parse(cookie.as_str()) {
                    Ok(c) => builder = builder.cookie(c),
                    Err(_e) => {
                        println!("remote-htpp: request cookie parsing fucky");
                        return None;
                    }
                }
            }
        }

        if let Some(tk) = &self.bearer_token {
            builder = builder.bearer_auth(tk.as_str());
        }

        if let Some(ct) = &self.content_type {
            if let Ok(content_type) = HeaderValue::from_str(ct.as_str()) {
                builder = builder.content_type(content_type);
            } else {
                println!("remote-http: setting content-type went fucky");
            }
        }

        let res = if let Some(body) = &self.body {
            builder.send_body(body.clone()).await
        } else {
            builder.send().await
        };

        if res.is_err() {
            let err = res.err().unwrap();
            println!(
                "remote-http: sending and getting body went terrible - {}",
                err
            );
            return None;
        }

        if let Ok(mut res) = res {
            let status = res.status().as_u16();

            let mut content_type = String::new();

            let hmap = res.headers();
            let mut headers = HashMap::with_capacity(hmap.len() - 1);
            for (key, value) in hmap.iter() {
                if let Ok(v) = String::from_utf8(value.as_bytes().to_vec()) {
                    if key == "content-type" {
                        content_type = v;
                    } else {
                        headers.insert(key.to_string(), v);
                    }
                } else {
                    println!("remote-http: reading response header went fucky");
                }
            }

            if let Ok(raw) = res.body().await {
                if let Ok(body) = String::from_utf8(raw.to_vec()) {
                    return Some(RemoteHttpResponse {
                        status,
                        headers,
                        content_type,
                        body,
                    });
                } else {
                    println!("remote-http: reading response body to string went super fuckedly");
                }
            } else {
                println!("remote-http: reading response body went fuckedly");
            }
        }
        println!("remote-http: sending and getting a response went fucky");

        None
    }
}

#[post("/remote-http")]
pub async fn remote_http(
    req: HttpRequest,
    remote_req: web::Json<RemoteHttpRequest>,
) -> HttpResponse {
    if ORC.is_valid_admin_session(&req) {
        match remote_req.run().await {
            Some(res) => responses::Ok(res),
            None => responses::InternalServerError("something went wrong"),
        }
    } else {
        responses::Forbidden("remote-http is for admins only")
    }
}

#[get("/reload-templates")]
pub async fn reload_templates_request(req: HttpRequest) -> HttpResponse {
    if req.peer_addr().map_or(false, |ip| format!("{}", ip).contains("127.0.0.1")) || ORC.is_valid_admin_session(&req) {
        if TEMPLATES.write().full_reload().is_err() {
            println!(":( the templates were not properly reloaded, trouble is afoot.");
            return responses::InternalServerError(
                ":( the templates were not reloaded, trouble is afoot.",
            );
        }
        println!("templates reloaded!");
        return responses::Accepted("templates succesfully reloaded");
    }
    responses::Forbidden("templates may only be reloaded by admins or requests from localhost")
}

#[post("/expire-data")]
pub async fn expire_data_request(
    req: HttpRequest,
    exp_data_req: web::Data<ExpireDataRequest>,
) -> HttpResponse {
    if ORC.is_valid_admin_session(&req) {
        let exp_data_req = exp_data_req.into_inner();

        let mut exp_data: BTreeMap<String, Vec<Vec<u8>>> = BTreeMap::new();

        for (tree, keys) in exp_data_req.data.iter() {
            exp_data.insert(
                tree.clone(),
                keys.into_par_iter()
                    .map(|key| key.as_bytes().to_vec())
                    .collect()
            );
        }

        let expirable_data = ExpirableData::MultiTree(exp_data);

        let unexpire_key = exp_data_req.unexpire_key.as_ref().map(|uk| uk.as_bytes());

        if ORC.expire_data(
            exp_data_req.from_now,
            expirable_data,
            unexpire_key,
        ) {
            return responses::Accepted("data will be expired");
        }
        return responses::InternalServerError("something went wrong, data will not be expired");
    }
    responses::Forbidden("data may only be expired by admins")
}

#[derive(Serialize, Deserialize)]
pub struct ExpireDataRequest {
    pub from_now: i64,
    pub data: HashMap<String, Vec<String>>,
    pub unexpire_key: Option<String>,
}
