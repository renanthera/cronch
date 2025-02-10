use crate::error::Error;
use crate::request::{run_query_cached, run_query_uncached};
use chrono::{DateTime, Utc};
use flate2::write::{GzDecoder, GzEncoder};
use flate2::Compression;
use rusqlite::Error as RusqliteError;
use rusqlite::{Connection, OpenFlags, Row};
use serde::{Deserialize, Serialize};
use serde_json::{from_slice, to_vec};
use std::io::Write;

// TODO: close db connection
// TODO: check to make sure the correct tables exist, not just a db
// TODO: is postcard more effective than serde for serialization

pub const DBPATH: &str = "cache.db";
const CREATE_QUERY_TABLE: &str = "CREATE TABLE query (id INTEGER PRIMARY KEY, query TEXT, hits INT, time_first_request BLOB, time_last_request BLOB)";
const CREATE_RESPONSE_TABLE: &str = "CREATE TABLE response (id INTEGER PRIMARY KEY, response BLOB)";
const CREATE_TOKEN_TABLE: &str = "CREATE TABLE token (id INTEGER PRIMARY KEY, access_token TEXT, token_type TEXT, expires_in INTEGER, expires_at REAL)";
const INSERT_QUERY: &str = "INSERT INTO query (query, hits, time_first_request, time_last_request) VALUES (?1, ?2, ?3, ?4)";
const INSERT_RESPONSE: &str = "INSERT INTO response (id, response) VALUES (?1, ?2)";
const UPDATE_QUERY: &str = "UPDATE query SET hits = ?2, time_last_request = ?3 WHERE id = ?1";
const SELECT_QUERY: &str = "SELECT * FROM query WHERE query = (?1)";
const SELECT_RESPONSE: &str = "SELECT * FROM response WHERE id = (?)";

// implemented by proc macro cache_attribute::cache
pub trait Cache {
    fn run_query<U>(params: U) -> Result<Self, Error>
    where
        U: cynic::QueryVariables + serde::Serialize,
        Self: Sized
            + cynic::QueryBuilder<U>
            + serde::Serialize
            + for<'a> serde::Deserialize<'a>
            + 'static;

    fn run_query_cached<U>(params: U) -> Result<Self, Error>
    where
        U: cynic::QueryVariables + serde::Serialize,
        Self: Sized
            + cynic::QueryBuilder<U>
            + serde::Serialize
            + for<'a> serde::Deserialize<'a>
            + 'static,
    {
        run_query_cached(params)
    }

    fn run_query_uncached<U>(params: U) -> Result<Self, Error>
    where
        U: cynic::QueryVariables + serde::Serialize,
        Self: Sized
            + cynic::QueryBuilder<U>
            + serde::Serialize
            + for<'a> serde::Deserialize<'a>
            + 'static,
    {
        run_query_uncached(params)
    }
}

pub fn init_db() -> Result<Connection, RusqliteError> {
    if let Ok(conn) = Connection::open_with_flags(
        DBPATH,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        return Ok(conn);
    }

    match Connection::open(DBPATH) {
        Ok(conn) => {
            conn.execute(CREATE_QUERY_TABLE, ())?;
            conn.execute(CREATE_RESPONSE_TABLE, ())?;
            conn.execute(CREATE_TOKEN_TABLE, ())?;
            Ok(conn)
        }
        Err(err) => Err(err),
    }
}

#[derive(Debug)]
pub struct Query {
    pub id: i32,
    pub query: String,
    pub hits: i32,
    pub time_first_request: DateTime<Utc>,
    pub time_last_request: DateTime<Utc>,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct Response<T> {
    pub id: i32,
    pub response: T,
}

#[derive(Debug)]
struct InternalResponse {
    id: i32,
    response: Vec<u8>,
}

impl<T> TryFrom<InternalResponse> for Response<T>
where
    T: for<'a> Deserialize<'a>,
{
    type Error = Error;
    fn try_from(value: InternalResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            response: from_slice(&decompress(value.response)?)?,
        })
    }
}

impl<T> TryFrom<Response<T>> for InternalResponse
where
    T: Serialize,
{
    type Error = Error;
    fn try_from(value: Response<T>) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            response: compress(to_vec(&value.response)?)?,
        })
    }
}

