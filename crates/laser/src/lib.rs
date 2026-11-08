//! Hardware-neutral laser production primitives for VectorCraft.
//!
//! This crate deliberately does not assume a controller protocol. The Reilogic Series 9
//! controller's transport and command dialect must be established from the actual machine,
//! board markings and public/vendor documentation before a hardware backend is added.
//!
//! The initial boundary models two independently addressable controller boards, validates
//! production settings, and requires explicit interlock confirmation before a backend may
//! start a job. It contains no serial, USB, network or GPIO implementation.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

/// The machine has two controller-board positions. They are not assumed to be interchangeable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BoardSlot {
    Primary,
    Secondary,
}

impl BoardSlot {
    pub const ALL: [Self; 2] = [Self::Primary, Self::Secondary];
}

/// Controller connection state as reported by a future hardware backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Ready,
    Busy,
    Fault,
}

/// Safety inputs required before a production job can be started.
///
/// A backend must populate these from real machine signals; the UI must never synthesize
/// a safe value merely because a sensor is absent or unsupported.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SafetyInterlocks {
    pub emergency_stop_released: bool,
    pub lid_closed: bool,
    pub exhaust_ready: bool,
    pub cooling_ready: bool,
}

impl SafetyInterlocks {
    pub fn permits_laser_emission(self) -> bool {
        self.emergency_stop_released && self.lid_closed && self.exhaust_ready && self.cooling_ready
    }
}

/// User-editable production parameters. Power is a percentage; speed is mm/s.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LaserSettings {
    pub power_percent: f32,
    pub speed_mm_per_second: f32,
    pub passes: u16,
}

impl LaserSettings {
    pub fn validate(self) -> Result<Self, LaserError> {
        if !self.power_percent.is_finite() || !(0.0..=100.0).contains(&self.power_percent) {
            return Err(LaserError::InvalidPower);
        }
        if !self.speed_mm_per_second.is_finite() || self.speed_mm_per_second <= 0.0 {
            return Err(LaserError::InvalidSpeed);
        }
        if self.passes == 0 || self.passes > 100 {
            return Err(LaserError::InvalidPassCount);
        }
        Ok(self)
    }
}

/// A bounded, backend-neutral job description. Geometry conversion is supplied separately.
#[derive(Clone, Debug, PartialEq)]
pub struct LaserJob {
    pub name: String,
    pub settings: LaserSettings,
    pub estimated_duration_seconds: Option<u64>,
}

impl LaserJob {
    pub fn validate(&self) -> Result<(), LaserError> {
        if self.name.trim().is_empty() || self.name.len() > 256 {
            return Err(LaserError::InvalidJobName);
        }
        self.settings.validate()?;
        Ok(())
    }
}

/// A snapshot of one board's discoverable state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoardSnapshot {
    pub slot: BoardSlot,
    pub state: ConnectionState,
    pub interlocks: SafetyInterlocks,
}

/// Narrow interface implemented by a concrete, separately reviewed hardware backend.
///
/// Implementations must fail closed: missing/unknown safety inputs are false, and start_job
/// must independently re-check the interlocks immediately before enabling laser output.
pub trait LaserBackend {
    fn board_snapshot(&mut self, slot: BoardSlot) -> Result<BoardSnapshot, LaserError>;
    fn start_job(&mut self, slot: BoardSlot, job: &LaserJob) -> Result<(), LaserError>;
    fn stop_job(&mut self, slot: BoardSlot) -> Result<(), LaserError>;
}

/// Errors surfaced to the UI without terminating the application.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LaserError {
    #[error("laser power must be between 0 and 100 percent")]
    InvalidPower,
    #[error("laser speed must be a finite positive value")]
    InvalidSpeed,
    #[error("pass count must be between 1 and 100")]
    InvalidPassCount,
    #[error("job name must contain 1 to 256 bytes")]
    InvalidJobName,
    #[error("board is not ready")]
    BoardNotReady,
    #[error("one or more safety interlocks are not confirmed")]
    SafetyInterlockOpen,
    #[error("hardware backend error: {0}")]
    Backend(String),
}

/// Validates a job and the last observed board state before delegating to a backend.
///
/// The backend still has to re-check live interlocks inside start_job; this preflight is
/// an additional guard and a source of clear UI feedback, not a replacement for hardware safety.
pub fn start_job<B: LaserBackend>(
    backend: &mut B,
    slot: BoardSlot,
    job: &LaserJob,
) -> Result<(), LaserError> {
    job.validate()?;
    let board = backend.board_snapshot(slot)?;
    if board.state != ConnectionState::Ready {
        return Err(LaserError::BoardNotReady);
    }
    if !board.interlocks.permits_laser_emission() {
        return Err(LaserError::SafetyInterlockOpen);
    }
    backend.start_job(slot, job)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exactly_two_board_slots_are_exposed() {
        assert_eq!(BoardSlot::ALL, [BoardSlot::Primary, BoardSlot::Secondary]);
    }

    #[test]
    fn interlocks_fail_closed() {
        let safe = SafetyInterlocks {
            emergency_stop_released: true,
            lid_closed: true,
            exhaust_ready: true,
            cooling_ready: true,
        };
        assert!(safe.permits_laser_emission());
        assert!(!SafetyInterlocks { lid_closed: true, ..SafetyInterlocks::default() }.permits_laser_emission());
    }

    #[test]
    fn laser_settings_reject_non_finite_and_out_of_range_values() {
        let valid = LaserSettings { power_percent: 45.0, speed_mm_per_second: 120.0, passes: 1 };
        assert_eq!(valid.validate(), Ok(valid));
        assert_eq!(LaserSettings { power_percent: f32::NAN, ..valid }.validate(), Err(LaserError::InvalidPower));
        assert_eq!(LaserSettings { power_percent: 101.0, ..valid }.validate(), Err(LaserError::InvalidPower));
        assert_eq!(LaserSettings { speed_mm_per_second: 0.0, ..valid }.validate(), Err(LaserError::InvalidSpeed));
        assert_eq!(LaserSettings { passes: 0, ..valid }.validate(), Err(LaserError::InvalidPassCount));
    }

    struct FakeBackend {
        snapshot: BoardSnapshot,
        started: bool,
    }

    impl LaserBackend for FakeBackend {
        fn board_snapshot(&mut self, _slot: BoardSlot) -> Result<BoardSnapshot, LaserError> {
            Ok(self.snapshot)
        }

        fn start_job(&mut self, _slot: BoardSlot, _job: &LaserJob) -> Result<(), LaserError> {
            self.started = true;
            Ok(())
        }

        fn stop_job(&mut self, _slot: BoardSlot) -> Result<(), LaserError> {
            self.started = false;
            Ok(())
        }
    }

    #[test]
    fn preflight_does_not_start_when_interlock_is_open() {
        let mut backend = FakeBackend {
            snapshot: BoardSnapshot {
                slot: BoardSlot::Primary,
                state: ConnectionState::Ready,
                interlocks: SafetyInterlocks::default(),
            },
            started: false,
        };
        let job = LaserJob {
            name: "Stamp outline".to_string(),
            settings: LaserSettings { power_percent: 20.0, speed_mm_per_second: 80.0, passes: 1 },
            estimated_duration_seconds: None,
        };
        assert_eq!(start_job(&mut backend, BoardSlot::Primary, &job), Err(LaserError::SafetyInterlockOpen));
        assert!(!backend.started);
    }
}
