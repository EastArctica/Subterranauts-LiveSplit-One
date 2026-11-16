use asr::{
    game_engine::unity::mono::{Image, Module},
    Address, PointerSize, Process,
};

// This bounds corrupt or unexpectedly large queues, not the number of commands
// a runner may use. Only the queue's allocated capacity is read by ASR.
const MAX_RESPONSE_CAPACITY: usize = 4096;
const WHEEL_LOADING: i32 = 3;
const OPENING_TIME: f32 = 9.0;
const OPENING_TITLE: &str = "Opening door : ";

#[derive(Clone, Copy, PartialEq, Eq)]
struct QueueState {
    terminal: Address,
    queue: Address,
    version: i32,
}

#[derive(Clone, Copy)]
pub(super) struct DepartureState {
    pub marker: Option<u64>,
}

pub(super) struct DepartureReader {
    response: u32,
    array: u32,
    head: u32,
    size: u32,
    version: u32,
    line_type: u32,
    loading_time: u32,
    text: u32,
    cached: Option<(QueueState, DepartureState)>,
}

impl DepartureReader {
    pub async fn resolve(process: &Process, module: &Module, image: &Image) -> Self {
        let terminal = image
            .wait_get_class(process, module, "TerminalManager")
            .await;
        let line = image.wait_get_class(process, module, "TerminalLine").await;
        let core = module.wait_get_image(process, "mscorlib").await;
        let queue = core
            .wait_get_class(process, module, "System.Collections.Generic.Queue`1")
            .await;
        Self {
            response: terminal
                .wait_get_field_offset(process, module, "response")
                .await,
            array: queue.wait_get_field_offset(process, module, "_array").await,
            head: queue.wait_get_field_offset(process, module, "_head").await,
            size: queue.wait_get_field_offset(process, module, "_size").await,
            version: queue
                .wait_get_field_offset(process, module, "_version")
                .await,
            line_type: line.wait_get_field_offset(process, module, "type").await,
            loading_time: line
                .wait_get_field_offset(process, module, "loadingTime")
                .await,
            text: line.wait_get_field_offset(process, module, "text").await,
            cached: None,
        }
    }

    /// Rejects unreadable queues rather than treating them as an absent command.
    pub fn read(
        &mut self,
        process: &Process,
        module: &Module,
        terminal: Address,
    ) -> Option<DepartureState> {
        let queue = process
            .read_pointer(terminal + self.response, PointerSize::Bit64)
            .ok()?;
        if queue.is_null() {
            return None;
        }
        let state = QueueState {
            terminal,
            queue,
            version: process.read(queue + self.version).ok()?,
        };
        if let Some((cached, departure)) = self.cached {
            if cached == state {
                return Some(departure);
            }
        }

        let head = usize::try_from(process.read::<i32>(queue + self.head).ok()?).ok()?;
        let size = usize::try_from(process.read::<i32>(queue + self.size).ok()?).ok()?;
        let array = module
            .read_reference_array::<MAX_RESPONSE_CAPACITY>(process, queue + self.array)
            .ok()?;
        if size > array.len() || (size > 0 && head >= array.len()) {
            return None;
        }

        let mut marker = None;
        for index in 0..size {
            let line = array[(head + index) % array.len()];
            if line.is_null() {
                return None;
            }
            // StartTrainCommand enqueues this line only after its departure
            // checks pass, before opening the gate and waiting six seconds.
            if process.read::<i32>(line + self.line_type).ok()? == WHEEL_LOADING
                && process.read::<u32>(line + self.loading_time).ok()? == OPENING_TIME.to_bits()
                && module
                    .read_string::<32>(process, line + self.text)
                    .ok()?
                    .matches_str(OPENING_TITLE)
            {
                marker = Some(line.value());
            }
        }

        if process.read::<i32>(queue + self.version).ok()? != state.version
            || process
                .read_pointer(terminal + self.response, PointerSize::Bit64)
                .ok()?
                != queue
        {
            return None;
        }
        let departure = DepartureState { marker };
        self.cached = Some((state, departure));
        Some(departure)
    }
}
