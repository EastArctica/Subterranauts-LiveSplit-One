mod memory;
mod terminal;

use asr::{
    future::next_tick,
    game_engine::unity::mono::{profiles, Module},
    settings::Gui,
    timer::{self, TimerState},
    Process,
};

use crate::logic::{Action, Options, Timer, Tracker};

asr::async_main!(stable);
asr::panic_handler!();

#[derive(Gui)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Independent checkboxes in LiveSplit."
)]
struct Settings {
    /// Start on the initial departure command
    ///
    /// Preparation at the initial checkpoint is untimed. Starts when the host's
    /// command is accepted, including the gate-opening and departure sequence.
    #[default = true]
    start: bool,
    /// Split on completed assignments
    ///
    /// Splits when the checkpoint accepts the assignment and starts opening its door.
    #[default = true]
    assignments: bool,
    /// Split on station arrivals
    ///
    /// Adds a split when travel to a scrap station finishes, including the first station.
    #[default = false]
    stations: bool,
    /// Reset on new game
    ///
    /// Resets running, paused, or finished attempts when a new game is hosted
    /// from the main menu. Loading a save or joining another host does not
    /// reset.
    #[default = true]
    reset_on_new_game: bool,
}

impl From<&Settings> for Options {
    fn from(settings: &Settings) -> Self {
        Self {
            start: settings.start,
            assignments: settings.assignments,
            stations: settings.stations,
            reset_on_new_game: settings.reset_on_new_game,
        }
    }
}

impl From<TimerState> for Timer {
    fn from(state: TimerState) -> Self {
        match state {
            TimerState::NotRunning => Self::NotRunning,
            TimerState::Running => Self::Running,
            TimerState::Paused => Self::Paused,
            TimerState::Ended => Self::Ended,
            _ => Self::Unknown,
        }
    }
}

async fn main() {
    let mut settings = Settings::register();
    loop {
        let process = Process::wait_attach("Subterranauts.exe").await;
        asr::print_message("Attached to Subterranauts.exe");
        process
            .until_closes(async {
                // Unity 2022.3 uses this Mono layout. Game fields are still
                // resolved by name; this profile describes Mono's own metadata.
                let module = Module::wait_attach(
                    &process,
                    profiles::UNITY_2021_2_0F1_WINDOWS_MONO_BDWGC_X86_64,
                )
                .await;
                asr::print_message("Found Mono; waiting for game classes");
                let mut memory = memory::Memory::resolve(&process, &module).await;
                asr::print_message("Resolved game fields");
                let mut tracker = Tracker::default();
                let mut readable = false;
                loop {
                    settings.update();
                    if let Some(snapshot) = memory.read(&process, &module) {
                        if !readable {
                            asr::print_limited::<256>(&format_args!(
                                "Reading game state: assignment={}, station={}, started={}, checkpoint={}, traveling={}, entry={:?}, player_ready={}",
                                snapshot.assignment, snapshot.station, snapshot.started,
                                snapshot.at_checkpoint, snapshot.traveling,
                                snapshot.entry, snapshot.player_ready,
                            ));
                        }
                        readable = true;
                        if let Some(action) =
                            tracker.update(snapshot, timer::state().into(), (&settings).into())
                        {
                            apply(action);
                        }
                    } else {
                        if readable {
                            asr::print_message("Game state unavailable; waiting");
                        }
                        readable = false;
                        tracker.interrupt();
                    }
                    next_tick().await;
                }
            })
            .await;
        asr::print_message("Subterranauts.exe closed");
    }
}

fn apply(action: Action) {
    match action {
        Action::Start => {
            asr::print_message("Start: initial LEAVE_CHECKPOINT accepted");
            timer::start();
        }
        Action::SplitAssignment => {
            asr::print_message("Split: assignment completed");
            timer::split();
        }
        Action::SplitStation => {
            asr::print_message("Split: station arrived");
            timer::split();
        }
        Action::Reset => {
            asr::print_message("Reset: new game");
            timer::reset();
        }
    }
}
