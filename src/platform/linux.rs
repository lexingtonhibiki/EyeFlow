//! X11 sensors. All protocol I/O runs on one background thread, never the UI.
//! Native Wayland is deliberately unsupported: XWayland is not a desktop sensor.

use super::sensors::{Reading, SensorSnapshot};
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::{Duration, Instant};
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::xproto::ConnectionExt;
use x11rb::protocol::{screensaver, xproto};
use x11rb::rust_connection::RustConnection;
use zbus::blocking::{connection::Builder as BusBuilder, Connection as BusConnection};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

const NO_SESSION: &str = "No native X11 desktop display is available";
const WAYLAND: &str = "Native Wayland idle and fullscreen sensors are not implemented";
const SESSION_UNKNOWN: &str =
    "X11 screen saver state does not establish that the session is unlocked";
const MAX_ATOMS: u32 = 4096;

pub struct NativeSensors {
    request: Option<SyncSender<()>>,
    result: Receiver<SensorSnapshot>,
    last: Option<SensorSnapshot>,
    unavailable: &'static str,
}

impl Default for NativeSensors {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeSensors {
    pub fn new() -> Self {
        let (request_tx, request_rx) = mpsc::sync_channel(1);
        let (result_tx, result_rx) = mpsc::sync_channel(1);
        let reason = desktop_reason(
            std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
            std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
            std::env::var("DISPLAY").ok().as_deref(),
        );
        let mut request = None;
        let mut unavailable = reason.unwrap_or("Waiting for the first X11 sensor reading");
        if reason.is_none() {
            // No join on Drop: even a frozen/remote X server must not hold up Quit.
            // At most one worker and one queued request exist for this instance.
            match std::thread::Builder::new()
                .name("eyeflow-x11-sensors".into())
                .spawn(move || {
                    let mut backend = None;
                    let mut session = None;
                    while request_rx.recv().is_ok() {
                        let started = Instant::now();
                        if backend.is_none() {
                            backend = X11Sensors::connect().ok();
                        }
                        let snapshot = match backend.as_ref() {
                            Some(native) => {
                                let mut snapshot = native.sample(started);
                                if matches!(snapshot.interruptible, Reading::Unavailable(SESSION_UNKNOWN)) && matches!(snapshot.idle, Reading::Known(_)) {
                                    if session.is_none() { session = SessionSensors::connect().ok(); }
                                    snapshot.interruptible = match session.as_ref() {
                                        Some(session) => session.sample(),
                                        None => Reading::Unavailable("Cannot connect to system and session D-Bus for lock detection"),
                                    };
                                    if matches!(snapshot.interruptible, Reading::Failed(_)) { session = None; }
                                }
                                snapshot
                            },
                            None => SensorSnapshot {
                                sampled_at: started,
                                idle: Reading::Failed("Cannot connect to the X11 display"),
                                fullscreen: Reading::Failed("Cannot connect to the X11 display"),
                                interruptible: Reading::Unavailable(SESSION_UNKNOWN),
                            },
                        };
                        if matches!(snapshot.idle, Reading::Failed(_))
                            || matches!(snapshot.fullscreen, Reading::Failed(_))
                        {
                            backend = None; // Retry a broken connection on the next request.
                        }
                        if result_tx.send(snapshot).is_err() {
                            break;
                        }
                    }
                }) {
                Ok(_) => request = Some(request_tx),
                Err(_) => unavailable = "Cannot start the X11 sensor worker",
            }
        }
        Self {
            request,
            result: result_rx,
            last: None,
            unavailable,
        }
    }

    /// Nonblocking. A stalled X server leaves the original timestamp unchanged;
    /// the consumer MUST apply SensorSnapshot::is_fresh before using readings.
    pub fn sample(&mut self, now: Instant) -> SensorSnapshot {
        while let Ok(snapshot) = self.result.try_recv() {
            self.last = Some(snapshot);
        }
        if let Some(request) = &self.request {
            let _ = request.try_send(());
        }
        self.last
            .unwrap_or_else(|| SensorSnapshot::unavailable(now, self.unavailable))
    }
}

fn desktop_reason(
    session: Option<&str>,
    wayland: Option<&str>,
    display: Option<&str>,
) -> Option<&'static str> {
    if session.is_some_and(|s| s.eq_ignore_ascii_case("wayland"))
        || wayland.is_some_and(|s| !s.is_empty())
    {
        Some(WAYLAND)
    } else if display.is_none_or(str::is_empty) {
        Some(NO_SESSION)
    } else {
        None
    }
}

