#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Checkpoint {
    pub id: u64,
    pub door_open: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SessionEntry {
    NewGame,
    LoadSave,
    Join,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Mirrors independent game fields."
)]
pub(crate) struct Snapshot {
    pub game: u64,
    pub entry: Option<SessionEntry>,
    pub player_ready: bool,
    pub departure: Option<u64>,
    pub started: bool,
    // Despite its name, numMapsCompleted counts assignments *started*.
    pub assignment: i32,
    pub station: i32,
    pub at_checkpoint: bool,
    pub traveling: bool,
    pub checkpoint: Option<Checkpoint>,
    pub game_over: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Timer {
    NotRunning,
    Running,
    Paused,
    Ended,
    Unknown,
}

#[derive(Clone, Copy, Debug)]
#[allow(clippy::struct_excessive_bools, reason = "Independent user settings.")]
pub(crate) struct Options {
    pub start: bool,
    pub assignments: bool,
    pub stations: bool,
    pub reset_on_game_over: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            start: true,
            assignments: true,
            stations: false,
            reset_on_game_over: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Start,
    SplitAssignment,
    SplitStation,
    Reset,
}

#[derive(Default)]
pub(crate) struct Tracker {
    previous: Option<Snapshot>,
    game: Option<u64>,
    completed_assignment: i32,
    arrived_station: i32,
    start_consumed: bool,
    fresh_game: bool,
}

impl Tracker {
    /// Break edge detection without forgetting already consumed events.
    pub fn interrupt(&mut self) {
        self.previous = None;
    }

    pub fn update(&mut self, current: Snapshot, timer: Timer, options: Options) -> Option<Action> {
        let previous = self
            .previous
            .replace(current)
            .filter(|old| old.game == current.game);
        if self.game != Some(current.game) {
            self.game = Some(current.game);
            self.completed_assignment = 0;
            self.arrived_station = if current.traveling {
                current.station.saturating_sub(1)
            } else {
                current.station
            };
            self.start_consumed = false;
            self.fresh_game = false;
        }

        if let Some(entry) = current.entry {
            self.fresh_game = entry == SessionEntry::NewGame;
        }
        if current.started || current.assignment != 0 || current.station != 0 || current.game_over {
            self.fresh_game = false;
            self.start_consumed = true;
        }
        let Some(previous) = previous else {
            // Loading the splitter after acceptance must not start it late.
            self.start_consumed |= current.departure.is_some();
            return None;
        };

        let initial_departure = self.fresh_game
            && current.player_ready
            && !current.started
            && current.assignment == 0
            && current.station == 0
            && current.at_checkpoint
            && current.checkpoint.is_some()
            && !current.traveling
            && !current.game_over
            && previous.at_checkpoint
            && previous.assignment == 0
            && previous.station == 0
            && !previous.started
            && !previous.traveling
            && current.departure.is_some()
            && current.departure != previous.departure;
        if !self.start_consumed && initial_departure {
            self.start_consumed = true;
            if timer == Timer::NotRunning && options.start {
                return Some(Action::Start);
            }
        }

        let failed = !previous.game_over && current.game_over;
        if failed {
            return (options.reset_on_game_over && matches!(timer, Timer::Running | Timer::Paused))
                .then_some(Action::Reset);
        }
        if !current.started || current.game_over {
            return None;
        }

        let assignment_completed = current.assignment > 0
            && current.assignment == previous.assignment
            && current.at_checkpoint
            && previous.at_checkpoint
            && !current.traveling
            && !previous.traveling
            && previous.started
            && !previous.game_over
            && matches!(
                (previous.checkpoint, current.checkpoint),
                (Some(old), Some(new))
                    if old.id == new.id && !old.door_open && new.door_open
            );
        if assignment_completed && current.assignment > self.completed_assignment {
            self.completed_assignment = current.assignment;
            if timer == Timer::Running && options.assignments {
                return Some(Action::SplitAssignment);
            }
        }

        let station_arrived = previous.traveling
            && !current.traveling
            && !current.at_checkpoint
            && current.station > 0
            && current.station == previous.station;
        if station_arrived && current.station > self.arrived_station {
            self.arrived_station = current.station;
            if timer == Timer::Running && options.stations {
                return Some(Action::SplitStation);
            }
        }
        None
    }
}
