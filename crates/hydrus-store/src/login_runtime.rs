//! Store-owned login admission and cancellation, shared by GUI and daemon engines.
//! A file lease serializes HTTP processes and survives neither drops nor crashes;
//! its persisted descriptor makes cancellation target the exact reviewed owner.

use crate::{
    Result, Store,
    network_runtime::LoginProcess,
    settings::{self, Setting},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Default, Serialize, Deserialize)]
struct Coordination {
    sequence: u64,
    process: Option<LoginProcess>,
    cancelled: bool,
}
impl Setting for Coordination {
    const KEY: &'static str = "login_runtime";
}

/// Allocate an engine epoch atomically across independently opened store handles.
pub fn next_epoch(store: &Store) -> Result<String> {
    store.write(|ctx| {
        let mut state: Coordination = settings::get(ctx.conn())?;
        state.sequence = state.sequence.saturating_add(1);
        let epoch = format!("login-engine:{}", state.sequence);
        settings::set(ctx.conn(), &state)?;
        Ok(epoch)
    })
}

/// An exclusive process lease; its descriptor is retired before releasing the file.
#[derive(Debug)]
pub struct Lease {
    store: Arc<Store>,
    _file: std::fs::File,
    owner: LoginProcess,
}
impl Lease {
    /// Publish the current step and observe cancellation addressed to this owner.
    pub fn pulse(&self, status: String) -> Result<bool> {
        let owner = self.owner.clone();
        self.store.write(move |ctx| {
            let mut state: Coordination = settings::get(ctx.conn())?;
            if !matches_owner(state.process.as_ref(), &owner.epoch, owner.id) {
                return Ok(true);
            }
            let cancelled = state.cancelled;
            if let Some(process) = state.process.as_mut()
                && process.status != status
            {
                process.status = status;
                settings::set(ctx.conn(), &state)?;
            }
            Ok(cancelled)
        })
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        let owner = self.owner.clone();
        // A failed cleanup leaves a stale descriptor, never an admission lock.
        // Readers probe the file lock and ignore stale metadata after a crash.
        let _ = self.store.write(move |ctx| {
            let mut state: Coordination = settings::get(ctx.conn())?;
            if matches_owner(state.process.as_ref(), &owner.epoch, owner.id) {
                state.process = None;
                state.cancelled = false;
                settings::set(ctx.conn(), &state)?;
            }
            Ok(())
        });
    }
}
fn matches_owner(process: Option<&LoginProcess>, epoch: &str, id: u64) -> bool {
    process.is_some_and(|process| process.epoch == epoch && process.id == id)
}

/// Try to admit a process, replacing stale metadata only while owning the file.
pub fn try_acquire(store: &Arc<Store>, owner: LoginProcess) -> Result<Option<Lease>> {
    let Some(file) = crate::store::lock_login(store.dir())? else {
        return Ok(None);
    };
    let process = owner.clone();
    store.write(move |ctx| {
        let mut state: Coordination = settings::get(ctx.conn())?;
        state.process = Some(process);
        state.cancelled = false;
        settings::set(ctx.conn(), &state)
    })?;
    Ok(Some(Lease {
        store: store.clone(),
        _file: file,
        owner,
    }))
}

/// The live process, independent of the daemon's jobs snapshot or heartbeat.
pub fn current(store: &Store) -> Result<Option<LoginProcess>> {
    if crate::store::lock_login(store.dir())?.is_some() {
        return Ok(None);
    }
    store.read(|conn| Ok(settings::get::<Coordination>(conn)?.process))
}

/// Persist cancellation only when the exact epoch and process id are still live.
pub fn cancel(store: &Store, epoch: String, id: u64) -> Result<bool> {
    if crate::store::lock_login(store.dir())?.is_some() {
        return Ok(false);
    }
    store.write(move |ctx| {
        let mut state: Coordination = settings::get(ctx.conn())?;
        if !matches_owner(state.process.as_ref(), &epoch, id) {
            return Ok(false);
        }
        state.cancelled = true;
        settings::set(ctx.conn(), &state)?;
        Ok(true)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn process(epoch: String, id: u64) -> LoginProcess {
        let fixture = hydrus_testkit::fixture_json("login_demand.json");
        LoginProcess {
            epoch,
            id,
            domain: "login.example".into(),
            script: "demand fixture".into(),
            status: fixture["cancelled_process"]["status"]
                .as_str()
                .unwrap()
                .into(),
        }
    }
    #[test]
    fn separate_store_owners_serialize_cancel_and_retire_only_the_reviewed_login() {
        let dir = tempfile::tempdir().unwrap();
        let desktop = Store::open(dir.path()).unwrap();
        let daemon = Store::open(dir.path()).unwrap();
        let first = process(next_epoch(&desktop).unwrap(), 1);
        let second = process(next_epoch(&daemon).unwrap(), 1);
        assert_ne!(first.epoch, second.epoch);
        let lease = try_acquire(&desktop, first.clone()).unwrap().unwrap();
        assert_eq!(current(&daemon).unwrap(), Some(first.clone()));
        assert!(try_acquire(&daemon, second.clone()).unwrap().is_none());
        assert!(!cancel(&daemon, second.epoch.clone(), second.id).unwrap());
        assert!(!cancel(&daemon, first.epoch.clone(), first.id + 1).unwrap());
        assert!(cancel(&daemon, first.epoch.clone(), first.id).unwrap());
        assert!(lease.pulse(first.status.clone()).unwrap());
        drop(lease);
        assert!(current(&daemon).unwrap().is_none());
        assert!(!cancel(&daemon, first.epoch.clone(), first.id).unwrap());
        let next = try_acquire(&daemon, second.clone()).unwrap().unwrap();
        assert!(!cancel(&desktop, first.epoch, first.id).unwrap());
        assert!(!next.pulse("second step".into()).unwrap());
        assert_eq!(current(&desktop).unwrap().unwrap().status, "second step");
        drop(next);
        assert!(current(&desktop).unwrap().is_none());
        assert!(crate::store::lock_login(dir.path()).unwrap().is_some());
    }
    #[test]
    fn crashed_process_metadata_cannot_block_or_cancel_the_next_lease() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let retired = process(next_epoch(&store).unwrap(), 9);
        store
            .write({
                let retired = retired.clone();
                move |ctx| {
                    let mut state: Coordination = settings::get(ctx.conn())?;
                    state.process = Some(retired);
                    state.cancelled = true;
                    settings::set(ctx.conn(), &state)
                }
            })
            .unwrap();
        assert!(current(&store).unwrap().is_none());
        assert!(!cancel(&store, retired.epoch.clone(), retired.id).unwrap());
        let reopened = Store::open(dir.path()).unwrap();
        let owner = process(next_epoch(&reopened).unwrap(), 1);
        let lease = try_acquire(&reopened, owner.clone()).unwrap().unwrap();
        assert_eq!(current(&store).unwrap(), Some(owner.clone()));
        assert!(!cancel(&store, retired.epoch, retired.id).unwrap());
        assert!(!lease.pulse(owner.status).unwrap());
        drop(lease);
    }
}