struct X11Sensors {
    connection: RustConnection,
    root: u32,
}

impl X11Sensors {
    fn connect() -> Result<Self, ()> {
        let (connection, screen) = x11rb::connect(None).map_err(|_| ())?;
        let root = connection.setup().roots.get(screen).ok_or(())?.root;
        Ok(Self { connection, root })
    }

    fn sample(&self, sampled_at: Instant) -> SensorSnapshot {
        let mut interruptible = Reading::Unavailable(SESSION_UNKNOWN);
        let idle = match self
            .connection
            .extension_information(screensaver::X11_EXTENSION_NAME)
        {
            Ok(None) => Reading::Unavailable("X11 ScreenSaver extension is unavailable"),
            Err(_) => Reading::Failed("Cannot query X11 ScreenSaver extension"),
            Ok(Some(_)) => match screensaver::query_info(&self.connection, self.root)
                .map_err(|_| ())
                .and_then(|cookie| cookie.reply().map_err(|_| ()))
            {
                Ok(reply) => {
                    if reply.state == u8::from(screensaver::State::ON)
                        || reply.state == u8::from(screensaver::State::CYCLE)
                    {
                        interruptible = Reading::Known(false);
                    }
                    if reply.state != u8::from(screensaver::State::OFF)
                        && !matches!(interruptible, Reading::Known(false))
                    {
                        interruptible =
                            Reading::Unavailable("X11 screen saver state is disabled or unknown");
                    }
                    Reading::Known(Duration::from_millis(u64::from(reply.ms_since_user_input)))
                }
                Err(_) => Reading::Failed("X11 ScreenSaver query failed"),
            },
        };
        SensorSnapshot {
            sampled_at,
            idle,
            fullscreen: self.fullscreen(),
            interruptible,
        }
    }

    fn atom(&self, name: &[u8]) -> Result<u32, ()> {
        self.connection
            .intern_atom(true, name)
            .map_err(|_| ())?
            .reply()
            .map(|r| r.atom)
            .map_err(|_| ())
    }

    fn property(
        &self,
        window: u32,
        atom: u32,
        kind: xproto::AtomEnum,
    ) -> Result<xproto::GetPropertyReply, ()> {
        self.connection
            .get_property(false, window, atom, kind, 0, MAX_ATOMS)
            .map_err(|_| ())?
            .reply()
            .map_err(|_| ())
    }

    fn fullscreen(&self) -> Reading<bool> {
        match self.fullscreen_checked() {
            Ok(reading) => reading,
            Err(_) => Reading::Failed(
                "X11 fullscreen query failed; the active window may have disappeared",
            ),
        }
    }