impl Default for Query {
    fn default() -> Self {
        Query {
            id: Default::default(),
            query: Default::default(),
            hits: Default::default(),
            time_first_request: Utc::now(),
            time_last_request: Utc::now(),
        }
    }
}

pub trait SQL
where
    Self: Sized,
{
    fn select_query() -> &'static str;

    fn insert_query() -> &'static str;

    fn update_query() -> &'static str {
        "NONE"
    }

    fn from_sql(row: &Row<'_>) -> Result<Self, RusqliteError>;

    fn insert(&self, connection: &Connection) -> Result<usize, Error>;

    fn _insert<T>(&self, connection: &Connection, params: T) -> Result<usize, Error>
    where
        T: rusqlite::Params,
    {
        let mut statement = connection.prepare(Self::insert_query())?;
        Ok(statement.execute(params)?)
    }

    fn update<T>(connection: &Connection, params: T) -> Result<(), Error>
    where
        T: rusqlite::Params,
    {
        let mut statement = connection.prepare(Self::update_query())?;
        statement.query_map(params, Self::from_sql)?.for_each(drop);
        Ok(())
    }

    fn select(connection: &Connection, query: &str) -> Result<Self, Error> {
        let mut statement = connection.prepare(Self::select_query())?;
        let responses = statement.query_map((query,), Self::from_sql)?;
        match responses.last() {
            Some(Ok(last)) => Ok(last),
            Some(Err(err)) => Err(Error::Rusqlite(err)),
            None => Err(Error::NoResponseCache(query.to_string())),
        }
    }
}

impl SQL for Query {
    fn select_query() -> &'static str {
        SELECT_QUERY
    }

    fn insert_query() -> &'static str {
        INSERT_QUERY
    }

    fn update_query() -> &'static str {
        UPDATE_QUERY
    }

    fn insert(&self, connection: &Connection) -> Result<usize, Error> {
        self._insert(
            connection,
            (
                &self.query,
                &self.hits,
                &self.time_first_request,
                &self.time_last_request,
            ),
        )
    }

    fn from_sql(row: &Row<'_>) -> Result<Query, RusqliteError> {
        Ok(Query {
            id: row.get(0)?,
            query: row.get(1)?,
            hits: row.get(2)?,
            time_first_request: row.get(3)?,
            time_last_request: row.get(4)?,
        })
    }
}

impl SQL for InternalResponse {
    fn select_query() -> &'static str {
        SELECT_RESPONSE
    }

    fn insert_query() -> &'static str {
        INSERT_RESPONSE
    }

    fn insert(&self, connection: &Connection) -> Result<usize, Error> {
        self._insert(connection, (&self.id, &self.response))
    }

    fn from_sql(row: &Row<'_>) -> Result<InternalResponse, RusqliteError> {
        Ok(InternalResponse {
            id: row.get(0)?,
            response: row.get(1)?,
        })
    }
}

fn compress(response: Vec<u8>) -> Result<Vec<u8>, Error> {
    let mut e = GzEncoder::new(Vec::new(), Compression::default());
    let _ = e.write_all(&response);
    let r = e.finish()?;
    println!("compressing: {} bytes -> {} bytes", response.len(), r.len(),);
    Ok(r)
}

fn decompress(response: Vec<u8>) -> Result<Vec<u8>, Error> {
    let writer = Vec::new();
    let mut decoder = GzDecoder::new(writer);
    let _ = decoder.write_all(&response);
    let r = decoder.finish()?;
    println!(
        "decompressing: {} bytes -> {} bytes",
        response.len(),
        r.len(),
    );
    Ok(r)
}

pub fn insert<T>(query: &String, response: T) -> Result<(), Error>
where
    T: Serialize,
{
    let connection = init_db()?;
    let q = Query {
        query: query.clone(),
        ..Default::default()
    };
    q.insert(&connection)?;
    let id = Query::select(&connection, &query)?.id;
    let response = InternalResponse::try_from(Response { id, response })?;
    response.insert(&connection)?;
    Ok(())
}

pub fn select<T>(query: &String) -> Result<Response<T>, Error>
where
    T: for<'a> Deserialize<'a>,
{
    let connection = init_db()?;
    let query = Query::select(&connection, &query)?;
    let ir = InternalResponse::select(&connection, &(query.id.to_string()))?;
    Query::update(&connection, (&query.id, &query.hits + 1, Utc::now()))?;
    Ok(Response::try_from(ir)?)
}
