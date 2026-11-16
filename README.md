# Subterranauts auto splitter

A Rust/WebAssembly auto splitter for LiveSplit's
[Auto Splitting Runtime](https://github.com/LiveSplit/asr).

Targets the Windows x64 Mono build of **Subterranauts 1.0.31**
(Unity 2022.3.62f2).

## Timing

The initial checkpoint is an **untimed preparation area**. Players may join,
shop, collect equipment, open doors, change suits, and position themselves
before the run.

Timing starts when the host submits the initial successful `LEAVE_CHECKPOINT`
command. The gate-opening animation and departure delay count toward the run.
For video timing, use the frame that the command is submitted.

Assignment splits occur when the checkpoint lever interaction completes and the
game accepts the assignment and sets its door's success flag.

Create one segment per assignment you want to complete. For example, ten
segments will time ten completed assignments. Solo and co-op use the same
start convention. In co-op, run the splitter on the host's machine and use
the host's recording as the timing reference.

| Setting | Default | Trigger |
| --- | --- | --- |
| Start on the initial departure command | On | Host's first successful `LEAVE_CHECKPOINT` command in a freshly hosted game |
| Split on completed assignments | On | Checkpoint door changes from closed to accepted/open |
| Split on station arrivals | Off | Travel to a scrap station finishes; includes the first station, excludes checkpoints |
| Reset on game over | Off | Game-over screen appears during a running or paused attempt |

Station splits are additional to assignment splits. To time stations only,
disable assignment splits. Station counts vary with the route.

Use **Real Time**. There is no load removal or game-time override. This is
done because player actions are allowed during most loading and travel
scenarios.

## Build and use

Install [Rust](https://rustup.rs/), then:

```sh
cargo wasm --locked
```

The output is:

```text
target/wasm32-unknown-unknown/release/subterranauts_autosplitter.wasm
```

Load that file in a [LiveSplit One](https://github.com/LiveSplit/LiveSplitOne).
This is not an ASL script. The browser-only LiveSplit One page cannot read a
game's memory and therefore cannot be used to time the game.

Load the splitter **before hosting a new game** so it can identify the fresh
session. Lobby preparation may take as long as needed. Loaded saves, joining another
host, and attaching after the fresh-host transition do not arm auto-start,
you can still start manually and time subsequent events.

Begin each new attempt from a freshly hosted game. The game-over screen's
restart skips the initial checkpoint and retains state.

The splitter only reads process memory. Exiting the game, returning to the menu,
or losing readable state doesn't reset the timer. Completed attempts are not
automatically reset. Events observed while paused or disabled are not queued.

## Development

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo check-wasm
cargo fmt --check
```

`src/logic.rs` handles transitions without runtime dependencies.
`src/runtime/memory.rs` resolves Mono fields and reads snapshots.
`src/runtime/terminal.rs` recognizes the accepted initial command.
`src/runtime/mod.rs` connects them to LiveSplit and its settings.

See [the game-state notes](docs/game-state.md) for the fields and assumptions
behind the triggers.