    fn fullscreen_checked(&self) -> Result<Reading<bool>, ()> {
        let supporting_wm = self.atom(b"_NET_SUPPORTING_WM_CHECK")?;
        let supported = self.atom(b"_NET_SUPPORTED")?;
        let active = self.atom(b"_NET_ACTIVE_WINDOW")?;
        let state = self.atom(b"_NET_WM_STATE")?;
        let fullscreen = self.atom(b"_NET_WM_STATE_FULLSCREEN")?;
        if [supporting_wm, supported, active, state, fullscreen].contains(&0) {
            return Ok(Reading::Unavailable(
                "Window manager does not advertise EWMH fullscreen detection",
            ));
        }
        // EWMH requires a live supporting window whose check property points to
        // itself. Root properties alone may be leftovers from a dead WM.
        let wm = self.property(self.root, supporting_wm, xproto::AtomEnum::WINDOW)?;
        let Some(wm) = single_window(&wm) else {
            return Ok(Reading::Unavailable(
                "No live EWMH window manager is advertised",
            ));
        };
        let check = self.property(wm, supporting_wm, xproto::AtomEnum::WINDOW)?;
        if single_window(&check) != Some(wm) {
            return Ok(Reading::Unavailable(
                "EWMH window manager identity check failed",
            ));
        }
        let capabilities = self.property(self.root, supported, xproto::AtomEnum::ATOM)?;
        let Some(capabilities) = property32(&capabilities, u32::from(xproto::AtomEnum::ATOM))
        else {
            return Ok(Reading::Unavailable(
                "Window manager EWMH capabilities are absent or malformed",
            ));
        };
        if ![active, state, fullscreen]
            .iter()
            .all(|atom| capabilities.contains(atom))
        {
            return Ok(Reading::Unavailable(
                "Window manager does not support all EWMH fullscreen properties",
            ));
        }
        let active_property = self.property(self.root, active, xproto::AtomEnum::WINDOW)?;
        let Some(windows) = property32(&active_property, u32::from(xproto::AtomEnum::WINDOW))
        else {
            return Ok(Reading::Unavailable(
                "EWMH active window property is absent or malformed",
            ));
        };
        if windows.len() != 1 {
            return Ok(Reading::Unavailable(
                "EWMH active window property has invalid length",
            ));
        }
        if windows[0] == 0 {
            return Ok(Reading::Known(false));
        }
        let states = self.property(windows[0], state, xproto::AtomEnum::ATOM)?;
        Ok(
            match property32(&states, u32::from(xproto::AtomEnum::ATOM)) {
                Some(states) => Reading::Known(states.contains(&fullscreen)),
                None => Reading::Unavailable("Active window EWMH state is absent or malformed"),
            },
        )
    }
}

// Official contracts: systemd org.freedesktop.login1; GNOME shellDBus.js
// ScreenSaverDBus.GetActive; KDE kscreenlocker interface.cpp GetActive.
// LockedHint alone is not proof of unlock: require a supported live desktop
// service too. This is a point-in-time sensor, not an atomic lock guarantee.
struct SessionSensors {
    system: BusConnection,
    session: BusConnection,
}

impl SessionSensors {
    fn connect() -> zbus::Result<Self> {
        let timeout = Duration::from_millis(500);
        Ok(Self {
            system: BusBuilder::system()?.method_timeout(timeout).build()?,
            session: BusBuilder::session()?.method_timeout(timeout).build()?,
        })
    }

    fn sample(&self) -> Reading<bool> {
        self.sample_checked()
            .unwrap_or(Reading::Failed("D-Bus session or lock-state query failed"))
    }

    fn sample_checked(&self) -> zbus::Result<Reading<bool>> {
        let session_path: OwnedObjectPath = self
            .system
            .call_method(
                Some("org.freedesktop.login1"),
                "/org/freedesktop/login1",
                Some("org.freedesktop.login1.Manager"),
                "GetSessionByPID",
                &(std::process::id(),),
            )?
            .body()
            .deserialize()?;
        let mut properties: HashMap<String, OwnedValue> = self
            .system
            .call_method(
                Some("org.freedesktop.login1"),
                session_path.as_str(),
                Some("org.freedesktop.DBus.Properties"),
                "GetAll",
                &("org.freedesktop.login1.Session",),
            )?
            .body()
            .deserialize()?;
        let session_type = take_string(&mut properties, "Type");
        let class = take_string(&mut properties, "Class");
        let display = take_string(&mut properties, "Display");
        // Bind observations to THIS process's graphical session, not seat0 or
        // an arbitrary session belonging to the same user.
        if session_type.as_deref() != Some("x11")
            || class.as_deref() != Some("user")
            || !same_display(display.as_deref(), std::env::var("DISPLAY").ok().as_deref())
        {
            return Ok(Reading::Unavailable(
                "Cannot match this X11 display to the process login session",
            ));
        }
        let active = take_bool(&mut properties, "Active");
        let locked = take_bool(&mut properties, "LockedHint");
        if let Some(reading) = session_gate(active, locked) {
            return Ok(reading);
        }
        // Direct Get bypasses proxy property caches, so unchanged timestamps do
        // not conceal stale PreparingForSleep state after a service restart.
        let sleeping: OwnedValue = self
            .system
            .call_method(
                Some("org.freedesktop.login1"),
                "/org/freedesktop/login1",
                Some("org.freedesktop.DBus.Properties"),
                "Get",
                &("org.freedesktop.login1.Manager", "PreparingForSleep"),
            )?
            .body()
            .deserialize()?;
        let Ok(sleeping) = bool::try_from(sleeping) else {
            return Ok(Reading::Unavailable(
                "Login manager sleep state is malformed",
            ));
        };
        if sleeping {
            return Ok(Reading::Known(false));
        }
        let desktop = take_string(&mut properties, "Desktop").unwrap_or_default();
        let current = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        let Some((service, path, interface)) = desktop_service(&desktop, &current) else {
            return Ok(Reading::Unavailable(
                "Automatic lock detection currently supports GNOME and KDE X11 sessions only",
            ));
        };
        // Resolve a current owner first. Address the unique owner to avoid
        // activating a screen-locking service or calling its replacement.
        let owner: String = self
            .session
            .call_method(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                Some("org.freedesktop.DBus"),
                "GetNameOwner",
                &(service,),
            )?
            .body()
            .deserialize()?;
        // Session buses may be shared across multiple logins of one user. Do
        // not borrow the other login's GNOME/KDE lock status.
        let owner_pid: u32 = self
            .session
            .call_method(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                Some("org.freedesktop.DBus"),
                "GetConnectionUnixProcessID",
                &(owner.as_str(),),
            )?
            .body()
            .deserialize()?;
        let owner_session: OwnedObjectPath = self
            .system
            .call_method(
                Some("org.freedesktop.login1"),
                "/org/freedesktop/login1",
                Some("org.freedesktop.login1.Manager"),
                "GetSessionByPID",
                &(owner_pid,),
            )?
            .body()
            .deserialize()?;
        if owner_session != session_path {
            return Ok(Reading::Unavailable(
                "Screen saver service belongs to a different login session",
            ));
        }
        let saver_active: bool = self
            .session
            .call_method(
                Some(owner.as_str()),
                path,
                Some(interface),
                "GetActive",
                &(),
            )?
            .body()
            .deserialize()?;
        Ok(Reading::Known(!saver_active))
    }
}

