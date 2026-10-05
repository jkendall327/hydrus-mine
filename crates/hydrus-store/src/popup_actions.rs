//! Owner-qualified GUI requests for live producer-owned popup callbacks/replies.
//! Callback code stays in its producer process; only intent crosses SQLite.
use crate::{Result, popups};
use rusqlite::{Connection, OptionalExtension as _, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Request {
    Call,
    Answer { question: [u8; 32], answer: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    pub request: Request,
    pub gui_owner: Option<[u8; 32]>,
}
/// Recheck the owner just before beginning a callable's effect.
pub fn can_call(
    conn: &Connection,
    key: &[u8; 32],
    owner: &[u8; 32],
    gui_owner: Option<&[u8; 32]>,
    now: i64,
) -> Result<bool> {
    if !owns(conn, key, owner)? {
        return Ok(false);
    }
    if let Some(gui_owner) = gui_owner {
        let alive: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM popup_action_gui_owners WHERE owner = ?)",
            [gui_owner.as_slice()],
            |row| row.get(0),
        )?;
        if !alive {
            return Ok(false);
        }
    }
    Ok(popups::get(conn, key, now)?.is_some_and(|job| {
        job.action_owner.as_ref() == Some(owner) && job.user_callable_label.is_some()
    }))
}

/// A GUI incarnation owns its unconsumed intents independently of producer jobs.
pub fn begin_gui(conn: &Connection, owner: &[u8; 32]) -> Result<()> {
    conn.execute(
        "INSERT INTO popup_action_gui_owners (owner) VALUES (?)",
        [owner.as_slice()],
    )?;
    Ok(())
}
pub fn retire_gui(conn: &Connection, owner: &[u8; 32]) -> Result<()> {
    // Committed yes/no replies survive GUI retirement; only pending calls retire.
    conn.execute(
        "DELETE FROM popup_action_requests WHERE gui_owner = ? AND request = ?",
        params![owner.as_slice(), serde_json::to_string(&Request::Call)?],
    )?;
    conn.execute(
        "DELETE FROM popup_action_gui_owners WHERE owner = ?",
        [owner.as_slice()],
    )?;
    Ok(())
}

