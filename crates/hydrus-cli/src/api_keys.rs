//! `hydrus api-keys`: Client API access keys from the command line, and
//! accepting the keys tools ask for (what the reference's "review services"
//! registration dialog does). A running `serve` notices changes within a
//! minute.

use std::io::{BufRead as _, Write as _};
use std::path::Path;
use std::time::Duration;

use anyhow::{Result, bail};
use clap::Subcommand;

use hydrus_api::auth::{self, AccessPermissions, Permission, Registration};
use hydrus_core::TagFilter;
use hydrus_store::Store;

#[derive(Subcommand)]
pub enum Action {
    /// Every access key: its name, key and what it may do.
    List,
    /// Make a new access key and print it.
    Add {
        name: String,
        /// Allow everything.
        #[arg(long)]
        all: bool,
        /// A permission to allow, by number (repeat for more; `list` shows
        /// them).
        #[arg(long = "permission")]
        permissions: Vec<u8>,
    },
    /// Remove access keys, by key or name.
    Remove { key_or_name: String },
    /// Accept the keys tools ask for (`/request_new_permissions`, e.g. a
    /// browser extension's "request new API key" button) while this runs.
    Listen {
        /// Stop after this many minutes.
        #[arg(long, default_value_t = 10)]
        minutes: u64,
        /// Accept every request without asking.
        #[arg(long)]
        yes: bool,
    },
}

pub fn run(dir: &Path, action: Action) -> Result<()> {
    let store = Store::open(dir)?;
    match action {
        Action::List => {
            let mut keys = store.read(auth::stored_keys)?;
            keys.sort_by(|a, b| a.name.cmp(&b.name));
            for key in &keys {
                println!(
                    "{}  {}\n    {}",
                    key.name,
                    hex::encode(&key.access_key),
                    key.human_description()
                );
            }
            println!("\npermissions:");
            for p in Permission::ALL {
                println!("  {:2}  {}", u8::from(p), p.description());
            }
            Ok(())
        }
        Action::Add {
            name,
            all,
            permissions,
        } => {
            let basic = permissions
                .into_iter()
                .map(|code| Permission::try_from(code).map_err(anyhow::Error::msg))
                .collect::<Result<_>>()?;
            let key = AccessPermissions {
                access_key: rand::random::<[u8; 32]>().to_vec(),
                name,
                permits_everything: all,
                basic,
                search_filter: TagFilter::default(),
            };
            println!("{}", hex::encode(&key.access_key));
            store.write(move |ctx| auth::save_key(ctx.conn(), &key))?;
            Ok(())
        }
        Action::Remove { key_or_name } => {
            let keys = store.read(auth::stored_keys)?;
            let doomed: Vec<Vec<u8>> = keys
                .into_iter()
                .filter(|k| k.name == key_or_name || hex::encode(&k.access_key) == key_or_name)
                .map(|k| k.access_key)
                .collect();
            if doomed.is_empty() {
                bail!("no access key is called or named {key_or_name:?}");
            }
            let n = doomed.len();
            store.write(move |ctx| {
                for key in &doomed {
                    auth::delete_key(ctx.conn(), key)?;
                }
                Ok(())
            })?;
            println!("removed {n} access key(s)");
            Ok(())
        }
        Action::Listen { minutes, yes } => listen(&store, minutes, yes),
    }
}

fn set_open(store: &Store, until: Option<i64>) -> Result<()> {
    store.write(move |ctx| {
        let mut registration: Registration = hydrus_store::settings::get(ctx.conn())?;
        registration.open_until_ms = until;
        hydrus_store::settings::set(ctx.conn(), &registration)
    })?;
    Ok(())
}

fn listen(store: &Store, minutes: u64, yes: bool) -> Result<()> {
    let until = hydrus_core::time::TimestampMs::now().millis() + i64::try_from(minutes * 60_000)?;
    set_open(store, Some(until))?;
    println!(
        "accepting access key requests for {minutes} minutes (a running `hydrus serve` answers them); Ctrl-C to stop"
    );
    let result = (|| -> Result<()> {
        while hydrus_core::time::TimestampMs::now().millis() < until {
            let requests = store.write(|ctx| {
                let mut registration: Registration = hydrus_store::settings::get(ctx.conn())?;
                let requests = std::mem::take(&mut registration.requests);
                hydrus_store::settings::set(ctx.conn(), &registration)?;
                Ok(requests)
            })?;
            for (key, name, permits_everything, basic) in requests {
                let key = AccessPermissions {
                    access_key: hex::decode(&key)?,
                    name,
                    permits_everything,
                    basic: basic.into_iter().collect(),
                    search_filter: TagFilter::default(),
                };
                println!(
                    "\n{:?} asks for an access key: {}",
                    key.name,
                    key.human_description()
                );
                if yes || ask("accept it? [y/N] ")? {
                    store.write(move |ctx| auth::save_key(ctx.conn(), &key))?;
                    println!("accepted");
                } else {
                    println!("refused");
                }
            }
            std::thread::sleep(Duration::from_secs(1));
        }
        Ok(())
    })();
    set_open(store, None)?;
    result
}

fn ask(prompt: &str) -> Result<bool> {
    print!("{prompt}");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(matches!(line.trim(), "y" | "Y" | "yes"))
}