fn session_gate(active: Option<bool>, locked: Option<bool>) -> Option<Reading<bool>> {
    if active == Some(false) || locked == Some(true) {
        Some(Reading::Known(false))
    } else if active.is_none() || locked.is_none() {
        Some(Reading::Unavailable(
            "Login session activity or locked hint is missing",
        ))
    } else {
        None
    } // Still need the live desktop's lock and sleep observations.
}

fn take_bool(properties: &mut HashMap<String, OwnedValue>, key: &str) -> Option<bool> {
    bool::try_from(properties.remove(key)?).ok()
}

fn take_string(properties: &mut HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    String::try_from(properties.remove(key)?).ok()
}

fn same_display(session: Option<&str>, actual: Option<&str>) -> bool {
    fn local_display(value: &str) -> Option<&str> {
        // Only local X displays. :0 and :0.0 refer to the same X server.
        let value = value.strip_prefix(':')?;
        let number = value.split('.').next()?;
        (!number.is_empty() && number.bytes().all(|c| c.is_ascii_digit())).then_some(number)
    }
    match (
        session.and_then(local_display),
        actual.and_then(local_display),
    ) {
        (Some(session), Some(actual)) => session == actual,
        _ => false,
    }
}

fn desktop_service(
    desktop: &str,
    current: &str,
) -> Option<(&'static str, &'static str, &'static str)> {
    let is = |name: &str| {
        desktop
            .split(':')
            .chain(current.split(':'))
            .any(|part| part.eq_ignore_ascii_case(name))
    };
    if is("GNOME") {
        Some((
            "org.gnome.Shell",
            "/org/gnome/ScreenSaver",
            "org.gnome.ScreenSaver",
        ))
    } else if is("KDE") {
        Some((
            "org.kde.screensaver",
            "/org/freedesktop/ScreenSaver",
            "org.freedesktop.ScreenSaver",
        ))
    } else {
        None
    }
}

fn single_window(reply: &xproto::GetPropertyReply) -> Option<u32> {
    let values = property32(reply, u32::from(xproto::AtomEnum::WINDOW))?;
    match values.as_slice() {
        [window] if *window != 0 => Some(*window),
        _ => None,
    }
}

