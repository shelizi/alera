use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;

use super::{ServerActor, ServerCommand};
use crate::terminal_host::demand_driven_ticker::DemandDrivenTicker;
use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::resources::{
    clamp_resource_interval, resource_idle_stop_for, warming_snapshot, ResourceSampler,
    SessionPidRoot, RESOURCE_IDLE_STOP, RESOURCE_SAMPLE_INTERVAL,
};

/// Resource sampling state, driven lazily: nothing sweeps the process table
/// until a client asks, and the ticker stops itself once they stop asking.
pub(super) struct ResourceMonitorState {
    sampler: Arc<Mutex<ResourceSampler>>,
    ticker: DemandDrivenTicker,
    last_snapshot: Option<Value>,
    sample_in_flight: bool,
    release_sampler_when_ready: bool,
    /// Pid of the app process to attribute, as reported by the last caller.
    app_pid: Option<u32>,
    /// Exact App generation supplied alongside its PID, never inferred.
    app_creation: Option<u64>,
    sample_app_identity: Option<(Option<u32>, Option<u64>)>,
    /// Cadence the ticker is currently running at, following what callers say
    /// they poll at rather than a fixed value.
    interval: Duration,
}

impl Default for ResourceMonitorState {
    fn default() -> Self {
        ResourceMonitorState {
            sampler: Arc::default(),
            ticker: DemandDrivenTicker::new(RESOURCE_IDLE_STOP),
            last_snapshot: None,
            sample_in_flight: false,
            release_sampler_when_ready: false,
            app_pid: None,
            app_creation: None,
            sample_app_identity: None,
            interval: RESOURCE_SAMPLE_INTERVAL,
        }
    }
}

impl ResourceMonitorState {
    fn stop_ticker(&mut self) {
        self.ticker.stop();
        self.last_snapshot = None;
        self.app_pid = None;
        self.app_creation = None;
        self.interval = RESOURCE_SAMPLE_INTERVAL;
        if self.sample_in_flight {
            self.release_sampler_when_ready = true;
        } else if let Ok(mut sampler) = self.sampler.lock() {
            sampler.release_process_cache();
        }
    }

    fn finish_sample(&mut self, snapshot: Value) {
        self.sample_in_flight = false;
        let sample_identity = self.sample_app_identity.take();
        if self.release_sampler_when_ready {
            self.release_sampler_when_ready = false;
            if let Ok(mut sampler) = self.sampler.lock() {
                sampler.release_process_cache();
            }
            if self.ticker.is_idle() {
                return;
            }
        }
        if sample_identity.is_some_and(|identity| identity != (self.app_pid, self.app_creation)) {
            return;
        }
        self.last_snapshot = Some(snapshot);
    }

    fn note_app_identity(&mut self, pid: Option<u32>, creation: Option<u64>) {
        if pid.is_some() && (pid, creation) != (self.app_pid, self.app_creation) {
            self.app_pid = pid;
            self.app_creation = creation;
            self.last_snapshot = None;
        }
    }
}

impl ServerActor {
    /// Answer `resources.snapshot` with the most recent sweep and keep the
    /// ticker alive for the caller.
    ///
    /// Never blocks on a sweep: a client polling every two seconds would
    /// otherwise serialize behind a process-table walk.
    pub(super) fn handle_resource_snapshot(&mut self, payload: &Value) -> HostResult<Value> {
        let app_pid = parse_app_pid(payload)?;
        let app_creation = parse_app_creation(payload, app_pid)?;
        let interval = parse_sample_interval(payload)?.unwrap_or(RESOURCE_SAMPLE_INTERVAL);
        self.resources.note_app_identity(app_pid, app_creation);
        self.resources
            .ticker
            .set_idle_stop(resource_idle_stop_for(interval));
        self.resources.ticker.note_request();
        let running = self.resources.ticker.is_running();
        if !running || self.resources.interval != interval {
            self.resources.interval = interval;
            self.start_resource_ticker(!running);
        }
        Ok(self
            .resources
            .last_snapshot
            .clone()
            .unwrap_or_else(warming_snapshot))
    }

