/* Copyright 2023 shadow3aaa@gitbub.com
*
*  Licensed under the Apache License, Version 2.0 (the "License");
*  you may not use this file except in compliance with the License.
*  You may obtain a copy of the License at
*
*      http://www.apache.org/licenses/LICENSE-2.0
*
*  Unless required by applicable law or agreed to in writing, software
*  distributed under the License is distributed on an "AS IS" BASIS,
*  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
*  See the License for the specific language governing permissions and
*  limitations under the License. */
mod mode_policy;
mod policy;
mod utils;

use std::{
    collections::{HashMap, VecDeque},
    sync::mpsc::{Receiver, RecvTimeoutError},
    time::{Duration, Instant},
};

use log::debug;
use sliding_features::{Echo, View, ALMA};

use super::{topapp::TimedWatcher, FasData};
use crate::{
    config::{Config, TargetFps},
    error::{Error, Result},
    PerformanceController,
};

const FRAME_UNIT: usize = 5;
const BUFFER_MAX: usize = 30;

pub type Buffers = HashMap<Process, Buffer>; // Process, (jank_scale, total_jank_time_ns)
pub type Process = (String, i32); // process, pid

#[derive(Debug)]
pub struct Buffer {
    pub target_fps: TargetFps,
    pub frametimes: VecDeque<Duration>,
    pub frame_unit: VecDeque<Duration>,
    pub last_jank: Option<Instant>,
    pub last_limit: Option<Instant>,
    pub rec_counter: u8,
    smoother: ALMA<Echo>,
}

impl Buffer {
    pub fn new(target_fps: TargetFps) -> Self {
        Self {
            target_fps,
            frametimes: VecDeque::with_capacity(BUFFER_MAX),
            frame_unit: VecDeque::with_capacity(FRAME_UNIT),
            last_jank: None,
            last_limit: None,
            rec_counter: 0,
            smoother: ALMA::new(Echo::new(), FRAME_UNIT * 2),
        }
    }

    pub fn push_frametime(&mut self, d: Duration) {
        if self.frametimes.len() >= BUFFER_MAX {
            self.frametimes.pop_back();
        }

        if self.frame_unit.len() >= FRAME_UNIT {
            self.frame_unit.pop_back();
        }

        self.smoother.update(d.as_nanos() as f64);
        let smoothed_frame = Duration::from_nanos(self.smoother.last() as u64);

        self.frametimes.push_front(d);
        self.frame_unit.push_front(smoothed_frame);
    }
}

pub struct Looper<P: PerformanceController> {
    rx: Receiver<FasData>,
    config: Config,
    controller: P,
    topapp_checker: TimedWatcher,
    buffers: Buffers,
    started: bool,
}

impl<P: PerformanceController> Looper<P> {
    pub fn new(rx: Receiver<FasData>, config: Config, controller: P) -> Self {
        Self {
            rx,
            config,
            controller,
            topapp_checker: TimedWatcher::new(),
            buffers: Buffers::new(),
            started: false,
        }
    }

    pub fn enter_loop(&mut self) -> Result<()> {
        loop {
            let data = if self.buffers.is_empty() {
                Some(
                    self.rx
                        .recv()
                        .map_err(|_| Error::Other("Binder Disconnected"))?,
                )
            } else {
                match self.rx.recv_timeout(Duration::from_secs(1)) {
                    Ok(d) => Some(d),
                    Err(e) => {
                        if e == RecvTimeoutError::Disconnected {
                            return Err(Error::Other("Binder Disconnected"));
                        }

                        self.retain_topapp()?;
                        continue;
                    }
                }
            };

            if let Some(data) = data {
                self.buffer_update(&data);
            }

            self.retain_topapp()?;
            self.buffers_policy()?;

            if self.started {
                debug!("{:#?}", self.buffers);
            }
        }
    }
}