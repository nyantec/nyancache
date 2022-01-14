#[macro_use]
extern crate diesel;

mod error;
mod models;
mod nixutils;
mod schema;
mod backend;

use std::collections::BTreeMap;
use std::io::Cursor;
use std::str::FromStr;
use std::sync::Arc;

use error::{Error, Result};
use models::DbPath;
use nixutils::NarInfo;
use schema::paths::dsl::paths;
use schema::paths::{id as db_id, url as db_url};
use backend::{Backend, local::LocalBackend, NarResponder};

use diesel::RunQueryDsl;
use diesel::QueryDsl;
use diesel::ExpressionMethods;
use log::warn;
use rocket::data::ToByteUnit;
use rocket::http::{ContentType, Status};
use rocket::request::FromParam;
use rocket::response::{Responder, Response};
use rocket::Request;
use rocket_sync_db_pools::{database, diesel as rocket_diesel};
use tokio::sync::Mutex;
use s3::Bucket;
use s3::creds::Credentials;


#[database("sqlite_nyancache")]
struct DbConn(rocket_diesel::SqliteConnection);

#[rocket::get("/nix-cache-info")]
fn nix_cache_info() -> &'static str {
    r"StoreDir: /nix/store
WantMassQuery: 1
Priority: 40"
}

macro_rules! generate_fromparam_ext {
    ($struct_name: ident, $ext: expr) => {
        struct $struct_name<'a>(&'a str);
        impl<'a> FromParam<'a> for $struct_name<'a> {
            type Error = ();

            fn from_param(param: &'a str) -> std::result::Result<Self, Self::Error> {
                match param.strip_suffix($ext) {
                    None => Err(()),
                    Some(x) => FromParam::from_param(x)
                        .map(|x| $struct_name(x))
                        .map_err(|_| ()),
                }
            }
        }
    };
}

generate_fromparam_ext!(NarinfoName, ".narinfo");
generate_fromparam_ext!(NarXzName, ".nar.xz");

#[rocket::get("/<name>")]
async fn get_narinfo(
    conn: DbConn,
    name: NarinfoName<'_>,
) -> Result<String> {
    let id = name.0.to_string();
    let matches = conn.run(move |c| {
        paths.filter(db_id.eq(id)).load::<DbPath>(c)
    })
    .await?;
    let db_path = matches.get(0).cloned().ok_or(Error::NotFound)?;
    let nar_info: NarInfo = db_path.into();

    Ok(nar_info.to_string())
}

#[rocket::put("/<name>", data = "<input>")]
async fn put_narinfo(
    conn: DbConn,
    name: NarinfoName<'_>,
    input: &str,
    state: &rocket::State<Arc<State>>,
) -> Result<()> {
    let mut nar_info = DbPath::from(NarInfo::from_str(input)?);
    nar_info.id = name.0.to_string();
    if let Some(url) = nar_info.url.clone().and_then(|full| full.strip_prefix("nar/").map(|x| x.to_string())) {
        add_incomplete(&conn, state, &url, IncompleteUpload::NarInfo(nar_info)).await?;
    } else {
        warn!("narinfo missing url");
    }
    Ok(())
}

#[rocket::get("/nar/<name>")]
async fn get_nar(
    conn: DbConn,
    name: NarXzName<'_>,
    state: &rocket::State<Arc<State>>,
) -> Result<NarResponder> {
    let id = name.0.to_string();
    let matches = conn.run(move |c| {
        paths.filter(db_url.eq(&format!("nar/{}.nar.xz", id))).load::<DbPath>(c)
    })
    .await?;
    let _db_path = matches.get(0).cloned().ok_or(Error::NotFound)?;

    let url = format!("{}.nar.xz", name.0);
    Ok(state.backend.read_nar(&url).await?)
}

#[rocket::head("/nar/<name>")]
async fn head_nar(
    conn: DbConn,
    name: NarXzName<'_>,
    state: &rocket::State<Arc<State>>,
) -> Result<()> {
    let id = name.0.to_string();
    let matches = conn.run(move |c| {
        paths.filter(db_url.eq(&format!("nar/{}.nar.xz", id))).load::<DbPath>(c)
    })
    .await?;
    let _db_path = matches.get(0).cloned().ok_or(Error::NotFound)?;
    Ok(())
}

#[rocket::put("/nar/<name>", data = "<data>")]
async fn put_nar(
    conn: DbConn,
    name: NarXzName<'_>,
    data: rocket::Data<'_>,
    state: &rocket::State<Arc<State>>,
) -> Result<()> {
    let url = format!("{}.nar.xz", name.0);
    state.backend.write_nar(&url, &mut data.open(10.gigabytes())).await?;
    add_incomplete(&conn, state, &url, IncompleteUpload::Nar).await?;
    Ok(())
}

async fn add_incomplete(
    conn: &DbConn,
    state: &rocket::State<Arc<State>>,
    url: &str,
    part: IncompleteUpload,
) -> Result<()> {
    if let Some(nar_info) = {
        let mut queued_uploads = state.queued_uploads.lock().await;
        match (part, queued_uploads.remove(&url.to_string())) {
            (IncompleteUpload::Nar, Some(IncompleteUpload::NarInfo(nar_info))) => {
                Some(nar_info)
            }
            (IncompleteUpload::NarInfo(nar_info), Some(IncompleteUpload::Nar)) => {
                Some(nar_info)
            }
            (part, _) => {
                queued_uploads.insert(url.to_string(), part);
                None
            }
        }
    } {
        complete_upload(conn, state, url, nar_info).await?;
    }
    Ok(())
}

async fn complete_upload(conn: &DbConn, state: &rocket::State<Arc<State>>, url: &str, nar_info: DbPath) -> Result<()> {
    state.backend.finish_nar(&url).await?;
    conn.run(move |c| {
        diesel::insert_into(paths)
            .values(DbPath::from(nar_info))
            .execute(c)
    })
    .await?;
    Ok(())
}

#[derive(Debug)]
pub enum IncompleteUpload {
    Nar,
    NarInfo(DbPath),
}

struct State {
    queued_uploads: Mutex<BTreeMap<String, IncompleteUpload>>,
    backend: Box<dyn Backend + Send + Sync>,
}

#[rocket::launch]
async fn rocket() -> _ {
    let backend = {
        let bucket_name = "yuka-testbucket";
        let region = "eu-central-1".parse().unwrap();
        let credentials = Credentials::default().unwrap();
        let bucket = Bucket::new(bucket_name, region, credentials).unwrap();

        /*let mut reader = Cursor::new("foo");
        let path = String::from("foo");
        bucket.put_object_stream(&mut reader, path).await.unwrap();*/

        Box::new(bucket)
        //Box::new(LocalBackend::new_current_dir().unwrap())
    };
    let state = Arc::new(State {
        queued_uploads: Default::default(),
        backend,
    });

    rocket::build()
        .manage(state)
        .attach(DbConn::fairing())
        .mount(
            "/",
            rocket::routes![
                nix_cache_info,
                get_narinfo,
                put_narinfo,
                get_nar,
                put_nar,
            ],
        )
}