    /// (Re)start the ticker at the current cadence.
    ///
    /// `reset_baseline` is what distinguishes the two reasons to do this. The
    /// CPU numbers are deltas between refreshes, so after an idle gap the next
    /// delta would describe minutes of history as if it were the last tick and
    /// the baseline has to restart with the ticker. A cadence change on a
    /// ticker that never stopped has no such gap, and resetting there would put
    /// the panel back into "measuring" every time the user hovers it.
    fn start_resource_ticker(&mut self, reset_baseline: bool) {
        if reset_baseline {
            if let Ok(mut sampler) = self.resources.sampler.lock() {
                sampler.reset_cpu_baseline();
            }
        }
        let inbox = self.inbox.clone();
        self.resources
            .ticker
            .start(self.resources.interval, move || {
                inbox.send(ServerCommand::ResourceSampleTick).is_ok()
            });
    }

    /// One tick: stop if nobody is watching, otherwise start a sweep on a
    /// blocking thread.
    pub(super) fn handle_resource_sample_tick(&mut self) {
        if self.resources.ticker.is_idle() {
            self.resources.stop_ticker();
            return;
        }
        if self.resources.sample_in_flight {
            // A previous sweep is still running on a loaded machine. Skipping
            // keeps blocking threads from piling up.
            return;
        }
        self.resources.sample_in_flight = true;
        let roots = self.resource_session_roots();
        let sampler = Arc::clone(&self.resources.sampler);
        let host_pid = std::process::id();
        let app_pid = self.resources.app_pid;
        let app_creation = self.resources.app_creation;
        self.resources.sample_app_identity = Some((app_pid, app_creation));
        let inbox = self.inbox.clone();
        tokio::task::spawn_blocking(move || {
            let snapshot = match sampler.lock() {
                Ok(mut sampler) => {
                    sampler.sample_with_app_identity(&roots, host_pid, app_pid, app_creation)
                }
                Err(_) => warming_snapshot(),
            };
            let _ = inbox.send(ServerCommand::ResourceSampleReady { snapshot });
        });
    }

    pub(super) fn handle_resource_sample_ready(&mut self, snapshot: Value) {
        self.resources.finish_sample(snapshot);
    }

    fn resource_session_roots(&self) -> Vec<SessionPidRoot> {
        self.sessions
            .values()
            .map(|session| SessionPidRoot {
                session_id: session.id.clone(),
                workspace_id: session.workspace_id.clone(),
                tab_id: session.tab_id.clone(),
                running: session.running(),
                shell: session.shell(),
            })
            .collect()
    }
}

/// The app measures itself through the host because only the host runs the
/// sampler. The pid travels per request rather than in `configure`: it belongs
/// to one client, not to the host's shared configuration.
/// The cadence the caller says it polls at, clamped to what the sampler serves.
///
/// Additive: a client that sends nothing gets the host's own default, which is
/// what an app predating this expects.
fn parse_sample_interval(payload: &Value) -> HostResult<Option<Duration>> {
    match payload.get("intervalMs") {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .filter(|milliseconds| *milliseconds > 0)
            .map(|milliseconds| Some(clamp_resource_interval(Duration::from_millis(milliseconds))))
            .ok_or_else(|| HostError::state("intervalMs must be a positive integer.")),
    }
}

fn parse_app_pid(payload: &Value) -> HostResult<Option<u32>> {
    match payload.get("appPid") {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|pid| u32::try_from(pid).ok())
            .filter(|pid| *pid > 0)
            .map(Some)
            .ok_or_else(|| HostError::state("appPid must be a positive integer.")),
    }
}

