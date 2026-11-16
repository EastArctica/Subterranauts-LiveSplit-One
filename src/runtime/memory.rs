use asr::{
    game_engine::unity::mono::{Class, Module},
    Address, PointerSize, Process,
};

use crate::logic::{Checkpoint, SessionEntry, Snapshot};

use super::terminal::DepartureReader;

// The first field after the two-word Mono object header on x64 is
// UnityEngine.Object.m_CachedPtr. Destroyed Unity wrappers can remain in statics.
const CACHED_PTR: u64 = 0x10;

fn read_object(process: &Process, slot: Address) -> Result<Option<Address>, asr::Error> {
    let object = process.read_pointer(slot, PointerSize::Bit64)?;
    if object.is_null()
        || process
            .read_pointer(object + CACHED_PTR, PointerSize::Bit64)?
            .is_null()
    {
        return Ok(None);
    }
    Ok(Some(object))
}

struct Singleton {
    slot: Address,
}

impl Singleton {
    async fn resolve(process: &Process, module: &Module, class: Class) -> Self {
        Self {
            slot: class.wait_get_static_table(process, module).await
                + class
                    .wait_get_field_offset(process, module, "instance")
                    .await,
        }
    }

    fn read(&self, process: &Process) -> Result<Option<Address>, asr::Error> {
        read_object(process, self.slot)
    }
}

pub(super) struct Memory {
    game: Singleton,
    surface: Singleton,
    checkpoint: Singleton,
    game_over: Singleton,
    menu: Singleton,
    network: Singleton,
    terminal: Singleton,
    departure: DepartureReader,
    menu_action: u32,
    local_data: u32,
    started: u32,
    assignment: u32,
    station: u32,
    at_checkpoint: u32,
    traveling: u32,
    door_open: u32,
    shown: u32,
}

impl Memory {
    pub async fn resolve(process: &Process, module: &Module) -> Self {
        let image = module.wait_get_default_image(process).await;
        let game = image
            .wait_get_class(process, module, "GameController")
            .await;
        let surface = image
            .wait_get_class(process, module, "SurfaceGenerator")
            .await;
        let checkpoint = image
            .wait_get_class(process, module, "CheckpointController")
            .await;
        let game_over = image
            .wait_get_class(process, module, "UIGameOverScreen")
            .await;
        let menu = image
            .wait_get_class(process, module, "MainMenuController")
            .await;
        let network = image
            .wait_get_class(process, module, "NetworkManager")
            .await;
        let terminal = image
            .wait_get_class(process, module, "TerminalManager")
            .await;
        Self {
            game: Singleton::resolve(process, module, game).await,
            surface: Singleton::resolve(process, module, surface).await,
            checkpoint: Singleton::resolve(process, module, checkpoint).await,
            game_over: Singleton::resolve(process, module, game_over).await,
            menu: Singleton::resolve(process, module, menu).await,
            network: Singleton::resolve(process, module, network).await,
            terminal: Singleton::resolve(process, module, terminal).await,
            departure: DepartureReader::resolve(process, module, &image).await,
            menu_action: menu.wait_get_field_offset(process, module, "action").await,
            local_data: network
                .wait_get_field_offset(process, module, "localData")
                .await,
            started: game
                .wait_get_field_offset(process, module, "gameStarted")
                .await,
            assignment: game
                .wait_get_field_offset(process, module, "numMapsCompleted")
                .await,
            station: game
                .wait_get_field_offset(process, module, "numZonesCompleted")
                .await,
            at_checkpoint: game
                .wait_get_field_offset(process, module, "atCheckpoint")
                .await,
            traveling: surface
                .wait_get_field_offset(process, module, "traveling")
                .await,
            door_open: checkpoint
                .wait_get_field_offset(process, module, "doorOpen")
                .await,
            shown: game_over
                .wait_get_field_offset(process, module, "shown")
                .await,
        }
    }

    pub fn read(&mut self, process: &Process, module: &Module) -> Option<Snapshot> {
        let game = self.game.read(process).ok()??;
        let surface = self.surface.read(process).ok()??;
        let game_over = self.game_over.read(process).ok()??;
        let menu = self.menu.read(process).ok()?;
        let network = self.network.read(process).ok()??;
        let local_data = read_object(process, network + self.local_data).ok()?;
        let terminal = self.terminal.read(process).ok()??;
        // MainMenuController.ActionTypes. Offline (4) is unused by the menu;
        // offline hosting still enters through HostNew or HostSave.
        let entry = match menu {
            Some(menu) => match process.read::<i32>(menu + self.menu_action).ok()? {
                0 | 4 => None,
                1 => Some(SessionEntry::NewGame),
                2 => Some(SessionEntry::LoadSave),
                3 => Some(SessionEntry::Join),
                _ => return None,
            },
            None => None,
        };
        let checkpoint = self.checkpoint.read(process).ok()?;
        let assignment = process.read::<i32>(game + self.assignment).ok()?;
        let station = process.read::<i32>(game + self.station).ok()?;
        if assignment < 0 || station < 0 {
            return None;
        }
        let checkpoint_state = match checkpoint {
            Some(object) => Some(Checkpoint {
                id: object.value(),
                door_open: process.read(object + self.door_open).ok()?,
            }),
            None => None,
        };
        let snapshot = Snapshot {
            game: game.value(),
            entry,
            player_ready: local_data.is_some(),
            departure: if assignment == 0 && station == 0 {
                self.departure.read(process, module, terminal)?.marker
            } else {
                None
            },
            started: process.read(game + self.started).ok()?,
            assignment,
            station,
            at_checkpoint: process.read(game + self.at_checkpoint).ok()?,
            traveling: process.read(surface + self.traveling).ok()?,
            checkpoint: checkpoint_state,
            game_over: process.read(game_over + self.shown).ok()?,
        };

        // Reject a scene replacement or counter update partway through sampling.
        // This is not an atomic snapshot, but avoids combining two assignments.
        if self.game.read(process).ok()? != Some(game)
            || self.surface.read(process).ok()? != Some(surface)
            || self.game_over.read(process).ok()? != Some(game_over)
            || self.menu.read(process).ok()? != menu
            || self.network.read(process).ok()? != Some(network)
            || self.terminal.read(process).ok()? != Some(terminal)
            || read_object(process, network + self.local_data).ok()? != local_data
            || self.checkpoint.read(process).ok()? != checkpoint
            || process.read::<i32>(game + self.assignment).ok()? != assignment
            || process.read::<i32>(game + self.station).ok()? != station
        {
            return None;
        }
        Some(snapshot)
    }
}