// x11rb decodes protocol CARD32 into u32, not platform-sized unsigned long.
// Reject truncation/type mismatch rather than guessing false from a partial list.
fn property32(reply: &xproto::GetPropertyReply, expected_type: u32) -> Option<Vec<u32>> {
    if reply.type_ != expected_type
        || reply.format != 32
        || reply.bytes_after != 0
        || reply.value.len() != usize::try_from(reply.value_len).ok()?.checked_mul(4)?
    {
        return None;
    }
    Some(reply.value32()?.collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_hints_never_prove_unlocked_alone() {
        assert_eq!(session_gate(Some(false), None), Some(Reading::Known(false)));
        assert_eq!(session_gate(None, Some(true)), Some(Reading::Known(false)));
        assert!(matches!(
            session_gate(Some(true), None),
            Some(Reading::Unavailable(_))
        ));
        assert!(matches!(
            session_gate(None, Some(false)),
            Some(Reading::Unavailable(_))
        ));
        assert_eq!(session_gate(Some(true), Some(false)), None);
    }

    #[test]
    fn desktop_and_display_identity_are_scoped() {
        assert!(same_display(Some(":0"), Some(":0.0")));
        assert!(!same_display(Some(":0"), Some(":1")));
        assert!(!same_display(None, Some(":0")));
        assert!(!same_display(Some(":0"), Some("remote:0")));
        assert!(desktop_service("", "ubuntu:GNOME").is_some());
        assert!(desktop_service("KDE", "").is_some());
        assert!(desktop_service("", "XFCE").is_none());
    }

    #[test]
    fn missing_and_malformed_bus_properties_remain_unknown() {
        let mut properties = HashMap::new();
        assert_eq!(take_bool(&mut properties, "Active"), None);
        properties.insert("Active".into(), OwnedValue::from(123_u32));
        assert_eq!(take_bool(&mut properties, "Active"), None);
        properties.insert("Active".into(), OwnedValue::from(false));
        assert_eq!(take_bool(&mut properties, "Active"), Some(false));
    }

    #[test]
    fn wayland_never_uses_xwayland() {
        assert_eq!(
            desktop_reason(Some("wayland"), None, Some(":0")),
            Some(WAYLAND)
        );
        assert_eq!(
            desktop_reason(Some("x11"), Some("wayland-0"), Some(":0")),
            Some(WAYLAND)
        );
        assert_eq!(desktop_reason(None, None, None), Some(NO_SESSION));
        assert_eq!(desktop_reason(Some("x11"), None, Some(":0")), None);
    }

    fn property(values: &[u32]) -> xproto::GetPropertyReply {
        xproto::GetPropertyReply {
            format: 32,
            sequence: 0,
            length: values.len() as u32,
            type_: u32::from(xproto::AtomEnum::ATOM),
            bytes_after: 0,
            value_len: values.len() as u32,
            value: values.iter().flat_map(|v| v.to_ne_bytes()).collect(),
        }
    }

    #[test]
    fn protocol_words_are_32_bit_even_on_64_bit_hosts() {
        let reply = property(&[1, 0xffff_ffff, 72]);
        assert_eq!(
            property32(&reply, reply.type_),
            Some(vec![1, 0xffff_ffff, 72])
        );
    }

    #[test]
    fn malformed_truncated_and_wrong_type_are_unknown() {
        let mut reply = property(&[7]);
        assert!(property32(&reply, 1234).is_none());
        reply.bytes_after = 4;
        assert!(property32(&reply, reply.type_).is_none());
        reply.bytes_after = 0;
        reply.format = 8;
        assert!(property32(&reply, reply.type_).is_none());
        reply.format = 32;
        reply.value_len = 2;
        assert!(property32(&reply, reply.type_).is_none());
    }

    #[test]
    fn sampling_does_not_refresh_a_stalled_worker_timestamp() {
        let (tx, _rx) = mpsc::sync_channel(1);
        let (_result_tx, result) = mpsc::sync_channel(1);
        let old = Instant::now() - Duration::from_secs(10);
        let snapshot = SensorSnapshot::unavailable(old, "test");
        let mut sensors = NativeSensors {
            request: Some(tx),
            result,
            last: Some(snapshot),
            unavailable: "test",
        };
        let now = Instant::now();
        assert_eq!(sensors.sample(now).sampled_at, old);
        assert!(!sensors.sample(now).is_fresh(now));
    }
}
