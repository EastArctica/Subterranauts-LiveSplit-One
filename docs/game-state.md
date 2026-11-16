# Game-state notes

## Build inspected

Subterranauts 1.0.31, Windows x64, Unity 2022.3.62f2, Mono backend.
The game logic was inspected via ILSpy against
`Subterranauts_Data/Managed/Assembly-CSharp.dll`:

```text
SHA-256  217e5559c249d7f3eb5b251b40adb7b7dd13407334608824e1d98b593967a0a6
MVID     5994ac5b-52d0-4a94-9408-5ddb27e93c70
```

## Signals

| Class | Fields read |
| --- | --- |
| `GameController` | `instance`, `gameStarted`, `numMapsCompleted`, `numZonesCompleted`, `atCheckpoint` |
| `SurfaceGenerator` | `instance`, `traveling` |
| `CheckpointController` | `instance`, `doorOpen` |
| `UIGameOverScreen` | `instance`, `shown` |
| `MainMenuController` | `instance`, `action` |
| `NetworkManager` | `instance`, `localData` |
| `TerminalManager` | `instance`, `response` |
| `TerminalLine` | `type`, `loadingTime`, `text` |
| `System.Collections.Generic.Queue<T>` | `_array`, `_head`, `_size`, `_version` |

`GameController.SendMapRPC` increments `numMapsCompleted` when a new assignment's
map is received. `SurfaceGenerator.GenerateSurfaceRoutine` increments
`numZonesCompleted` before generating a station.

`MainMenuController.action` distinguishes `HostNew` (1), `HostSave` (2), and
`Join` (3) throughout session creation. `LoadSceneAsync` retains that action
while creating or joining the room and clears it one second after entering the
room. Offline hosting uses the same host actions with the offline toggle set.

The tracker remembers `HostNew` for that game object. `HostSave` and `Join`
do not trigger the auto-start; both can pass through zero counters before
their saved/network data arrives. `NetworkManager.localData` must also reference
a live local player.

For the initial `LEAVE_CHECKPOINT`, `TerminalInterpreter.StartTrainCommand`
first rejects an already moving train, players outside the train, or enabled
cages. After those checks pass, it enqueues a `WheelLoading` response with
`loadingTime == 9` and text `"Opening door : "`. It then opens the checkpoint
gate and waits six seconds before starting the first assignment.

The queued response's arrival is the accepted-command signal. Merely entering
text, submitting invalid arguments, or receiving an error does not create
this response. A gate opened by the automatic respawn path does not create it
either. Reading the queue observes acceptance without waiting for the response
to be drawn or its animation to finish. `TerminalManager.OnGUI` writes the
submitted command to terminal history and dispatches it in the same call.
This is the video start marker; response rendering can happen later.

Auto-start requires a newly observed response object while still at assignment
0, station 0, with `gameStarted == false` and no travel. It is consumed even
when auto-start is disabled or the timer is already active. Attaching after
the response was enqueued does not start late.

`GameController.StartNewMapRPC` sets `gameStarted` after the six-second wait.
That event is not used as the delay would make manual timing of runs difficult.

`CheckpointController.OpenDoorRoutine` sets `doorOpen` immediately when accepting
an assignment, before the alarm and door animation. This also initiates the
reward. Assignment splits require a closed-to-open edge on the *same checkpoint
object*, in the same assignment, while not traveling. The introductory
checkpoint opens with `gameStarted == false` and does not count.

Travel ends at a station when `GenerateSurfaceRoutine` clears `traveling` after
releasing the cages and spawning resources. `GoToCheckpointRoutine` also clears
it, but leaves `atCheckpoint == true`; that arrival is excluded from station
splits.

`UIGameOverScreen.Show` raises `shown`. `GameController.ResetGameRPC` resets both
counters and starts a new first assignment without clearing `gameStarted`,
then hides the game-over screen. It skips the initial checkpoint and does not
re-arm auto-start or replay consumed splits.

## Memory and recovery

The reader uses ASR's Windows x64 Mono profile measured on Unity 2021.2, whose
layout is used by this game's Unity 2022.3 runtime. All game field offsets and
static tables are resolved by name. The only fixed object offset is `0x10`,
`UnityEngine.Object.m_CachedPtr` after the two-word Mono object header. A null
native pointer means the Unity object has been destroyed, even if its managed
wrapper remains in a static field.

Singletons are reread every tick. Missing game, surface, network, terminal, or game-over
objects invalidate the sample; a missing checkpoint is normal away from
checkpoints. A missing menu is normal after session creation, and a missing
local player means that the session is not ready to auto-start.
Failed reads are not replaced with zeroes. Singleton identities and progression
counters are checked again after sampling to reject obvious torn reads.
Reading another process cannot provide a truly atomic snapshot.

A failed sample breaks edge detection. Already consumed assignment and station
events remain consumed, including events seen with splitting disabled or the
timer paused. Only a new game object clears those guards. On process exit the
attachment and tracker are dropped, and attachment is retried.

The terminal reader resolves the queue and response fields from Mono metadata.
It scans live entries in ring-buffer order using ASR's managed-array and string
readers. Queue identity and version cache the result; a version change during
sampling rejects the read. Arrays with capacity above 4096 are rejected as a
resource bound, as are invalid counts, indices, or null live entries. The queue
is only inspected before the first assignment, so it adds no response scanning
during the timed stations.
