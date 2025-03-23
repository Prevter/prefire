#[macro_use]
extern crate rocket;

use base64::Engine;
use base64::engine::general_purpose::STANDARD_NO_PAD;
use rocket::form::Form;
use rocket::fs::NamedFile;
use rocket::futures::{SinkExt, StreamExt};
use rocket::http::{ContentType, Cookie, CookieJar, Header, Status};
use rocket::response::{Redirect, Responder, content};
use rocket::tokio::fs;
use rocket::tokio::io::AsyncWriteExt;
use rocket::{response, tokio};
use rocket_db_pools::sqlx::{self, Row};
use rocket_db_pools::{Connection, Database};
use rocket_dyn_templates::{Template, context};
use std::ops::DerefMut;
use std::path::PathBuf;
use uuid::Uuid;

/// == Database == ///
#[derive(Database)]
#[database("files_db")]
struct Files(sqlx::SqlitePool);

/// == Static Files == ///

#[get("/style.css")]
fn style() -> content::RawCss<&'static str> {
    content::RawCss(include_str!("www/style.css"))
}

#[get("/index.js")]
fn index_js() -> content::RawJavaScript<&'static str> {
    content::RawJavaScript(include_str!("www/index.js"))
}

#[get("/upload.js")]
fn upload_js() -> content::RawJavaScript<&'static str> {
    content::RawJavaScript(include_str!("www/upload.js"))
}

#[get("/img/prefire.svg")]
fn prefire_logo() -> (ContentType, &'static [u8]) {
    (ContentType::SVG, include_bytes!("www/img/prefire.svg"))
}

#[get("/")]
fn index() -> content::RawHtml<&'static str> {
    content::RawHtml(include_str!("www/index.html"))
}

/// == File Utilities == ///
fn get_crc32(file: &str) -> u32 {
    use crc32fast::Hasher;
    use std::fs::File;
    use std::io::{BufReader, Read};

    let mut hasher = Hasher::new();
    let mut reader = BufReader::new(File::open(file).unwrap());
    let mut buffer = [0; 1024];
    loop {
        let bytes_read = reader.read(&mut buffer).unwrap();
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    hasher.finalize()
}

fn get_sha256(file: &str) -> String {
    use sha2::{Digest, Sha256};
    use std::fs::File;
    use std::io::{BufReader, Read};

    let mut hasher = Sha256::new();
    let mut reader = BufReader::new(File::open(file).unwrap());
    let mut buffer = [0; 1024];
    loop {
        let bytes_read = reader.read(&mut buffer).unwrap();
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    format!("{:x}", hasher.finalize())
}

fn format_size(size: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut size = size as f64;
    let mut unit = 0;
    while size >= 1024.0 {
        size /= 1024.0;
        unit += 1;
    }
    format!("{:.2} {}", size, units[unit])
}

fn format_date(date: i64) -> String {
    use chrono::{DateTime, Utc};
    let date = DateTime::<Utc>::from_timestamp(date, 0);
    match date {
        Some(date) => date.format("%m/%d/%Y, %I:%M:%S %p").to_string(),
        None => "Unknown".to_string(),
    }
}

#[derive(serde::Serialize)]
struct FileInfo {
    id: i64,
    name: String,
    stored_name: String,
    size: i64,
    file_type: String,
    created_at: String,
    sha256: String,
}

async fn get_file_info(db: &mut Connection<Files>, id: &str) -> Option<FileInfo> {
    let row = sqlx::query("SELECT * FROM files WHERE stored_name = ?")
        .bind(id)
        .fetch_one(&mut **db.deref_mut())
        .await
        .ok()?;
    Some(FileInfo {
        id: row.get("id"),
        name: row.get("name"),
        stored_name: row.get("stored_name"),
        size: row.get("size"),
        file_type: row.get("type"),
        created_at: row.get("created_at"),
        sha256: row.get("sha256"),
    })
}

async fn get_file_downloads(mut db: Connection<Files>, id: i64) -> Option<u32> {
    let row = sqlx::query("SELECT count FROM downloads WHERE file_id = ?")
        .bind(id)
        .fetch_one(&mut **db)
        .await
        .ok()?;

    Some(row.get("count"))
}

async fn increment_file_downloads(mut db: Connection<Files>, id: i64) {
    sqlx::query("INSERT INTO downloads(file_id, count) VALUES (?, 1) ON CONFLICT(file_id) DO UPDATE SET count = count + 1")
        .bind(id)
        .execute(&mut **db)
        .await
        .unwrap();
}

async fn check_hash(db: &mut Connection<Files>, hash: &str, is_sha256: bool) -> Option<String> {
    let row = if is_sha256 {
        sqlx::query("SELECT stored_name FROM files WHERE sha256 = ?")
            .bind(hash)
            .fetch_one(&mut **db.deref_mut())
            .await
            .ok()?
    } else {
        sqlx::query("SELECT stored_name FROM files WHERE crc32 = ?")
            .bind(hash)
            .fetch_one(&mut **db.deref_mut())
            .await
            .ok()?
    };
    Some(row.get("stored_name"))
}

fn get_preview_code(file_type: &str, id: &str, name: &str) -> String {
    // get a preview HTML code for the file type
    match file_type.split('/').next().unwrap() {
        "image" => format!(
            "<img src=\"/f/{id}/preview\" alt=\"{name}\" class=\"preview\">",
            id = id,
            name = name
        ),
        "audio" => format!(
            "<audio controls class=\"preview\"><source src=\"/f/{id}/preview\" type=\"{file_type}\"></audio>",
            id = id,
            file_type = file_type
        ),
        "video" => format!(
            "<video controls class=\"preview\"><source src=\"/f/{id}/preview\" type=\"{file_type}\"></video>",
            id = id,
            file_type = file_type
        ),
        _ => "".to_string(),
    }
}

/// == File Viewer == ///

struct FileWithHeaders {
    file: NamedFile,
    mime_type: String,
    filename: String,
}

impl<'r> Responder<'r, 'static> for FileWithHeaders {
    fn respond_to(self, req: &'r rocket::Request<'_>) -> response::Result<'static> {
        let mut response = self.file.respond_to(req)?;
        response.set_header(Header::new("Content-Type", self.mime_type));
        response.set_header(Header::new(
            "Content-Disposition",
            format!("inline; filename=\"{}\"", self.filename),
        ));
        Ok(response)
    }
}

