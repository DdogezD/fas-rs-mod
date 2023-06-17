use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::thread;

use super::Command;
use super::FrequencyTable;

struct CpuFreq {
    table: FrequencyTable,
    path: PathBuf,
    pos: usize,
}

impl CpuFreq {
    fn new(table: FrequencyTable, write_path: PathBuf) -> Self {
        Self {
            pos: table.len(),
            table,
            path: write_path,
        }
    }

    fn prev(&mut self) {
        if self.pos > 0 {
            self.pos -= 1;
            self.write();
        }
    }

    fn next(&mut self) {
        if self.pos < self.table.len() {
            self.pos += 1;
            self.write();
        }
    }

    fn reset(&mut self) {
        self.pos = self.table.len();
        self.write();
    }

    fn write(&self) {
        let value = self.table[self.pos].to_string();
        let _ = fs::write(&self.path, value);
    }
}

enum Mode {
    Single(CpuFreq),
    Double([CpuFreq; 2]),
}

enum Status {
    OnLeft,
    OnRight,
}

impl Status {
    fn swap(self) -> Self {
        match self {
            Self::OnLeft => Self::OnRight,
            Self::OnRight => Self::OnLeft,
        }
    }
}

pub(super) fn process_freq(
    mut tables: Vec<(FrequencyTable, PathBuf)>,
    command_receiver: Receiver<Command>,
    pause: Arc<AtomicBool>,
) {
    let mut status = None;
    let mut cpufreq = if tables.len() > 1 {
        let table = tables.remove(0);
        let freq_a = CpuFreq::new(table.0, table.1);

        let table = tables.remove(0);
        let freq_b = CpuFreq::new(table.0, table.1);

        status = Some(Status::OnLeft);

        Mode::Double([freq_a, freq_b])
    } else {
        let table = tables.remove(0);
        let freq = CpuFreq::new(table.0, table.1);

        Mode::Single(freq)
    };

    loop {
        if let Ok(command) = command_receiver.recv() {
            if pause.load(Ordering::Acquire) {
                process_pause(&mut cpufreq);
                thread::park();
                continue;
            }

            match command {
                Command::Stop => {
                    process_pause(&mut cpufreq);
                    return;
                }
                Command::Release => status = process_release(&mut cpufreq, status),
                Command::Limit => status = process_limit(&mut cpufreq, status),
            }
        } else {
            return;
        }
    }
}

fn process_pause(cpufreq: &mut Mode) {
    match cpufreq {
        Mode::Single(cpu) => cpu.reset(),
        Mode::Double(cpus) => {
            for cpu in cpus {
                cpu.reset();
            }
        }
    }
}

fn process_release(cpufreq: &mut Mode, status: Option<Status>) -> Option<Status> {
    match cpufreq {
        Mode::Single(cpu) => {
            cpu.next();
            None
        }
        Mode::Double(cpus) => {
            let status = status.unwrap();
            match status {
                Status::OnLeft => cpus[0].next(),
                Status::OnRight => cpus[1].next(),
            }
            Some(status.swap())
        }
    }
}

fn process_limit(cpufreq: &mut Mode, status: Option<Status>) -> Option<Status> {
    match cpufreq {
        Mode::Single(cpu) => {
            cpu.next();
            None
        }
        Mode::Double(cpus) => {
            let status = status.unwrap().swap();
            match status {
                Status::OnLeft => cpus[0].prev(),
                Status::OnRight => cpus[1].prev(),
            }
            Some(status)
        }
    }
}