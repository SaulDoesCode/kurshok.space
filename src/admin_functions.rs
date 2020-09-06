use actix_web::{client, post, web, http::{Cookie, HeaderName, HeaderValue}, HttpRequest, HttpResponse};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use serde_json::json;

use std::{
    collections::HashMap, 
    sync::Arc, 
    thread,
    process::Command,
};

use crate::orchestrator::Orchestrator;

use super::TEMPLATES;

pub fn watch_and_update_files() -> thread::JoinHandle<()> {
    thread::spawn(|| {      
        let (tx, rx) = flume::unbounded();  

        let mut watcher: RecommendedWatcher = Watcher::new_immediate(move |res| {
            if let Ok(event) = res {
                if let Err(err) = tx.send(event) {
                    println!("error sending file change event through channel: {}", err);
                }
            }
        }).expect("failed to setup file watcher for hot reloading related development features");

        watcher.watch("./templates", RecursiveMode::Recursive)
            .expect("watcher failed to watch ./templates");
        watcher.watch("./assets/js", RecursiveMode::Recursive)
            .expect("watcher failed to watch ./assets/js");
        watcher.watch("./assets/css", RecursiveMode::Recursive)
            .expect("watcher failed to watch ./assets/css");
  
        while let Ok(event) = rx.recv() {
            match event.kind {
                notify::EventKind::Modify(_) => {
                    for path in event.paths {
                        if !path.is_file() {continue;}
                        if path.to_str().unwrap().contains("templates") {
                            print!("reloading templates...");
                            let mut templates = TEMPLATES.write();
                            if (*templates).full_reload().is_err() {
                                println!(":( the templates were not reloaded, trouble is afoot.");
                            }
                            break;
                        }
    
                        if let Some(ext) = path.extension() {
                            let filename = String::from(path.file_name().unwrap().to_str().unwrap());
                            if filename.contains(".min.") {
                                continue;
                            }
                            if ext == "js" {
                                let res = Command::new("python")
                                    .current_dir("./assets/js")
                                    .arg("minify-all.py")
                                    .arg(&filename)
                                    .output();
        
                                if let Ok(_) = res {
                                    println!("minified {}", &filename);
                                } else if let Err(err) = res {
                                    println!("failed to minify {}, error: {:?}", &filename, err);
                                }
                            } else if ext == "css" {
                                let res = Command::new("python")
                                    .current_dir("./assets/css")
                                    .arg("minify-all.py")
                                    .arg(&filename)
                                    .output();
        
                                if let Ok(_) = res {
                                    println!("minified {}", &filename);
                                } else if let Err(err) = res {
                                    println!("failed to minify {}, error: {:?}", &filename, err);
                                }
                            }
                        }
                    }
                },
                _ => continue,
            }
        }
    })
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
                return None
            },
        };

        if let Some(headers) = &self.headers {
            for (key, value) in headers {
                if let Ok(k) = HeaderName::from_bytes(key.as_bytes()) {
                    if let Ok(v) = HeaderValue::from_str(value.as_str()) {
                        builder = builder.header(k, v);
                    } else {
                        print!("headers value fucky");
                    }
                } else {
                    print!("headers name fucky");
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
                    },
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
            println!("remote-http: sending and getting body went terrible - {}", err);
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
                    return Some(RemoteHttpResponse{
                        status,
                        headers,
                        content_type,
                        body
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
    orc: web::Data<Arc<Orchestrator>>,
) -> HttpResponse {
    if let Some(_usr) = orc.admin_by_session(&req) {
        if let Some(res) = remote_req.run().await {
            return HttpResponse::Ok().json(json!({
                "ok": true,
                "data": res
            }));
        }

        return HttpResponse::InternalServerError().body(json!({
            "ok": false,
            "status": "something went wrong",
        }));
    }

    return HttpResponse::Unauthorized().body(json!({
        "ok": false,
        "status": "remote-http is for admins only",
    }));
}
