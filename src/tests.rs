use crate::logic::{Action, Checkpoint, Options, SessionEntry, Snapshot, Timer, Tracker};

// Representative states from the game paths documented in docs/game-state.md.
// These are hand-written snapshots, not recordings or a mock memory reader.
const LOBBY: Snapshot = Snapshot {
    game: 1,
    entry: Some(SessionEntry::NewGame),
    player_ready: true,
    departure: None,
    started: false,
    assignment: 0,
    station: 0,
    at_checkpoint: true,
    traveling: false,
    checkpoint: Some(Checkpoint {
        id: 10,
        door_open: false,
    }),
    game_over: false,
};
const ACCEPTED: Snapshot = Snapshot {
    entry: None,
    departure: Some(100),
    ..LOBBY
};
const TRAVELING: Snapshot = Snapshot {
    entry: None,
    started: true,
    assignment: 1,
    station: 1,
    at_checkpoint: false,
    traveling: true,
    checkpoint: None,
    ..LOBBY
};
const CHECKPOINT: Snapshot = Snapshot {
    at_checkpoint: true,
    traveling: false,
    checkpoint: Some(Checkpoint {
        id: 20,
        door_open: false,
    }),
    ..TRAVELING
};
const COMPLETED: Snapshot = Snapshot {
    checkpoint: Some(Checkpoint {
        id: 20,
        door_open: true,
    }),
    ..CHECKPOINT
};
const ALL_EVENTS: Options = Options {
    start: true,
    assignments: true,
    stations: true,
    reset_on_game_over: true,
};

#[test]
fn default_run_starts_on_acceptance_and_splits_once_per_assignment() {
    let mut tracker = Tracker::default();
    let options = Options::default();
    let mut preparing = Snapshot {
        player_ready: false,
        ..LOBBY
    };
    assert_eq!(tracker.update(preparing, Timer::NotRunning, options), None);

    // The host marker disappears during preparation. Opening the gate alone
    // must not start timing; only the accepted command does.
    preparing.entry = None;
    preparing.player_ready = true;
    assert_eq!(tracker.update(preparing, Timer::NotRunning, options), None);
    preparing.checkpoint.as_mut().unwrap().door_open = true;
    assert_eq!(tracker.update(preparing, Timer::NotRunning, options), None);
    preparing.departure = Some(100);
    assert_eq!(
        tracker.update(preparing, Timer::NotRunning, options),
        Some(Action::Start)
    );
    // A manual timer reset must not replay the start.
    assert_eq!(tracker.update(preparing, Timer::NotRunning, options), None);
    preparing.departure = Some(101);
    assert_eq!(tracker.update(preparing, Timer::NotRunning, options), None);

    for assignment in 1..=2 {
        let traveling = Snapshot {
            assignment,
            station: assignment,
            ..TRAVELING
        };
        let closed = Snapshot {
            assignment,
            station: assignment,
            ..CHECKPOINT
        };
        let open = Snapshot {
            assignment,
            station: assignment,
            ..COMPLETED
        };
        // Counter increments, station arrival, and checkpoint arrival aren't
        // assignment completion. Default settings don't add station splits.
        for snapshot in [
            traveling,
            Snapshot {
                traveling: false,
                ..traveling
            },
            closed,
        ] {
            assert_eq!(tracker.update(snapshot, Timer::Running, options), None);
        }
        assert_eq!(
            tracker.update(open, Timer::Running, options),
            Some(Action::SplitAssignment)
        );
        assert_eq!(tracker.update(open, Timer::Running, options), None);
        // Reopening the same assignment's door must not split again.
        assert_eq!(tracker.update(closed, Timer::Running, options), None);
        assert_eq!(tracker.update(open, Timer::Running, options), None);
    }
}

#[test]
fn saves_joins_and_late_attachment_do_not_start_or_split_retroactively() {
    let options = Options::default();
    for entry in [None, Some(SessionEntry::LoadSave), Some(SessionEntry::Join)] {
        let mut tracker = Tracker::default();
        assert_eq!(
            tracker.update(Snapshot { entry, ..LOBBY }, Timer::NotRunning, options),
            None
        );
        assert_eq!(tracker.update(ACCEPTED, Timer::NotRunning, options), None);
    }
    for (snapshot, timer) in [(ACCEPTED, Timer::NotRunning), (COMPLETED, Timer::Running)] {
        let mut tracker = Tracker::default();
        assert_eq!(tracker.update(snapshot, timer, options), None);
        assert_eq!(tracker.update(snapshot, timer, options), None);
    }
}

#[test]
fn a_missing_or_replaced_checkpoint_is_not_a_completion_edge() {
    let options = Options::default();
    for checkpoint in [
        None,
        Some(Checkpoint {
            id: 99,
            door_open: false,
        }),
    ] {
        let mut tracker = Tracker::default();
        assert_eq!(
            tracker.update(
                Snapshot {
                    checkpoint,
                    ..CHECKPOINT
                },
                Timer::Running,
                options
            ),
            None
        );
        assert_eq!(tracker.update(COMPLETED, Timer::Running, options), None);
        // Ignoring that apparent edge must not consume a later real acceptance.
        assert_eq!(tracker.update(CHECKPOINT, Timer::Running, options), None);
        assert_eq!(
            tracker.update(COMPLETED, Timer::Running, options),
            Some(Action::SplitAssignment)
        );
    }
}