pub fn begin(conn: &Connection, key: &[u8; 32], owner: &[u8; 32]) -> Result<()> {
    conn.execute(
        "DELETE FROM popup_action_requests WHERE job_key = ?",
        [key.as_slice()],
    )?;
    conn.execute("INSERT INTO popup_action_owners (job_key, owner) VALUES (?, ?) ON CONFLICT(job_key) DO UPDATE SET owner = excluded.owner",params![key.as_slice(),owner.as_slice()])?;
    Ok(())
}
fn owns(conn: &Connection, key: &[u8; 32], owner: &[u8; 32]) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT owner = ?2 FROM popup_action_owners WHERE job_key = ?1",
            params![key.as_slice(), owner.as_slice()],
            |r| r.get::<_, bool>(0),
        )
        .optional()?
        .unwrap_or(false))
}
/// Re-read current job state: label/payload updates need no intervening GUI poll.
pub fn request(
    conn: &Connection,
    key: &[u8; 32],
    owner: &[u8; 32],
    now: i64,
    request: Request,
) -> Result<bool> {
    request_inner(conn, key, owner, None, now, request)
}
/// Validate both the live producer and the originating GUI incarnation.
pub fn request_from_gui(
    conn: &Connection,
    key: &[u8; 32],
    owner: &[u8; 32],
    gui_owner: &[u8; 32],
    now: i64,
    request: Request,
) -> Result<bool> {
    request_inner(conn, key, owner, Some(gui_owner), now, request)
}
fn request_inner(
    conn: &Connection,
    key: &[u8; 32],
    owner: &[u8; 32],
    gui_owner: Option<&[u8; 32]>,
    now: i64,
    request: Request,
) -> Result<bool> {
    if !owns(conn, key, owner)? {
        return Ok(false);
    }
    if let Some(gui_owner) = gui_owner {
        let alive: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM popup_action_gui_owners WHERE owner = ?)",
            [gui_owner.as_slice()],
            |row| row.get(0),
        )?;
        if !alive {
            return Ok(false);
        }
    }
    let Some(job) = popups::get(conn, key, now)? else {
        return Ok(false);
    };
    if job.action_owner.as_ref() != Some(owner) {
        return Ok(false);
    }
    match &request {
        Request::Call if job.user_callable_label.is_none() => return Ok(false),
        Request::Answer { question, .. }
            if job.paused
                || job
                    .popup_yes_no_question
                    .as_ref()
                    .is_none_or(|(token, _)| token != question) =>
        {
            return Ok(false);
        }
        _ => (),
    }
    conn.execute(
        "INSERT INTO popup_action_requests (job_key, owner, gui_owner, request) VALUES (?, ?, ?, ?)",
        params![
            key.as_slice(),
            owner.as_slice(),
            gui_owner.map(|owner| owner.as_slice()),
            serde_json::to_string(&request)?
        ],
    )?;
    if matches!(request, Request::Answer { .. }) {
        popups::update(conn, key, now, |job| job.finish_and_dismiss(None, now))?;
    }
    Ok(true)
}
/// Called on the Store writer: drain only this producer incarnation's requests.
pub fn take_owned(conn: &Connection, key: &[u8; 32], owner: &[u8; 32]) -> Result<Vec<Pending>> {
    if !owns(conn, key, owner)? {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare_cached(
        "SELECT request, gui_owner FROM popup_action_requests WHERE job_key = ? AND owner = ? AND (json_type(request) = 'object' OR gui_owner IS NULL OR EXISTS(SELECT 1 FROM popup_action_gui_owners WHERE popup_action_gui_owners.owner = popup_action_requests.gui_owner)) ORDER BY seq",
    )?;
    let rows = stmt.query_map(params![key.as_slice(), owner.as_slice()], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Option<Vec<u8>>>(1)?))
    })?;
    let mut requests = Vec::new();
    for row in rows {
        let (request, gui_owner) = row?;
        let gui_owner = gui_owner
            .map(|owner| {
                owner
                    .try_into()
                    .map_err(|_| crate::StoreError::Corrupt("invalid popup GUI owner".into()))
            })
            .transpose()?;
        requests.push(Pending {
            request: serde_json::from_str(&request)?,
            gui_owner,
        });
    }
    conn.execute(
        "DELETE FROM popup_action_requests WHERE job_key = ? AND owner = ?",
        params![key.as_slice(), owner.as_slice()],
    )?;
    Ok(requests)
}
/// Retire only this owner; retained handles cannot consume/delete a successor.
pub fn retire(conn: &Connection, key: &[u8; 32], owner: &[u8; 32]) -> Result<()> {
    popups::update(conn, key, i64::MIN, |job| {
        if job.action_owner.as_ref() == Some(owner) {
            job.action_owner = None;
            job.user_callable_label = None;
            job.popup_yes_no_question = None;
        }
    })?;
    conn.execute(
        "DELETE FROM popup_action_requests WHERE job_key = ? AND owner = ?",
        params![key.as_slice(), owner.as_slice()],
    )?;
    conn.execute(
        "DELETE FROM popup_action_owners WHERE job_key = ? AND owner = ?",
        params![key.as_slice(), owner.as_slice()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn take(conn: &Connection, key: &[u8; 32], owner: &[u8; 32]) -> Result<Vec<Request>> {
        Ok(take_owned(conn, key, owner)?
            .into_iter()
            .map(|pending| pending.request)
            .collect())
    }
    fn conn() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::schema::configure(&conn).unwrap();
        crate::schema::migrate(&mut conn).unwrap();
        conn
    }
    #[test]
    fn gui_retirement_drops_unconsumed_calls_but_preserves_committed_answers() {
        let conn = conn();
        let gui = [8; 32];
        let next_gui = [9; 32];
        let owner = [1; 32];
        let question = [2; 32];
        let mut job = popups::Job::new(false, true, 0.0);
        job.action_owner = Some(owner);
        job.user_callable_label = Some("command".into());
        job.popup_yes_no_question = Some((question, "yes?".into()));
        begin(&conn, &job.key, &owner).unwrap();
        begin_gui(&conn, &gui).unwrap();
        popups::add(&conn, &job, 0).unwrap();
        assert!(request_from_gui(&conn, &job.key, &owner, &gui, 0, Request::Call).unwrap());
        retire_gui(&conn, &gui).unwrap();
        assert!(take(&conn, &job.key, &owner).unwrap().is_empty());
        assert!(!request_from_gui(&conn, &job.key, &owner, &gui, 0, Request::Call).unwrap());
        begin_gui(&conn, &next_gui).unwrap();
        assert!(
            request_from_gui(
                &conn,
                &job.key,
                &owner,
                &next_gui,
                0,
                Request::Answer {
                    question,
                    answer: true
                }
            )
            .unwrap()
        );
        retire_gui(&conn, &next_gui).unwrap();
        assert!(popups::get(&conn, &job.key, 0).unwrap().is_none());
        assert_eq!(
            take(&conn, &job.key, &owner).unwrap(),
            [Request::Answer {
                question,
                answer: true
            }]
        );
    }

    #[test]
    fn answer_survives_dismissal_and_old_owner_cannot_consume_or_retire_successor() {
        let conn = conn();
        let mut job = popups::Job::new(true, true, 0.0);
        let key = job.key;
        let owner = [1; 32];
        let question = [2; 32];
        job.action_owner = Some(owner);
        job.popup_yes_no_question = Some((question, "first?".into()));
        job.user_callable_label = Some("call".into());
        begin(&conn, &key, &owner).unwrap();
        popups::add(&conn, &job, 0).unwrap();
        assert!(
            !request(
                &conn,
                &key,
                &owner,
                0,
                Request::Answer {
                    question: [3; 32],
                    answer: true
                }
            )
            .unwrap()
        );
        popups::update(&conn, &key, 0, |j| j.paused = true).unwrap();
        assert!(
            !request(
                &conn,
                &key,
                &owner,
                0,
                Request::Answer {
                    question,
                    answer: true
                }
            )
            .unwrap()
        );
        assert!(
            request(&conn, &key, &owner, 0, Request::Call).unwrap(),
            "paused still permits callable"
        );
        popups::update(&conn, &key, 0, |j| j.paused = false).unwrap();
        assert!(
            request(
                &conn,
                &key,
                &owner,
                0,
                Request::Answer {
                    question,
                    answer: false
                }
            )
            .unwrap()
        );
        assert!(popups::get(&conn, &key, 0).unwrap().is_none());
        assert_eq!(
            take(&conn, &key, &owner).unwrap(),
            [
                Request::Call,
                Request::Answer {
                    question,
                    answer: false
                }
            ]
        );
        assert!(take(&conn, &key, &owner).unwrap().is_empty());
        let next = [4; 32];
        job.action_owner = Some(next);
        job.popup_yes_no_question = Some(([5; 32], "successor?".into()));
        begin(&conn, &key, &next).unwrap();
        popups::add(&conn, &job, 0).unwrap();
        assert!(request(&conn, &key, &next, 0, Request::Call).unwrap());
        retire(&conn, &key, &owner).unwrap();
        assert!(take(&conn, &key, &owner).unwrap().is_empty());
        assert_eq!(take(&conn, &key, &next).unwrap(), [Request::Call]);
        assert!(!request(&conn, &key, &owner, 0, Request::Call).unwrap());
        assert_eq!(
            popups::get(&conn, &key, 0).unwrap().unwrap().action_owner,
            Some(next)
        );
        retire(&conn, &key, &next).unwrap();
        let job = popups::get(&conn, &key, 0).unwrap().unwrap();
        assert!(job.user_callable_label.is_none() && job.popup_yes_no_question.is_none());
    }
    #[test]
    fn pre_action_schema_upgrades_and_restart_forgets_ephemeral_handlers_but_keeps_clipboard() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::schema::configure(&conn).unwrap();
        crate::schema::migrate(&mut conn).unwrap();
        conn.execute_batch("DROP TABLE popup_action_requests; DROP TABLE popup_action_owners; DROP TABLE popup_action_gui_owners; PRAGMA user_version=17;").unwrap();
        crate::schema::migrate(&mut conn).unwrap();
        let mut job = popups::Job::text("finished", 0.0);
        let owner = [7; 32];
        job.action_owner = Some(owner);
        job.user_callable_label = Some("ephemeral".into());
        job.popup_clipboard = Some(("copy".into(), "retained".into()));
        begin(&conn, &job.key, &owner).unwrap();
        popups::add(&conn, &job, 0).unwrap();
        assert!(request(&conn, &job.key, &owner, 0, Request::Call).unwrap());
        popups::forget_unfinished(&conn, 0).unwrap();
        let retained = popups::get(&conn, &job.key, 0).unwrap().unwrap();
        assert_eq!(retained.popup_clipboard, job.popup_clipboard);
        assert!(retained.user_callable_label.is_none() && retained.action_owner.is_none());
        assert!(take(&conn, &job.key, &owner).unwrap().is_empty());
        let mut legacy = serde_json::to_value(popups::Job::text("old JSON", 0.0)).unwrap();
        for key in [
            "popup_clipboard",
            "popup_yes_no_question",
            "user_callable_label",
            "action_owner",
        ] {
            legacy.as_object_mut().unwrap().remove(key);
        }
        let decoded: popups::Job = serde_json::from_value(legacy).unwrap();
        assert!(decoded.action_owner.is_none() && decoded.popup_clipboard.is_none());
    }
}