/// Decimal string avoids loss of FILETIME's subsecond bits through JSON
/// implementations that represent numbers as IEEE-754 doubles.
fn parse_app_creation(payload: &Value, pid: Option<u32>) -> HostResult<Option<u64>> {
    match payload.get("appCreationFiletime") {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 20 && s.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|ticks| pid.is_some() && *ticks >= 116_444_736_010_000_000)
            .map(Some)
            .ok_or_else(|| {
                HostError::state(
                    "appCreationFiletime must be a valid decimal FILETIME with appPid.",
                )
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_absent_app_pid_is_accepted() {
        assert_eq!(parse_app_pid(&json!({})).unwrap(), None);
        assert_eq!(parse_app_pid(&json!({ "appPid": null })).unwrap(), None);
    }

    #[test]
    fn a_valid_app_pid_is_parsed() {
        assert_eq!(
            parse_app_pid(&json!({ "appPid": 4242 })).unwrap(),
            Some(4242)
        );
    }

    #[test]
    fn a_nonsense_app_pid_is_rejected() {
        assert!(parse_app_pid(&json!({ "appPid": 0 })).is_err());
        assert!(parse_app_pid(&json!({ "appPid": -1 })).is_err());
        assert!(parse_app_pid(&json!({ "appPid": "one" })).is_err());
    }

    #[test]
    fn app_creation_is_optional_lossless_and_bound_to_a_pid() {
        let ticks = 133_500_000_000_000_123u64;
        let payload = json!({"appPid": 42, "appCreationFiletime": ticks.to_string()});
        assert_eq!(parse_app_creation(&payload, Some(42)).unwrap(), Some(ticks));
        assert!(parse_app_creation(&payload, None).is_err());
        for invalid in [
            json!(ticks),
            json!("0"),
            json!("-1"),
            json!("18446744073709551616"),
            json!("abc"),
        ] {
            assert!(
                parse_app_creation(&json!({"appCreationFiletime": invalid}), Some(42)).is_err()
            );
        }
        assert_eq!(parse_app_creation(&json!({}), Some(42)).unwrap(), None);
    }

    #[test]
    fn app_generation_changes_discard_stale_and_in_flight_snapshots() {
        let mut resources = ResourceMonitorState::default();
        resources.note_app_identity(Some(42), Some(133_500_000_000_000_123));
        resources.last_snapshot = Some(json!({"old": true}));
        resources.sample_app_identity = Some((resources.app_pid, resources.app_creation));
        resources.sample_in_flight = true;
        // A bare-PID client cannot accidentally reuse a prior exact seal.
        resources.note_app_identity(Some(42), None);
        assert!(resources.last_snapshot.is_none());
        assert!(resources.app_creation.is_none());
        resources.finish_sample(json!({"old": true}));
        assert!(resources.last_snapshot.is_none());
        resources.note_app_identity(None, None);
        assert_eq!(resources.app_pid, Some(42));
        resources.sample_app_identity = Some((resources.app_pid, resources.app_creation));
        resources.finish_sample(json!({"new": true}));
        assert_eq!(resources.last_snapshot, Some(json!({"new": true})));
        resources.stop_ticker();
        assert!(resources.app_pid.is_none() && resources.app_creation.is_none());
    }

    #[test]
    fn an_absent_interval_leaves_the_host_default_in_place() {
        assert_eq!(parse_sample_interval(&json!({})).unwrap(), None);
        assert_eq!(
            parse_sample_interval(&json!({ "intervalMs": null })).unwrap(),
            None
        );
    }

    #[test]
    fn a_stated_interval_is_parsed_and_clamped() {
        assert_eq!(
            parse_sample_interval(&json!({ "intervalMs": 15_000 })).unwrap(),
            Some(Duration::from_secs(15))
        );
        assert_eq!(
            parse_sample_interval(&json!({ "intervalMs": 50 })).unwrap(),
            Some(RESOURCE_SAMPLE_INTERVAL)
        );
    }

    #[test]
    fn a_nonsense_interval_is_rejected() {
        assert!(parse_sample_interval(&json!({ "intervalMs": 0 })).is_err());
        assert!(parse_sample_interval(&json!({ "intervalMs": -1 })).is_err());
        assert!(parse_sample_interval(&json!({ "intervalMs": "fast" })).is_err());
    }

    #[test]
    fn stopping_an_idle_monitor_releases_cached_resources() {
        let mut resources = ResourceMonitorState::default();
        resources
            .sampler
            .lock()
            .unwrap()
            .sample(&[], std::process::id(), None);
        resources.last_snapshot = Some(json!({"sample": true}));
        resources.app_pid = Some(42);

        resources.stop_ticker();

        assert!(resources.last_snapshot.is_none());
        assert!(resources.app_pid.is_none());
        assert_eq!(
            resources
                .sampler
                .lock()
                .unwrap()
                .sample(&[], std::process::id(), None)["warming"],
            json!(true)
        );
    }

    #[test]
    fn an_in_flight_idle_sample_is_discarded_before_releasing_its_cache() {
        let mut resources = ResourceMonitorState::default();
        resources
            .sampler
            .lock()
            .unwrap()
            .sample(&[], std::process::id(), None);
        resources.sample_in_flight = true;

        resources.stop_ticker();
        assert!(resources.release_sampler_when_ready);

        resources.finish_sample(json!({"stale": true}));

        assert!(!resources.sample_in_flight);
        assert!(!resources.release_sampler_when_ready);
        assert!(resources.last_snapshot.is_none());
        assert_eq!(
            resources
                .sampler
                .lock()
                .unwrap()
                .sample(&[], std::process::id(), None)["warming"],
            json!(true)
        );
    }
}
