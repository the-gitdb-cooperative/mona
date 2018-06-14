extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate byteorder;
extern crate gitdb;
extern crate base64;

mod error;
mod account;
mod core;
use std::io::{self, Read, Write};

use error::{Result, Error};
use account::Account;
use core::State;

#[derive(Debug, Deserialize)]
enum Cmd {
    Login {
        pass: String
    },
    AccountQuery {
        query: String
    },
    GetAccount {
        account: String
    }
}

#[derive(Debug, Serialize)]
enum Response {
    NotInitialized, // returned if a request is made before a successfull login
    Login { success: bool },
    AccountQuery { results: Vec<String> },
    GetAccount { account: String, creds: Vec<Account> },
    NoAccount { account: String },
    UnknownError { msg: String }
}

fn recv<T: serde::de::DeserializeOwned>() -> Result<T> {
    let stdin = io::stdin();
    let mut handle = stdin.lock();

    use byteorder::ReadBytesExt;
    let length = handle.read_u32::<byteorder::NativeEndian>()?;
    eprintln!("reading {} bytes", length);
    // TODO: but some sanity checks on this length
    let mut buf = vec![0u8; length as usize];
    handle.read_exact(&mut buf)?;
    eprintln!("got: {} ", String::from_utf8(buf.clone()).unwrap());
    // TAI: we are discarding length information here.. ok? bad?
    let data: T = serde_json::from_slice(&buf)?;
    Ok(data)
}

fn send(msg: impl serde::Serialize) -> Result<()>{
    let data = serde_json::to_vec(&msg)?;
    eprintln!("sending: {}", String::from_utf8(data.clone()).unwrap());
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    use byteorder::WriteBytesExt;
    // TODO: ff will restrict amount of data from app to WebExtensions to 4mb, check this here
    handle.write_u32::<byteorder::NativeEndian>(data.len() as u32)?;
    handle.write_all(&data)?;
    handle.flush()?;
    Ok(())
}

fn login(pass: &str) -> Result<Option<State>> {
    let root = &core::default_root()?;
    let state = match State::init(&root, pass.as_bytes()) {
        Ok(s) => {
            if let Ok(()) = s.validate_encryption_key() {
                send(Response::Login{ success: true })?;
                Some(s)
            } else {
                send(Response::Login{ success: false })?;
                None
            }
        },
        Err(e) => {
            eprintln!("Failed in login attempt: {}", e);
            send(Response::UnknownError {
                msg: "Failed login, inspect logs".into()
            })?;
            None
        }
    };
    Ok(state)
}

fn main() -> Result<()> {
    let mut state: Option<State> = None;
    loop {
        eprintln!("loop");
        match recv()? {
            Cmd::Login { pass } => {
                state = login(&pass)?;
            },
            Cmd::AccountQuery { query } => {
                if let Some(ref s) = state {
                    send(Response::AccountQuery {
                        results: s.account_query(query)?.collect()
                    })?;
                } else {
                    send(Response::NotInitialized)?;
                }
            },
            Cmd::GetAccount { account } => {
                if let Some(ref s) = state {
                    match s.account(&account) {
                        Ok(creds) => {
                            send(Response::GetAccount {
                                account: account,
                                creds: creds
                            })?;
                        },
                        Err(Error::Gitdb(gitdb::Error::NotFound)) => {
                            send(Response::NoAccount {
                                account: account
                            })?;
                        },
                        Err(e) => {
                            eprintln!("Failed GetAccount {}: {}", account, e);
                            send(Response::UnknownError {
                                msg: "Failed to GetAccount, inspect logs".into()
                            })?;
                        }
                    }
                } else {
                    send(Response::NotInitialized)?;
                }
            }
        }
    }
}