#[get("/f/<id>/download")]
async fn file_download(mut db: Connection<Files>, id: &str) -> Result<FileWithHeaders, Status> {
    let file_info = get_file_info(&mut db, id).await.ok_or(Status::NotFound)?;
    increment_file_downloads(db, file_info.id).await;

    let file_path = PathBuf::from(format!("uploads/{}", id));
    let named_file = NamedFile::open(&file_path)
        .await
        .map_err(|_| Status::NotFound)?;

    // Build a response with a custom header
    let response = FileWithHeaders {
        file: named_file,
        mime_type: file_info.file_type,
        filename: file_info.name,
    };

    Ok(response)
}

#[get("/f/<id>/preview")]
async fn file_preview(mut db: Connection<Files>, id: &str) -> Result<FileWithHeaders, Status> {
    let file_info = get_file_info(&mut db, id).await.ok_or(Status::NotFound)?;
    let file_path = PathBuf::from(format!("uploads/{}", id));
    let named_file = NamedFile::open(&file_path)
        .await
        .map_err(|_| Status::NotFound)?;

    // Build a response with a custom header
    let response = FileWithHeaders {
        file: named_file,
        mime_type: file_info.file_type,
        filename: file_info.name,
    };

    Ok(response)
}

#[get("/f/<id>")]
async fn file_overview(mut db: Connection<Files>, id: &str) -> Template {
    match get_file_info(&mut db, id).await {
        Some(file_info) => {
            let preview = get_preview_code(&file_info.file_type, id, &file_info.name);
            Template::render(
                "file",
                context! {
                    id: id,
                    name: file_info.name,
                    size: format_size(file_info.size as u64),
                    date: file_info.created_at,
                    preview: preview,
                    filetype: file_info.file_type,
                    filehash: file_info.sha256,
                    downloads: get_file_downloads(db, file_info.id).await.unwrap_or(0),
                },
            )
        }
        None => Template::render("missing", context! {}),
    }
}

/// == File Upload == ///

