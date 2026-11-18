//! Job methods (`app.start`, `app.stop`, …): the call is answered only when the job ends, and
//! meanwhile `core.get_jobs` events report it. A job's first event (`added`, or `changed` when
//! the call joined a job already queued) lists the call's JSON-RPC id in `message_ids`; later
//! ones carry only the job `id` and what changed (`progress`, `state`).
//!
//! [`run`] follows one call's job and hands its progress to the caller. The `core.get_jobs`
//! stream itself (every job on the NAS) never leaves this module.
//!
//! Guide: <https://api.truenas.com/v25.10/jobs.html>; event:
//! <https://api.truenas.com/v25.10/api_events_core.get_jobs.html> (no role).

use std::time::Duration;

use serde::Serialize;
use serde_json::{Value, json};
use tokio::time::Instant;

use super::error::Error;
use super::events::{Event, Subscription, TopicOptions};
use super::session::Session;

/// The job event.
pub const EVENT: &str = "core.get_jobs";

/// Job events kept for a slow follower. Every job on the NAS reports here, image pulls many times
/// a second; a call's first event must not be lost before its job is known.
const EVENT_BUFFER: usize = 256;

/// Progress goes out at most this often; image pulls report many steps a second.
const PROGRESS_EVERY: Duration = Duration::from_millis(250);

/// Longest progress description forwarded, in characters.
const DESCRIPTION_MAX: usize = 200;

/// How far a job has got, as the browser sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JobProgress {
    /// 0 to 100, or `None` while the job does not say.
    pub percent: Option<u8>,
    /// What the job is doing, in the NAS's words.
    pub description: Option<String>,
}

impl JobProgress {
    /// The `progress` object of a job event; `None` when the event has none.
    fn parse(fields: &Value) -> Option<Self> {
        let p = fields.get("progress")?.as_object()?;
        let percent = p
            .get("percent")
            .and_then(Value::as_f64)
            .filter(|n| n.is_finite())
            // Clamped to 0..=100 first, so the cast cannot truncate.
            .map(|n| n.clamp(0.0, 100.0).round() as u8);
        let description = p
            .get("description")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|d| !d.is_empty())
            .map(|d| d.chars().take(DESCRIPTION_MAX).collect());
        Some(Self {
            percent,
            description,
        })
    }
}

/// Picks one call's job out of the `core.get_jobs` stream.
#[derive(Debug)]
struct Tracker {
    call_id: u64,
    job_id: Option<i64>,
    last: Option<JobProgress>,
}

impl Tracker {
    fn new(call_id: u64) -> Self {
        Self {
            call_id,
            job_id: None,
            last: None,
        }
    }

    /// The new progress when an update (its `id` and `fields`) is about this call's job and its
    /// progress changed.
    fn update(&mut self, id: Option<&Value>, fields: &Value) -> Option<JobProgress> {
        let event_job = fields.get("id").or(id).and_then(Value::as_i64);
        match self.job_id {
            Some(job) if event_job != Some(job) => return None,
            Some(_) => {}
            None => {
                let ours = fields
                    .get("message_ids")
                    .and_then(Value::as_array)
                    .is_some_and(|ids| ids.iter().any(|i| i.as_u64() == Some(self.call_id)));
                if !ours {
                    return None;
                }
                self.job_id = event_job;
            }
        }
        let progress = JobProgress::parse(fields)?;
        if self.last.as_ref() == Some(&progress) {
            return None;
        }
        self.last = Some(progress.clone());
        Some(progress)
    }

    /// Looks for this call's job in a `core.get_jobs` query result, after its first event may
    /// have been lost. The new progress if it was found and has some.
    fn find_in(&mut self, jobs: &Value) -> Option<JobProgress> {
        jobs.as_array()?
            .iter()
            .find_map(|job| self.update(None, job))
    }
}

/// The jobs still going on, with only what [`Tracker`] reads. `core.get_jobs` needs no role and
/// lists the user's own jobs.
async fn running_jobs(session: &Session) -> Result<Value, Error> {
    session
        .call(
            EVENT,
            json!([
                [["state", "in", ["WAITING", "RUNNING"]]],
                {"select": ["id", "message_ids", "progress"]}
            ]),
        )
        .await
}

/// The next job event, or never when there is no subscription.
async fn next_event(sub: &mut Option<Subscription>) -> Option<Event> {
    match sub {
        Some(s) => s.recv().await,
        None => std::future::pending().await,
    }
}

