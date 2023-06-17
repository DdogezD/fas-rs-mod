mod process;

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use fas_rs_fw::prelude::*;

use process::*;

const CPUFREQ: &str = "/sys/devices/system/cpu/cpufreq";

pub(crate) type Frequency = usize;
pub(crate) type FrequencyTable = Vec<Frequency>;
pub struct CpuCommon {
    command_sender: SyncSender<Command>,
    thread_handle: JoinHandle<()>,
    pause: Arc<AtomicBool>,
}

pub(crate) enum Command {
    Stop,
    Release,
    Limit,
}

impl Drop for CpuCommon {
    fn drop(&mut self) {
        self.command_sender.send(Command::Stop).unwrap();
    }
}

impl VirtualPerformanceController for CpuCommon {
    fn support() -> bool
    where
        Self: Sized,
    {
        Path::new(CPUFREQ).exists()
    }

    fn new() -> Result<Self, Box<dyn Error>>
    where
        Self: Sized,
    {
        let mut table = Vec::with_capacity(2);

        let cpufreq = fs::read_dir(CPUFREQ)?;
        for policy in cpufreq {
            let path = policy?.path();
            let table_path = path.join("scaling_available_frequencies");
            let mut this_table: FrequencyTable = fs::read_to_string(table_path)?
                .split_whitespace()
                .filter_map(|freq| freq.parse().ok())
                .collect();
            this_table.sort_unstable(); // 频率表升序排列

            table.push((this_table, path.join("scaling_max_freq")));
        }

        table.reverse();
        table.truncate(2); // 保留后两个集群即可

        let (command_sender, command_receiver) = mpsc::sync_channel(1);
        let pause = Arc::new(AtomicBool::new(false));
        let pause_clone = pause.clone();

        let thread_handle =
            thread::spawn(move || process_freq(table, command_receiver, pause_clone));

        Ok(Self {
            command_sender,
            thread_handle,
            pause,
        })
    }

    fn limit(&self) {
        let _ = self.command_sender.try_send(Command::Limit);
    }

    fn release(&self) {
        let _ = self.command_sender.try_send(Command::Release);
    }

    fn plug_in(&self) -> Result<(), Box<dyn Error>> {
        self.pause.store(false, Ordering::Release);
        self.thread_handle.thread().unpark();
        Ok(())
    }

    fn plug_out(&self) -> Result<(), Box<dyn Error>> {
        self.pause.store(true, Ordering::Release);
        Ok(())
    }
}