#[test]
fn station_only_mode_splits_new_stations_but_not_checkpoints_or_repeat_visits() {
    let mut tracker = Tracker::default();
    let options = Options {
        assignments: false,
        stations: true,
        ..Options::default()
    };
    assert_eq!(
        tracker.update(
            Snapshot {
                traveling: true,
                ..CHECKPOINT
            },
            Timer::Running,
            options
        ),
        None
    );
    assert_eq!(tracker.update(CHECKPOINT, Timer::Running, options), None);

    for station in 2..=3 {
        let traveling = Snapshot {
            station,
            ..TRAVELING
        };
        let arrived = Snapshot {
            traveling: false,
            ..traveling
        };
        assert_eq!(tracker.update(traveling, Timer::Running, options), None);
        assert_eq!(
            tracker.update(arrived, Timer::Running, options),
            Some(Action::SplitStation)
        );
        // A read gap and another visit must not forget an already split station.
        tracker.interrupt();
        assert_eq!(tracker.update(traveling, Timer::Running, options), None);
        assert_eq!(tracker.update(arrived, Timer::Running, options), None);
    }
}

#[test]
fn disabled_or_paused_events_are_not_queued_for_later() {
    for (start_timer, split_timer, options) in [
        (
            Timer::NotRunning,
            Timer::Running,
            Options {
                start: false,
                assignments: false,
                stations: false,
                ..Options::default()
            },
        ),
        (Timer::Paused, Timer::Paused, ALL_EVENTS),
    ] {
        let mut tracker = Tracker::default();
        assert_eq!(tracker.update(LOBBY, start_timer, options), None);
        assert_eq!(tracker.update(ACCEPTED, start_timer, options), None);
        assert_eq!(
            tracker.update(
                Snapshot {
                    departure: Some(101),
                    ..ACCEPTED
                },
                Timer::NotRunning,
                ALL_EVENTS
            ),
            None
        );

        let route = [
            TRAVELING,
            Snapshot {
                traveling: false,
                ..TRAVELING
            },
            CHECKPOINT,
            COMPLETED,
        ];
        for snapshot in route {
            assert_eq!(tracker.update(snapshot, split_timer, options), None);
        }
        // Enabling splitting/resuming, even followed by another visit and door
        // opening, must not replay the events observed above.
        for snapshot in route {
            assert_eq!(tracker.update(snapshot, Timer::Running, ALL_EVENTS), None);
        }
    }
}

#[test]
fn a_read_failure_does_not_invent_a_start_split_or_reset() {
    for (before, after, timer) in [
        (LOBBY, ACCEPTED, Timer::NotRunning),
        (
            TRAVELING,
            Snapshot {
                traveling: false,
                ..TRAVELING
            },
            Timer::Running,
        ),
        (CHECKPOINT, COMPLETED, Timer::Running),
        (
            CHECKPOINT,
            Snapshot {
                game_over: true,
                ..CHECKPOINT
            },
            Timer::Running,
        ),
    ] {
        let mut tracker = Tracker::default();
        assert_eq!(tracker.update(before, timer, ALL_EVENTS), None);
        tracker.interrupt();
        assert_eq!(tracker.update(after, timer, ALL_EVENTS), None);
    }
}

#[test]
fn game_over_reset_is_opt_in_and_never_resets_a_finished_timer() {
    // Include a simultaneous door edge: game over must win over splitting.
    let failed = Snapshot {
        game_over: true,
        ..COMPLETED
    };
    let mut tracker = Tracker::default();
    assert_eq!(
        tracker.update(CHECKPOINT, Timer::Running, Options::default()),
        None
    );
    assert_eq!(
        tracker.update(failed, Timer::Running, Options::default()),
        None
    );

    for (timer, expected) in [
        (Timer::Running, Some(Action::Reset)),
        (Timer::Paused, Some(Action::Reset)),
        (Timer::NotRunning, None),
        (Timer::Ended, None),
        (Timer::Unknown, None),
    ] {
        let mut tracker = Tracker::default();
        assert_eq!(tracker.update(CHECKPOINT, timer, ALL_EVENTS), None);
        assert_eq!(tracker.update(failed, timer, ALL_EVENTS), expected);
        assert_eq!(tracker.update(failed, timer, ALL_EVENTS), None);
    }
}

#[test]
fn only_a_new_game_rearms_consumed_starts_and_splits() {
    let mut tracker = Tracker::default();
    let route = [
        (LOBBY, Timer::NotRunning, None),
        (ACCEPTED, Timer::NotRunning, Some(Action::Start)),
        (TRAVELING, Timer::Running, None),
        (
            Snapshot {
                traveling: false,
                ..TRAVELING
            },
            Timer::Running,
            Some(Action::SplitStation),
        ),
        (CHECKPOINT, Timer::Running, None),
        (COMPLETED, Timer::Running, Some(Action::SplitAssignment)),
    ];
    for (snapshot, timer, expected) in route {
        assert_eq!(tracker.update(snapshot, timer, ALL_EVENTS), expected);
    }
    assert_eq!(
        tracker.update(
            Snapshot {
                game_over: true,
                ..COMPLETED
            },
            Timer::Running,
            ALL_EVENTS
        ),
        Some(Action::Reset)
    );
    tracker.interrupt();
    // The game's in-place restart skips the lobby and retains consumed events.
    assert_eq!(
        tracker.update(TRAVELING, Timer::NotRunning, ALL_EVENTS),
        None
    );
    for (snapshot, _, _) in &route[3..] {
        assert_eq!(tracker.update(*snapshot, Timer::Running, ALL_EVENTS), None);
    }
    // Hosting again replaces the game object, allowing the same events anew.
    for (snapshot, timer, expected) in route {
        assert_eq!(
            tracker.update(
                Snapshot {
                    game: 2,
                    ..snapshot
                },
                timer,
                ALL_EVENTS
            ),
            expected
        );
    }
}