/// Calls the job method `method` and waits for it to end, without a timeout (a job can take
/// minutes), calling `on_progress` as the job reports progress (at most every 250 ms, only on
/// change). Progress is best effort: if `core.get_jobs` is refused or ends, the call still runs.
///
/// Dropping the future does not stop the job on the NAS.
///
/// # Errors
///
/// The job's failure as [`Error::Rpc`], and the errors of [`Session::start_call`] (the link
/// dropped while the job ran: [`Error::Closed`]; it may still finish on the NAS).
pub async fn run(
    session: &Session,
    method: &str,
    params: Value,
    mut on_progress: impl FnMut(JobProgress),
) -> Result<Value, Error> {
    // Subscribed before the call, so its first event cannot be missed.
    let opts = TopicOptions {
        buffer: Some(EVENT_BUFFER),
        replay: 0,
    };
    let mut sub = match session.subscribe_with(EVENT, opts).await {
        Ok(sub) => Some(sub),
        Err(Error::LoggedOut) => return Err(Error::LoggedOut),
        Err(e) => {
            tracing::debug!(error = %e, "job progress unavailable");
            None
        }
    };
    let (call_id, reply) = session.start_call(method, params, None).await?;
    let mut reply = std::pin::pin!(reply);
    let mut tracker = Tracker::new(call_id);
    let mut pending: Option<JobProgress> = None;
    let mut next_send = Instant::now();
    loop {
        tokio::select! {
            res = &mut reply => return res,
            event = next_event(&mut sub) => match event {
                Some(Event::Update(u)) => {
                    if let Some(p) = tracker.update(u.id.as_ref(), &u.fields) {
                        pending = Some(p);
                    }
                }
                // Events of a gap are lost: once the job is known, the next event about it
                // brings it up to date. Before that, only its first event names this call, so
                // look for the job instead.
                Some(Event::Resumed | Event::Missed(_)) => {
                    if tracker.job_id.is_none() {
                        match running_jobs(session).await {
                            Ok(jobs) => {
                                if let Some(p) = tracker.find_in(&jobs) {
                                    pending = Some(p);
                                }
                            }
                            Err(e) => tracing::debug!(error = %e, "cannot look for the job"),
                        }
                    }
                }
                Some(Event::Ended(_)) | None => sub = None,
            },
            () = tokio::time::sleep_until(next_send), if pending.is_some() => {
                if let Some(p) = pending.take() {
                    on_progress(p);
                }
                next_send = Instant::now() + PROGRESS_EVERY;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// An event about `fields`, with its id where TrueNAS puts it.
    fn track(t: &mut Tracker, fields: Value) -> Option<JobProgress> {
        t.update(fields.get("id"), &fields)
    }

    fn progress(percent: Option<u8>, description: &str) -> Option<JobProgress> {
        Some(JobProgress {
            percent,
            description: Some(description.to_owned()),
        })
    }

    #[test]
    fn follows_the_job_that_lists_the_call_then_its_id() {
        let fixture: Value = serde_json::from_str(include_str!("testdata/job_event.json")).unwrap();
        let mut t = Tracker::new(7);
        // Another call's job.
        let other = json!({"id": 52, "message_ids": [6], "progress": {"percent": 10}});
        assert_eq!(track(&mut t, other), None);
        assert_eq!(t.job_id, None);
        // Ours, as TrueNAS sends it: the whole job.
        assert_eq!(track(&mut t, fixture), progress(Some(20), "Stopping app"));
        assert_eq!(t.job_id, Some(53));
        // Later events carry only the id.
        let later = json!({"id": 53, "progress": {"percent": 60.4, "description": "Stopping"}});
        assert_eq!(track(&mut t, later), progress(Some(60), "Stopping"));
        let elsewhere = json!({"id": 54, "message_ids": [7], "progress": {"percent": 90}});
        assert_eq!(track(&mut t, elsewhere), None);
    }

    #[test]
    fn a_call_that_joins_a_queued_job_is_found_on_changed() {
        let mut t = Tracker::new(7);
        let joined = json!({"id": 40, "message_ids": [3, 7], "progress": {"percent": 0,
            "description": "Waiting"}});
        assert_eq!(track(&mut t, joined), progress(Some(0), "Waiting"));
        assert_eq!(t.job_id, Some(40));
    }

    #[test]
    fn a_job_whose_first_event_was_lost_is_found_by_query() {
        let mut t = Tracker::new(7);
        let jobs = json!([
            {"id": 50, "message_ids": [3], "progress": {"percent": 80}},
            {"id": 53, "message_ids": [7], "progress": {"percent": 40, "description": "Pulling"}}
        ]);
        assert_eq!(t.find_in(&jobs), progress(Some(40), "Pulling"));
        assert_eq!(t.job_id, Some(53));
        // Then followed by id, as usual.
        let later = json!({"id": 53, "progress": {"percent": 50, "description": "Pulling"}});
        assert_eq!(track(&mut t, later), progress(Some(50), "Pulling"));

        let mut none = Tracker::new(9);
        assert_eq!(none.find_in(&jobs), None);
        assert_eq!(none.find_in(&json!({"error": 1})), None);
        assert_eq!(none.job_id, None);
    }

    #[test]
    fn only_changes_are_reported() {
        let mut t = Tracker::new(1);
        let same = json!({"id": 5, "message_ids": [1], "progress": {"percent": 5,
            "description": "Pulling"}});
        assert!(track(&mut t, same.clone()).is_some());
        assert_eq!(track(&mut t, same), None);
        // An event without progress (a state change) says nothing new.
        assert_eq!(track(&mut t, json!({"id": 5, "state": "RUNNING"})), None);
    }

    #[test]
    fn odd_progress_is_cleaned() {
        let p = |progress: Value| JobProgress::parse(&json!({ "progress": progress }));
        let long = "x".repeat(500);
        assert_eq!(
            p(json!({"percent": 150, "description": "  "})),
            Some(JobProgress {
                percent: Some(100),
                description: None
            })
        );
        assert_eq!(p(json!({"percent": -3})).unwrap().percent, Some(0));
        assert_eq!(p(json!({"percent": null})).unwrap().percent, None);
        assert_eq!(p(json!({"percent": "50"})).unwrap().percent, None);
        assert_eq!(
            p(json!({"description": long}))
                .unwrap()
                .description
                .unwrap()
                .len(),
            DESCRIPTION_MAX
        );
        assert_eq!(JobProgress::parse(&json!({"progress": null})), None);
    }
}