#[get("/upload")]
fn upload(mut db: Connection<Files>, ws: ws::WebSocket) -> ws::Channel<'static> {
    ws.channel(move |mut stream| {
        Box::pin(async move {
            let (_size, chunks, name) = match stream.next().await {
                Some(Ok(message)) => {
                    let message = message.into_data();
                    if message.len() < 16 {
                        return Ok(());
                    }
                    let size = u64::from_le_bytes([
                        message[0], message[1], message[2], message[3], message[4], message[5],
                        message[6], message[7],
                    ]);
                    let chunks = u64::from_le_bytes([
                        message[8],
                        message[9],
                        message[10],
                        message[11],
                        message[12],
                        message[13],
                        message[14],
                        message[15],
                    ]);
                    let name = String::from_utf8(message[16..].to_vec()).unwrap();
                    (size, chunks, name)
                }
                _ => return Ok(()),
            };

            // ensure uploads directory exists
            fs::create_dir_all("uploads").await.unwrap();

            // open a file stream
            let id = Uuid::new_v4().to_string().replace("-", "");
            let filename = format!("uploads/{}", id);
            let file = fs::File::create(&filename).await.unwrap();
            let mut file = tokio::io::BufWriter::new(file);

            // send a message to the client to start sending chunks
            stream.send(vec![0].into()).await.unwrap();

            // write chunks to file
            for _ in 0..chunks {
                let chunk = match stream.next().await {
                    Some(Ok(message)) => message.into_data(),
                    _ => {
                        // if the stream ends early, delete the file
                        tokio::fs::remove_file(filename).await.unwrap();
                        return Ok(());
                    }
                };
                file.write_all(&chunk).await.unwrap();
            }

            // close the file stream
            file.flush().await.unwrap();

            // get the file hash
            let sha256 = get_sha256(&filename);

            // check if CRC32 hash already exists
            if let Some(id) = check_hash(&mut db, &sha256.to_string(), true).await {
                // delete the file if it already exists
                tokio::fs::remove_file(filename).await.unwrap();
                stream.send(id.as_bytes().to_vec().into()).await.unwrap();
                return Ok(());
            }

            // save the file to the database
            let file_type = mime_guess::from_path(&name).first_or_octet_stream().to_string();
            let file_size = fs::metadata(&filename).await.unwrap().len();
            let crc32 = get_crc32(&filename);
            sqlx::query("INSERT INTO files(name, stored_name, size, type, created_at, sha256, crc32) VALUES (?, ?, ?, ?, ?, ?, ?)")
                .bind(name)
                .bind(&id)
                .bind(file_size as i64)
                .bind(file_type)
                .bind(format_date(chrono::Utc::now().timestamp()))
                .bind(sha256)
                .bind(crc32.to_string())
                .execute(&mut **db)
                .await
                .unwrap();

            // send a message to the client that the file was uploaded with the file id
            stream.send(id.as_bytes().to_vec().into()).await.unwrap();

            Ok(())
        })
    })
}

/// == Admin == ///

fn verify_admin_cookie(cookie_jar: &CookieJar<'_>) -> bool {
    let admin_cookie = cookie_jar.get("admin");
    if admin_cookie.is_some() {
        let auth_secret = std::env::var("ADMIN_COOKIE").unwrap();
        return admin_cookie.unwrap().value() == auth_secret;
    }
    false
}

#[derive(FromForm)]
struct LoginInput<'r> {
    username: &'r str,
    password: &'r str,
}

#[post("/admin", data = "<login_input>")]
fn login(login_input: Form<LoginInput<'_>>, cookie_jar: &CookieJar<'_>) -> Redirect {
    let auth_secret = STANDARD_NO_PAD
        .encode(format!("{}:{}", login_input.username, login_input.password).as_bytes());
    cookie_jar.add(Cookie::new("admin", auth_secret));
    Redirect::to(uri!(admin))
}

#[get("/d/<id>")]
async fn delete_file(mut db: Connection<Files>, id: &str, cookie_jar: &CookieJar<'_>) -> Redirect {
    if !verify_admin_cookie(cookie_jar) {
        return Redirect::to(uri!(admin));
    }

    let file_id: i64 = sqlx::query("SELECT id FROM files WHERE stored_name = ?")
        .bind(id)
        .fetch_one(&mut **db)
        .await
        .unwrap()
        .get("id");

    let file_path = PathBuf::from(format!("uploads/{}", id));
    tokio::fs::remove_file(file_path).await.unwrap_or_default();
    sqlx::query("DELETE FROM downloads WHERE file_id = ?")
        .bind(file_id)
        .execute(&mut **db)
        .await
        .unwrap_or_default();
    sqlx::query("DELETE FROM files WHERE stored_name = ?")
        .bind(id)
        .execute(&mut **db)
        .await
        .unwrap_or_default();

    Redirect::to(uri!(admin))
}

#[get("/admin")]
async fn admin(mut db: Connection<Files>, cookie_jar: &CookieJar<'_>) -> Template {
    if !verify_admin_cookie(cookie_jar) {
        return Template::render("login", context! {});
    }

    let files = sqlx::query("SELECT * FROM files")
        .fetch_all(&mut **db)
        .await
        .unwrap()
        .into_iter()
        .map(|row| FileInfo {
            id: row.get("id"),
            name: row.get("name"),
            stored_name: row.get("stored_name"),
            size: row.get("size"),
            file_type: row.get("type"),
            created_at: row.get("created_at"),
            sha256: row.get("sha256"),
        })
        .collect::<Vec<_>>();
    Template::render(
        "admin",
        context! {
            files: files,
        },
    )
}

#[launch]
fn rocket() -> _ {
    // check if ADMIN_COOKIE is set
    if std::env::var("ADMIN_COOKIE").is_err() {
        panic!("ADMIN_COOKIE environment variable is not set");
    }
    
    rocket::build()
        .attach(Files::init())
        .attach(Template::fairing())
        .mount(
            "/",
            routes![
                index,
                style,
                index_js,
                upload_js,
                prefire_logo,
                upload,
                file_download,
                file_preview,
                file_overview,
                admin,
                login,
                delete_file,
            ],
        )
}
